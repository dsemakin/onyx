//! A canonical restoration of a diary, independent of any application's data model.
//!
//! The specification's real test (§5) is whether a consumer can rebuild a usable diary
//! from the **portable layer alone**, with no vendor block present. That is impossible to
//! assert against an arbitrary reader's internal types, so this defines the projection
//! every conforming consumer must agree on regardless of what it stores internally:
//! per local day, how many entries, how much energy, and what the subject weighed.
//!
//! Agreeing on that projection is what makes `corpus/consumer/` checkable by an
//! implementation in any language. It also happens to be the thing most consumers want.

use crate::diary;
use crate::document::Document;
use crate::timestamp::{CivilDate, Timestamp};

/// One local day, reduced to the facts any consumer must recover identically.
#[derive(Debug, Clone, PartialEq)]
pub struct DaySummary {
    /// Local calendar date, `YYYY-MM-DD`.
    pub date: String,
    pub entry_count: usize,
    /// Energy for the day in kilocalories, whatever unit the document used.
    ///
    /// Prefers the recorded `energyConsumed`, which §3.3 allows to differ from the sum of
    /// entries — the recorded figure is what the subject saw, so it wins.
    pub energy_kcal: Option<f64>,
    /// Body mass in kilograms, whatever unit the document used.
    pub body_mass_kg: Option<f64>,
}

/// Reduces a document to per-day facts, folding body measurements onto local days.
///
/// A weigh-in arrives as its own observation series rather than as a property of a day,
/// so it has to be folded. The local day is taken from the timestamp's own offset, which
/// is exactly what RFC 3339 preserves and exactly why §2.6 requires one — no time zone
/// database is needed, and none is consulted.
pub fn summarise(document: &Document) -> Vec<DaySummary> {
    let mut days: Vec<DaySummary> = Vec::new();

    for day in document.days() {
        let entries = day.entries.as_deref().unwrap_or(&[]);

        let recorded = day
            .energy_consumed
            .as_ref()
            .and_then(crate::document::Quantity::kcal);

        let summed = {
            let (total, counted) = diary::sum_energy_kcal(entries);
            (counted > 0).then_some(total)
        };

        days.push(DaySummary {
            date: day.date.clone().unwrap_or_default(),
            entry_count: entries.len(),
            energy_kcal: recorded.or(summed),
            body_mass_kg: None,
        });
    }

    // Which weigh-in currently owns each day, by instant.
    let mut latest: std::collections::HashMap<String, i64> = std::collections::HashMap::new();

    // Every position each date occupies. A date holds more than one when the document has
    // duplicate days, which is an error `summarise` does not itself check for.
    let mut by_date: std::collections::HashMap<String, Vec<usize>> =
        std::collections::HashMap::new();
    for (index, day) in days.iter().enumerate() {
        by_date.entry(day.date.clone()).or_default().push(index);
    }

    for measurement in document.body_measurements() {
        if measurement.kind.as_deref() != Some("bodyMass") {
            continue;
        }
        let Some(stamp) = measurement
            .observed_at
            .as_deref()
            .and_then(Timestamp::parse)
        else {
            continue;
        };
        let Some(kilograms) = measurement
            .value
            .as_ref()
            .and_then(crate::document::Quantity::kg)
        else {
            continue;
        };

        let date = stamp.date.to_string();
        let instant = stamp.instant();

        // Several weigh-ins in one day is normal — people step on and off the scale — so the
        // one that counts is the latest by the clock, not the last in the array.
        //
        // It reaches every day carrying that date, because duplicate dates are an error
        // `summarise` does not itself check for and the wasm `summary` export calls this
        // directly; folding onto only the first would be arbitrary. Looked up rather than
        // rescanned, which is the quadratic shape this crate has already paid for once.
        let matching = by_date.get(&date).cloned().unwrap_or_default();

        if !matching.is_empty() {
            if latest.get(&date).is_none_or(|best| instant >= *best) {
                for index in matching {
                    days[index].body_mass_kg = Some(kilograms);
                }
                latest.insert(date, instant);
            }
            continue;
        }

        // No day carries this date yet. A weigh-in with no food logged still makes the
        // day real — and the index has to learn about it, or a second measurement on the
        // same date finds nothing and pushes a duplicate day.
        by_date.entry(date.clone()).or_default().push(days.len());
        days.push(DaySummary {
            date: date.clone(),
            entry_count: 0,
            energy_kcal: None,
            body_mass_kg: Some(kilograms),
        });
        latest.insert(date, instant);
    }

    days.sort_by_key(|day| CivilDate::parse(&day.date).map_or(i64::MAX, |date| date.day_number()));
    days
}

#[cfg(test)]
mod tests {
    /// People step on the scale more than once a day. The one that counts is the last one
    /// by the clock — not the last one the producer happened to write.
    #[test]
    fn the_latest_weigh_in_of_a_day_wins_whatever_order_it_was_written_in() {
        let text = r#"{
          "format": "onyx",
          "specVersion": "1.0.0",
          "exportedAt": "2026-08-14T09:00:00+02:00",
          "producer": { "name": "Test" },
          "bodyMeasurements": [
            { "observedAt": "2026-08-10T21:00:00+02:00", "type": "bodyMass",
              "value": { "value": 80, "unit": "kg" } },
            { "observedAt": "2026-08-10T07:00:00+02:00", "type": "bodyMass",
              "value": { "value": 82, "unit": "kg" } }
          ]
        }"#;

        let days = crate::summarise(&crate::parse(text).unwrap());
        assert_eq!(days.len(), 1);
        assert_eq!(
            days[0].body_mass_kg,
            Some(80.0),
            "the 07:00 reading sits later in the array and must not win"
        );
    }

    /// Different zones on the same local day still order by instant.
    #[test]
    fn weigh_ins_in_different_zones_still_order_by_instant() {
        let text = r#"{
          "format": "onyx",
          "specVersion": "1.0.0",
          "exportedAt": "2026-08-14T09:00:00+02:00",
          "producer": { "name": "Test" },
          "bodyMeasurements": [
            { "observedAt": "2026-08-10T08:00:00+09:00", "type": "bodyMass",
              "value": { "value": 70, "unit": "kg" } },
            { "observedAt": "2026-08-10T08:00:00+02:00", "type": "bodyMass",
              "value": { "value": 71, "unit": "kg" } }
          ]
        }"#;

        let days = crate::summarise(&crate::parse(text).unwrap());
        assert_eq!(days.len(), 1);
        // 08:00+02:00 is seven hours after 08:00+09:00.
        assert_eq!(days[0].body_mass_kg, Some(71.0));
    }

    /// Two records for one local day is invalid — `day/duplicate-date` is an error — but
    /// `summarise` does not run the validator, and the wasm `summary` export calls it
    /// directly. Folding onto only the first day silently dropped the weigh-in from the
    /// other one, depending on array order.
    #[test]
    fn a_weigh_in_reaches_every_day_carrying_its_date() {
        let text = r#"{
          "format": "onyx",
          "specVersion": "1.0.0",
          "exportedAt": "2026-08-14T09:00:00+02:00",
          "producer": { "name": "Test" },
          "days": [
            { "date": "2026-08-10", "entries": [] },
            { "date": "2026-08-10", "entries": [] }
          ],
          "bodyMeasurements": [
            { "observedAt": "2026-08-10T07:00:00+02:00", "type": "bodyMass",
              "value": { "value": 80, "unit": "kg" } }
          ]
        }"#;

        let days = crate::summarise(&crate::parse(text).unwrap());
        assert_eq!(days.len(), 2, "both records are still days");
        for day in &days {
            assert_eq!(
                day.body_mass_kg,
                Some(80.0),
                "the weigh-in reached only some of the days with its date"
            );
        }
    }
}
