//! Semantic validation: the rules the specification states and JSON Schema cannot check.
//!
//! Nine rules in v1.0.0 are machine-enforced today and all nine are structural. Every
//! semantic rule — nutrients being per-portion, a local date not derived via UTC, units
//! being real UCUM codes — is honour-system, as is the entire consumer half of §5. This
//! module exists to move those out of that column.
//!
//! It does **not** check document shape; that is the JSON Schema's job, and running both
//! is the CLI's job. A document can satisfy the schema completely and still be quietly
//! wrong in every way that matters to someone reading their own history.

use crate::diary::{Day, FoodEntry, Nutrients};
use crate::document::{Document, Quantity};
use crate::timestamp::{CivilDate, Timestamp};
use crate::units::{self, Dimension};
use crate::vocabulary::Source;

/// How much a finding matters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    /// Lossy, unusual, or worth knowing. Not a defect.
    Info,
    /// Almost certainly a bug, but the specification permits it.
    Warning,
    /// A violation of the specification.
    Error,
}

impl Severity {
    pub fn name(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warning => "warning",
            Self::Error => "error",
        }
    }
}

/// One problem, located precisely enough to act on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub severity: Severity,
    /// A stable identifier, so tooling can filter without matching on prose.
    pub rule: &'static str,
    /// RFC 6901 JSON Pointer to the offending member.
    pub path: String,
    pub message: String,
}

/// Everything found in one document.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Report {
    pub findings: Vec<Finding>,
}

impl Report {
    /// Whether the document violates the specification.
    ///
    /// Warnings do not make a document non-conforming: several of them flag things the
    /// specification explicitly permits but which are usually mistakes, and refusing
    /// those would make the validator wrong rather than strict.
    pub fn is_conforming(&self) -> bool {
        !self.findings.iter().any(|f| f.severity == Severity::Error)
    }

    pub fn count(&self, severity: Severity) -> usize {
        self.findings
            .iter()
            .filter(|f| f.severity == severity)
            .count()
    }

    /// Findings for one rule, for tests and for tooling that cares about a single check.
    pub fn by_rule(&self, rule: &str) -> impl Iterator<Item = &Finding> {
        self.findings.iter().filter(move |f| f.rule == rule)
    }

    fn push(&mut self, severity: Severity, rule: &'static str, path: String, message: String) {
        self.findings.push(Finding {
            severity,
            rule,
            path,
            message,
        });
    }
}

/// Checks a document against the rules the schema cannot express.
pub fn validate(document: &Document) -> Report {
    let mut report = Report::default();

    check_document(document, &mut report);
    check_subject(document, &mut report);
    check_goals(document, &mut report);
    check_days(document, &mut report);
    check_measurements(document, &mut report);

    report
}

// ── document level ───────────────────────────────────────────────────────────

fn check_document(document: &Document, report: &mut Report) {
    if let Some(exported_at) = &document.exported_at {
        check_timestamp(exported_at, "/exportedAt", report);
    }

    let Some(zone) = &document.time_zone else {
        report.push(
            Severity::Warning,
            "document/missing-timezone",
            "/timeZone".into(),
            "No IANA time zone. Measurements fold by the offset each timestamp carries, so this is not needed for that — but without it a consumer cannot tell which zone the subject considers home, and §2.6 asks for one."
                .into(),
        );
        return;
    };

    // A real zone database is not available here and embedding one would bloat the wasm
    // build, so this checks the shape rather than membership. That is enough for the
    // failure modes that actually occur: a bare offset, a Windows display name, or an
    // abbreviation standing in for a zone.
    let plausible = zone == "UTC"
        || (zone.contains('/')
            && zone.chars().all(|c| {
                c.is_ascii_alphanumeric() || c == '/' || c == '_' || c == '-' || c == '+'
            }));

    if !plausible {
        report.push(
            Severity::Warning,
            "document/suspicious-timezone",
            "/timeZone".into(),
            format!(
                "{zone:?} does not look like an IANA zone name such as Europe/Berlin. \
                 Offsets and abbreviations cannot resolve which local day an instant \
                 belongs to."
            ),
        );
    }
}

fn check_subject(document: &Document, report: &mut Report) {
    let Some(subject) = &document.subject else {
        return;
    };

    check_quantity(
        subject.age.as_ref(),
        &[Dimension::Duration],
        "/subject/age",
        report,
    );
    check_quantity(
        subject.height.as_ref(),
        &[Dimension::Length],
        "/subject/height",
        report,
    );

    if let Some(sex) = &subject.sex {
        if !sex.is_known() {
            report.push(
                Severity::Info,
                "vocabulary/unknown",
                "/subject/sex".into(),
                format!(
                    "{:?} is outside the v1.0.0 vocabulary. Section 3.5 requires a consumer \
                     to treat it as absent rather than as an error, so this is a note \
                     rather than a defect.",
                    sex.as_str()
                ),
            );
        }
    }
}

/// Goals were previously not checked at all — `docs/architecture.md` claimed otherwise —
/// so a target weight in `"grams"`, or a negative one, passed in silence.
///
/// `kind` is an open string, so the dimension is only known for the kinds v1.0.0 names.
/// For anything else the quantity is still checked for completeness and sign, which hold
/// whatever the goal turns out to mean; the unit is left alone rather than guessed at.
fn check_goals(document: &Document, report: &mut Report) {
    for (index, goal) in document.goals.as_deref().unwrap_or(&[]).iter().enumerate() {
        let base = format!("/goals/{index}");

        if let Some(direction) = &goal.direction {
            if !direction.is_known() {
                report.push(
                    Severity::Info,
                    "vocabulary/unknown",
                    format!("{base}/direction"),
                    format!(
                        "{:?} is not a direction v1.0.0 defines; §3.5 says to treat it as absent.",
                        direction.as_str()
                    ),
                );
            }
        }

        match goal.kind.as_deref().unwrap_or_default() {
            "targetWeight" => check_quantity(
                goal.value.as_ref(),
                &[Dimension::Mass],
                &format!("{base}/value"),
                report,
            ),
            // Recognised, but its dimension is not one this build can name. It still gets
            // the checks that hold whatever the goal means — a recognised value must never
            // be examined less closely than an unrecognised one, which falls through below
            // and *is* checked.
            "weightDirection" => {
                check_quantity(goal.value.as_ref(), &[], &format!("{base}/value"), report)
            }
            other => {
                report.push(
                    Severity::Info,
                    "vocabulary/unknown",
                    format!("{base}/type"),
                    format!(
                        "{other:?} is not a goal type v1.0.0 defines; a consumer should skip it rather than reject the document."
                    ),
                );
                check_quantity(goal.value.as_ref(), &[], &format!("{base}/value"), report);
            }
        }
    }
}

fn check_measurements(document: &Document, report: &mut Report) {
    for (index, measurement) in document.body_measurements().iter().enumerate() {
        let base = format!("/bodyMeasurements/{index}");
        if let Some(observed_at) = &measurement.observed_at {
            check_timestamp(observed_at, &format!("{base}/observedAt"), report);
        }

        if measurement.kind.as_deref() != Some("bodyMass") {
            report.push(
                Severity::Info,
                "vocabulary/unknown",
                format!("{base}/type"),
                format!(
                    "{:?} is not a measurement type v1.0.0 defines; a consumer should \
                     skip it rather than reject the document.",
                    measurement.kind.as_deref().unwrap_or_default()
                ),
            );
            // Unknown type, so the dimension is unknown — but completeness and sign are not.
            check_quantity(
                measurement.value.as_ref(),
                &[],
                &format!("{base}/value"),
                report,
            );
            continue;
        }

        check_quantity(
            measurement.value.as_ref(),
            &[Dimension::Mass],
            &format!("{base}/value"),
            report,
        );
    }
}

// ── days and entries ─────────────────────────────────────────────────────────

fn check_days(document: &Document, report: &mut Report) {
    // A map, not a scan. This compared every day against every earlier day, which is the
    // same quadratic shape `json.rs`'s `Object` was fixed for — 32k members went from 2.1 s
    // to 16 ms there — and a diary of a few thousand days is not a strange thing to hold.
    let mut seen: std::collections::HashMap<CivilDate, usize> = std::collections::HashMap::new();
    // Gathered across every day so a systematic pattern can be judged at the end.
    let mut drifts: Vec<i64> = Vec::new();

    for (index, day) in document.days().iter().enumerate() {
        let base = format!("/days/{index}");

        let raw_date = day.date.as_deref().unwrap_or_default();
        let date = match CivilDate::parse(raw_date) {
            Some(date) => {
                if let Some(first) = seen.get(&date) {
                    report.push(
                        Severity::Error,
                        "day/duplicate-date",
                        format!("{base}/date"),
                        format!(
                            "{} already appears at /days/{first}. Two records for one \
                             local day leave a consumer no way to decide which is the \
                             day's history.",
                            raw_date
                        ),
                    );
                }
                seen.insert(date, index);
                Some(date)
            }
            None => {
                // Rendered from the raw value, not with `{:?}` on the Option, which printed
                // `Some("2026-02-30")` to whoever read the report.
                let message = match &day.date {
                    Some(raw) => {
                        format!("{raw:?} is not a real calendar date in YYYY-MM-DD form.")
                    }
                    None => "This day has no date. §3.2 requires one in YYYY-MM-DD form.".into(),
                };
                report.push(
                    Severity::Error,
                    "day/malformed-date",
                    format!("{base}/date"),
                    message,
                );
                None
            }
        };

        check_quantity(
            day.energy_target.as_ref(),
            &[Dimension::Energy],
            &format!("{base}/energyTarget"),
            report,
        );
        check_quantity(
            day.energy_consumed.as_ref(),
            &[Dimension::Energy],
            &format!("{base}/energyConsumed"),
            report,
        );

        let entries = day.entries.as_deref().unwrap_or(&[]);
        for (position, entry) in entries.iter().enumerate() {
            check_entry(
                entry,
                date.as_ref(),
                &format!("{base}/entries/{position}"),
                report,
            );

            // Parsed again without reporting: check_entry already surfaced anything
            // wrong with this timestamp, and the tally only needs the drift.
            if let (Some(stamp), Some(day)) =
                (entry.logged_at.as_deref().and_then(Timestamp::parse), date)
            {
                drifts.push(stamp.date.days_since(&day));
            }
        }

        check_day_total(day, entries, &base, report);
    }

    check_systematic_drift(&drifts, report);
}

/// `energyConsumed` may legitimately differ from the sum of entries — the specification
/// says so — but a large gap usually means entries were dropped in a conversion, so it is
/// worth surfacing without calling it an error.
fn check_day_total(day: &Day, entries: &[FoodEntry], base: &str, report: &mut Report) {
    let Some(recorded) = &day.energy_consumed else {
        return;
    };
    let Some(recorded_kcal) = recorded.kcal() else {
        return;
    };

    let (summed, counted) = crate::diary::sum_energy_kcal(entries);
    if counted == 0 {
        return;
    }

    let gap = (summed - recorded_kcal).abs();
    if gap > 1.0 && gap > recorded_kcal.max(summed) * 0.02 {
        report.push(
            Severity::Info,
            "day/energy-mismatch",
            format!("{base}/energyConsumed"),
            format!(
                "Recorded as {recorded_kcal:.0} kcal but the entries sum to {summed:.0}. \
                 The specification permits this, so it is reported rather than rejected."
            ),
        );
    }
}

/// Judges whether one-day gaps across the whole document look like a systematic error.
///
/// A single entry sitting one day from the date it is filed under is legitimate — §3.2
/// says so, and a 01:00 meal genuinely belongs to the previous evening in the subject's
/// mind. What is not legitimate is many of them all leaning the same way: that is the
/// signature of a producer deriving the local date by converting a timestamp to UTC,
/// which §3.2 forbids and which no single entry can demonstrate on its own.
///
/// This is the kind of rule only a purpose-built engine can carry. A JSON Schema sees one
/// member at a time; the defect only exists in the shape of the whole diary.
fn check_systematic_drift(drifts: &[i64], report: &mut Report) {
    let total = drifts.len();
    if total < 4 {
        return;
    }

    let behind = drifts.iter().filter(|drift| **drift == -1).count();
    let ahead = drifts.iter().filter(|drift| **drift == 1).count();
    let (count, direction) = if behind >= ahead {
        (behind, "the day before")
    } else {
        (ahead, "the day after")
    };

    // A quarter of the diary leaning one way is well past coincidence; a few late dinners
    // in a long diary is not.
    if count >= 4 && count * 4 >= total {
        report.push(
            Severity::Warning,
            "time/systematic-day-drift",
            "/days".into(),
            format!(
                "{count} of {total} entries fall on {direction} the day they are filed \
                 under. One such entry is a late-night meal; this many, all leaning the \
                 same way, is what deriving a local date from a UTC instant looks like."
            ),
        );
    }
}

fn check_entry(entry: &FoodEntry, day: Option<&CivilDate>, base: &str, report: &mut Report) {
    let stamp = entry
        .logged_at
        .as_deref()
        .and_then(|raw| check_timestamp(raw, &format!("{base}/loggedAt"), report));

    // The rule §3.2 states — a local date must never be derived by converting a timestamp
    // to UTC — cannot be checked against a date alone. It can be checked against the
    // timestamp's own local date, which is exactly what an RFC 3339 offset preserves.
    if let (Some(stamp), Some(day)) = (stamp, day) {
        let drift = stamp.date.days_since(day);
        match drift.abs() {
            0 => {}
            // §3.2 is explicit that a meal logged at 01:00 may belong to the previous day
            // in the subject's mind, so one day out is legitimate and merely noted.
            1 => report.push(
                Severity::Info,
                "time/day-mismatch",
                format!("{base}/loggedAt"),
                format!(
                    "Logged on {} but filed under {day}. One day out is legitimate for a \
                     late-night meal; more than one is not.",
                    stamp.date
                ),
            ),
            _ => report.push(
                Severity::Error,
                "time/day-mismatch",
                format!("{base}/loggedAt"),
                format!(
                    "Logged on {} but filed under {day}, {drift} days apart. A local date \
                     derived by converting a timestamp to UTC drifts exactly like this.",
                    stamp.date
                ),
            ),
        }
    }

    check_quantity(
        entry.quantity.as_ref(),
        units::PORTION_DIMENSIONS,
        &format!("{base}/quantity"),
        report,
    );

    // The schema's `$defs/portion` permits `%`, and §3.4 defines only two things: a
    // dimensional amount, or `1` as a count of the servings `servingDescription` names.
    // Fifty per cent of an unnamed something is not an amount anyone can act on, so it is
    // reported — but not rejected, because the published schema says it is allowed and a
    // producer that believed the schema was not being careless.
    if let Some(quantity) = &entry.quantity {
        if quantity.unit.as_deref() == Some("%") {
            report.push(
                Severity::Warning,
                "quantity/undefined-portion-unit",
                format!("{base}/quantity/unit"),
                "`%` is listed by the schema but §3.4 gives it no meaning for a portion: it names no whole for the percentage to be of. Record the amount directly, or use `1` with a `servingDescription`."
                    .into(),
            );
        }
    }

    if let Some(meal_type) = &entry.meal_type {
        if !meal_type.is_known() {
            report.push(
                Severity::Info,
                "vocabulary/unknown",
                format!("{base}/mealType"),
                format!(
                    "{:?} is outside the v1.0.0 vocabulary. Section 3.5 requires a consumer \
                     to treat it as absent rather than as an error, so this is a note \
                     rather than a defect.",
                    meal_type.as_str()
                ),
            );
        }
    }

    // §3.5: confidence is only meaningful for an estimate. Anywhere else it is either
    // noise or a sign that `source` was lost in a conversion.
    if let Some(source) = &entry.source {
        if !source.is_known() {
            report.push(
                Severity::Info,
                "vocabulary/unknown",
                format!("{base}/source"),
                format!(
                    "{:?} is not a source v1.0.0 defines; §3.5 says to treat it as absent.",
                    source.as_str()
                ),
            );
        }
    }

    // §3.5 states the range as 0..1. An out-of-range value is usually a percentage that
    // was never divided down, which silently misweights anything that trusts the number.
    if let Some(confidence) = entry.confidence {
        if !(0.0..=1.0).contains(&confidence) {
            report.push(
                Severity::Error,
                "entry/confidence-range",
                format!("{base}/confidence"),
                format!("confidence is {confidence}, and §3.5 defines it on 0..1."),
            );
        }
    }

    if entry.confidence.is_some() && entry.source != Some(Source::Estimated) {
        report.push(
            Severity::Warning,
            "entry/confidence-without-estimate",
            format!("{base}/confidence"),
            format!(
                "confidence is set but source is {}, and §3.5 gives it meaning only for \
                 an estimate.",
                entry.source.as_ref().map_or("absent", |s| s.as_str())
            ),
        );
    }

    if let Some(nutrients) = &entry.nutrients {
        let path = format!("{base}/nutrients");
        check_quantity(
            nutrients.energy.as_ref(),
            &[Dimension::Energy],
            &format!("{path}/energy"),
            report,
        );
        check_quantity(
            nutrients.protein.as_ref(),
            &[Dimension::Mass],
            &format!("{path}/protein"),
            report,
        );
        check_quantity(
            nutrients.carbohydrate.as_ref(),
            &[Dimension::Mass],
            &format!("{path}/carbohydrate"),
            report,
        );
        check_quantity(
            nutrients.fat.as_ref(),
            &[Dimension::Mass],
            &format!("{path}/fat"),
            report,
        );
        check_atwater(nutrients, &path, report);
    }
}

/// Cross-checks energy against its macronutrients.
///
/// §3.3 requires nutrients to be for the portion consumed, never per 100 g. Nothing can
/// prove that from one document, but the arithmetic catches it: macros stated per 100 g
/// beside a per-portion energy produce a sum wildly out of proportion. The same check
/// catches kilojoules mislabelled as kilocalories, which lands near a factor of 4.184.
fn check_atwater(nutrients: &Nutrients, base: &str, report: &mut Report) {
    let (Some(energy), Some(protein), Some(carbohydrate), Some(fat)) = (
        nutrients.energy.as_ref(),
        nutrients.protein.as_ref(),
        nutrients.carbohydrate.as_ref(),
        nutrients.fat.as_ref(),
    ) else {
        return;
    };

    let (Some(kcal), Some(protein_g), Some(carbohydrate_g), Some(fat_g)) = (
        energy.kcal(),
        protein.grams(),
        carbohydrate.grams(),
        fat.grams(),
    ) else {
        // Units already reported by check_quantity; nothing to add.
        return;
    };

    // Below this, label rounding dominates and every comparison is noise.
    if kcal < 50.0 {
        return;
    }

    let predicted = 4.0 * protein_g + 4.0 * carbohydrate_g + 9.0 * fat_g;
    if predicted <= 0.0 {
        return;
    }

    let ratio = predicted / kcal;
    if !(0.75..=1.25).contains(&ratio) {
        report.push(
            Severity::Warning,
            "nutrients/atwater",
            base.into(),
            format!(
                "Macros imply {predicted:.0} kcal but energy says {kcal:.0} — a factor of \
                 {ratio:.2}. Usual causes: macros stated per 100 g beside a per-portion \
                 energy, or kilojoules labelled as kilocalories. Alcohol, which this \
                 format has no member for, can also account for a genuine shortfall."
            ),
        );
    }
}

// ── shared checks ────────────────────────────────────────────────────────────

fn check_quantity(
    quantity: Option<&Quantity>,
    dimensions: &[Dimension],
    path: &str,
    report: &mut Report,
) {
    let Some(quantity) = quantity else {
        return;
    };

    // The schema requires both members. Reporting it here matters because the CLI does not
    // run the schema, so without this an unreadable quantity passes in silence — and it
    // used to pass as a confident zero.
    if !quantity.is_complete() {
        let missing = match (quantity.value.is_none(), quantity.unit.is_none()) {
            (true, true) => "neither `value` nor `unit`",
            (true, false) => "no `value`",
            (false, true) => "no `unit`",
            (false, false) => unreachable!("is_complete() covers this"),
        };
        report.push(
            Severity::Error,
            "quantity/incomplete",
            path.into(),
            format!(
                "This quantity has {missing}. Both are required, so it is reported rather than read as a zero."
            ),
        );
        return;
    }

    let unit = quantity.unit.as_deref().unwrap_or_default();
    let value = quantity.value.unwrap_or_default();

    // An empty list means the caller does not know which dimension applies — an open
    // vocabulary member it has never heard of. Checking sign and completeness is still
    // right; inventing an expectation about the unit is not.
    if !dimensions.is_empty() && !dimensions.iter().any(|d| d.accepts(unit)) {
        let accepted: Vec<&str> = dimensions
            .iter()
            .flat_map(|d| d.codes().iter().copied())
            .collect();
        let named: Vec<&str> = dimensions.iter().map(|d| d.name()).collect();
        report.push(
            Severity::Error,
            "unit/dimension",
            format!("{path}/unit"),
            format!(
                "{:?} is not a UCUM {} code. Expected one of: {}.",
                unit,
                named.join(" or "),
                accepted.join(", ")
            ),
        );
    }

    if value < 0.0 {
        report.push(
            Severity::Warning,
            "quantity/negative",
            format!("{path}/value"),
            format!("{value} is negative, which no quantity in this format should be."),
        );
    }
}

fn check_timestamp(raw: &str, path: &str, report: &mut Report) -> Option<Timestamp> {
    let Some(stamp) = Timestamp::parse(raw) else {
        report.push(
            Severity::Error,
            "time/malformed",
            path.into(),
            format!("{raw:?} is not an RFC 3339 timestamp with an offset."),
        );
        return None;
    };

    if stamp.is_utc_normalised() {
        report.push(
            Severity::Error,
            "time/utc-normalised",
            path.into(),
            "Normalised to UTC with `Z`. The instant survives but the local context does \
             not, and that context is what answers which day a meal belonged to."
                .into(),
        );
    }

    Some(stamp)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a document around one entry, so each test states only what it is about.
    fn with_entry(entry: &str) -> crate::Document {
        let text = format!(
            r#"{{
              "format": "onyx",
              "specVersion": "1.0.0",
              "exportedAt": "2026-08-14T12:00:00+03:00",
              "timeZone": "Europe/Berlin",
              "producer": {{ "name": "Test" }},
              "days": [{{ "date": "2026-08-10", "entries": [{entry}] }}]
            }}"#
        );
        crate::parse(&text).expect("fixture must parse")
    }

    fn rules(report: &Report) -> Vec<&str> {
        report.findings.iter().map(|f| f.rule).collect()
    }

    #[test]
    fn a_clean_document_produces_no_findings() {
        let document = with_entry(
            r#"{
              "loggedAt": "2026-08-10T08:30:00+03:00",
              "name": "Oats",
              "nutrients": {
                "energy": { "value": 389, "unit": "kcal" },
                "protein": { "value": 16.9, "unit": "g" },
                "carbohydrate": { "value": 66, "unit": "g" },
                "fat": { "value": 6.9, "unit": "g" }
              }
            }"#,
        );
        let report = validate(&document);
        assert!(
            report.findings.is_empty(),
            "unexpected: {:?}",
            report.findings
        );
        assert!(report.is_conforming());
    }

    #[test]
    fn rejects_an_invented_unit() {
        let document = with_entry(
            r#"{
              "loggedAt": "2026-08-10T08:30:00+03:00",
              "nutrients": { "protein": { "value": 17, "unit": "grams" } }
            }"#,
        );
        let report = validate(&document);
        assert!(rules(&report).contains(&"unit/dimension"));
        assert!(!report.is_conforming());

        let finding = report.by_rule("unit/dimension").next().unwrap();
        assert_eq!(finding.path, "/days/0/entries/0/nutrients/protein/unit");
    }

    #[test]
    fn rejects_a_real_unit_in_the_wrong_dimension() {
        let document = with_entry(
            r#"{
              "loggedAt": "2026-08-10T08:30:00+03:00",
              "nutrients": { "energy": { "value": 389, "unit": "kg" } }
            }"#,
        );
        assert!(!validate(&document).is_conforming());
    }

    #[test]
    fn catches_per_hundred_gram_macros_beside_a_per_portion_energy() {
        // 30 g of oats: energy is for the portion, macros were left per 100 g. Both halves
        // are individually plausible, which is what makes this the silent error it is.
        let document = with_entry(
            r#"{
              "loggedAt": "2026-08-10T08:30:00+03:00",
              "quantity": { "value": 30, "unit": "g" },
              "nutrients": {
                "energy": { "value": 117, "unit": "kcal" },
                "protein": { "value": 16.9, "unit": "g" },
                "carbohydrate": { "value": 66, "unit": "g" },
                "fat": { "value": 6.9, "unit": "g" }
              }
            }"#,
        );
        let report = validate(&document);
        assert!(rules(&report).contains(&"nutrients/atwater"));
        // A warning, not an error: nothing in one document can prove which half is wrong.
        assert!(report.is_conforming());
    }

    #[test]
    fn catches_kilojoules_labelled_as_kilocalories() {
        let document = with_entry(
            r#"{
              "loggedAt": "2026-08-10T08:30:00+03:00",
              "nutrients": {
                "energy": { "value": 1628, "unit": "kcal" },
                "protein": { "value": 16.9, "unit": "g" },
                "carbohydrate": { "value": 66, "unit": "g" },
                "fat": { "value": 6.9, "unit": "g" }
              }
            }"#,
        );
        assert!(rules(&validate(&document)).contains(&"nutrients/atwater"));
    }

    #[test]
    fn a_late_night_meal_one_day_out_is_noted_not_rejected() {
        // §3.2 is explicit that a 01:00 meal may belong to the previous day in the
        // subject's mind. Flagging this as an error would make the validator wrong.
        let document = with_entry(r#"{ "loggedAt": "2026-08-11T01:00:00+03:00" }"#);
        let report = validate(&document);

        let finding = report.by_rule("time/day-mismatch").next().unwrap();
        assert_eq!(finding.severity, Severity::Info);
        assert!(report.is_conforming());
    }

    #[test]
    fn a_date_that_drifted_further_than_a_day_is_an_error() {
        let document = with_entry(r#"{ "loggedAt": "2026-08-14T09:00:00+03:00" }"#);
        let report = validate(&document);

        let finding = report.by_rule("time/day-mismatch").next().unwrap();
        assert_eq!(finding.severity, Severity::Error);
        assert!(!report.is_conforming());
    }

    #[test]
    fn warns_when_a_timestamp_was_normalised_to_utc() {
        let document = with_entry(r#"{ "loggedAt": "2026-08-10T05:30:00Z" }"#);
        assert!(rules(&validate(&document)).contains(&"time/utc-normalised"));
    }

    #[test]
    fn warns_about_confidence_on_something_that_was_not_estimated() {
        let document = with_entry(
            r#"{
              "loggedAt": "2026-08-10T08:30:00+03:00",
              "source": "barcode",
              "confidence": 0.4
            }"#,
        );
        assert!(rules(&validate(&document)).contains(&"entry/confidence-without-estimate"));
    }

    #[test]
    fn an_unknown_vocabulary_value_is_information_not_a_failure() {
        let document =
            with_entry(r#"{ "loggedAt": "2026-08-10T08:30:00+03:00", "mealType": "brunch" }"#);
        let report = validate(&document);

        let finding = report.by_rule("vocabulary/unknown").next().unwrap();
        assert_eq!(finding.severity, Severity::Info);
        assert!(report.is_conforming());
    }

    #[test]
    fn two_records_for_one_local_day_are_an_error() {
        let document = crate::parse(
            r#"{
              "format": "onyx",
              "specVersion": "1.0.0",
              "exportedAt": "2026-08-14T12:00:00+03:00",
              "timeZone": "Europe/Berlin",
              "producer": { "name": "Test" },
              "days": [{ "date": "2026-08-10" }, { "date": "2026-08-10" }]
            }"#,
        )
        .unwrap();
        assert!(rules(&validate(&document)).contains(&"day/duplicate-date"));
        assert!(!validate(&document).is_conforming());
    }

    #[test]
    fn notices_an_offset_standing_in_for_a_zone_name() {
        let document = crate::parse(
            r#"{
              "format": "onyx",
              "specVersion": "1.0.0",
              "exportedAt": "2026-08-14T12:00:00+03:00",
              "timeZone": "GMT+3",
              "producer": { "name": "Test" }
            }"#,
        )
        .unwrap();
        assert!(rules(&validate(&document)).contains(&"document/suspicious-timezone"));
    }

    #[test]
    fn notes_a_day_total_that_disagrees_with_its_entries() {
        let document = crate::parse(
            r#"{
              "format": "onyx",
              "specVersion": "1.0.0",
              "exportedAt": "2026-08-14T12:00:00+03:00",
              "timeZone": "Europe/Berlin",
              "producer": { "name": "Test" },
              "days": [{
                "date": "2026-08-10",
                "energyConsumed": { "value": 2000, "unit": "kcal" },
                "entries": [{
                  "loggedAt": "2026-08-10T08:30:00+03:00",
                  "nutrients": { "energy": { "value": 389, "unit": "kcal" } }
                }]
              }]
            }"#,
        )
        .unwrap();
        let report = validate(&document);

        // The specification permits the two to differ, so this is reported, not rejected.
        let finding = report.by_rule("day/energy-mismatch").next().unwrap();
        assert_eq!(finding.severity, Severity::Info);
        assert!(report.is_conforming());
    }

    #[test]
    fn many_one_day_gaps_leaning_the_same_way_look_like_utc_derivation() {
        // Each entry is a 01:00 meal filed under the previous day. One of these is a late
        // dinner; four in a row is a producer deriving the date from a UTC instant, which
        // §3.2 forbids and which no single entry could prove.
        let document = crate::parse(
            r#"{
              "format": "onyx",
              "specVersion": "1.0.0",
              "exportedAt": "2026-08-14T12:00:00+03:00",
              "timeZone": "Europe/Berlin",
              "producer": { "name": "Derives From UTC" },
              "days": [
                { "date": "2026-08-10", "entries": [{ "loggedAt": "2026-08-11T01:00:00+03:00" }] },
                { "date": "2026-08-11", "entries": [{ "loggedAt": "2026-08-12T01:00:00+03:00" }] },
                { "date": "2026-08-12", "entries": [{ "loggedAt": "2026-08-13T01:00:00+03:00" }] },
                { "date": "2026-08-13", "entries": [{ "loggedAt": "2026-08-14T01:00:00+03:00" }] }
              ]
            }"#,
        )
        .unwrap();

        let report = validate(&document);
        let finding = report.by_rule("time/systematic-day-drift").next().unwrap();
        assert_eq!(finding.severity, Severity::Warning);
        assert_eq!(finding.path, "/days");
    }

    #[test]
    fn a_few_late_dinners_are_not_a_pattern() {
        // The same shape, but only one entry drifts. This must stay quiet, or the check
        // punishes producers for recording exactly what §3.2 says they may record.
        let document = crate::parse(
            r#"{
              "format": "onyx",
              "specVersion": "1.0.0",
              "exportedAt": "2026-08-14T12:00:00+03:00",
              "timeZone": "Europe/Berlin",
              "producer": { "name": "Ordinary" },
              "days": [
                { "date": "2026-08-10", "entries": [{ "loggedAt": "2026-08-11T01:00:00+03:00" }] },
                { "date": "2026-08-11", "entries": [{ "loggedAt": "2026-08-11T09:00:00+03:00" }] },
                { "date": "2026-08-12", "entries": [{ "loggedAt": "2026-08-12T09:00:00+03:00" }] },
                { "date": "2026-08-13", "entries": [{ "loggedAt": "2026-08-13T09:00:00+03:00" }] }
              ]
            }"#,
        )
        .unwrap();

        let report = validate(&document);
        assert_eq!(report.by_rule("time/systematic-day-drift").count(), 0);
    }
}
