//! The diary itself: days and the entries inside them.

use crate::json::Object;

use crate::document::Quantity;
use crate::vocabulary::{MealType, Source};

/// One local day of the diary.
#[derive(Debug, Clone, PartialEq)]
pub struct Day {
    /// A **local calendar date**, `YYYY-MM-DD`.
    ///
    /// This is the day the subject considers the entry to belong to, which is not always
    /// derivable from an instant — a meal logged at 01:00 may belong to the previous day
    /// in the user's mind. Producers must never compute this by converting a timestamp
    /// to UTC.
    pub date: Option<String>,

    /// The energy target that applied on this day.
    pub energy_target: Option<Quantity>,

    /// Total energy consumed **as recorded**, which may legitimately differ from the sum
    /// of `entries`. A consumer should not silently "correct" it.
    pub energy_consumed: Option<Quantity>,

    pub entries: Option<Vec<FoodEntry>>,

    pub note: Option<String>,

    pub extra: Object,
}

/// A single thing eaten.
///
/// Everything except `logged_at` is optional, by design: a file carrying nothing but
/// names, times and calories is a valid and useful log. A consumer should degrade
/// rather than discard — dropping an entry because it lacks the one field the consumer
/// happens to build its UI around loses the user's data to satisfy an implementation
/// detail.
#[derive(Debug, Clone, PartialEq)]
pub struct FoodEntry {
    /// RFC 3339 with an explicit offset.
    pub logged_at: Option<String>,

    pub meal_type: Option<MealType>,

    /// Plain text. The fallback identity when no identifier resolves, and the reason
    /// this format exists at all — HealthKit has no food name field, so item identity is
    /// lost the moment a diary passes through it.
    pub name: Option<String>,

    pub identifiers: Option<FoodIdentifiers>,

    /// The amount consumed. See [`FoodIdentifiers`] for the identity story; this is the
    /// portion story, and v1.0.0 leaves its meaning under-specified — a dimensionless
    /// `1` counts `serving_description` units, anything dimensional is a real measure.
    /// Section 3.4 defines this normatively.
    pub quantity: Option<Quantity>,

    pub serving_description: Option<String>,

    pub nutrients: Option<Nutrients>,

    pub source: Option<Source>,

    /// 0..1 confidence in the nutrition data. Only meaningful when `source` is
    /// `Estimated`.
    pub confidence: Option<f64>,

    pub note: Option<String>,

    pub extra: Object,
}

/// Identifiers for a food, any subset.
///
/// Food identity is a bag, not a key: a consumer resolves whichever scheme it knows and
/// falls back to the plain-text name when none of them resolve.
#[derive(Debug, Clone, PartialEq)]
pub struct FoodIdentifiers {
    /// Barcode: GTIN-8/12/13/14. A string, not a number — leading zeros are significant.
    pub gtin: Option<String>,

    /// USDA FoodData Central id.
    pub fdc_id: Option<i64>,

    /// Open Food Facts product code.
    pub off_id: Option<String>,

    pub extra: Object,
}

/// Energy and macronutrients **for the portion actually consumed**.
///
/// Never per 100 g and never per serving. This is the single most common source of
/// silent error when converting between trackers, which is why the specification states
/// it normatively rather than leaving it to convention.
///
/// v1.0.0 defines four members. Micronutrients are the most likely content of 1.1 and
/// arrive through `extra` in the meantime, so a document carrying them is never
/// degraded by passing through this build.
#[derive(Debug, Clone, PartialEq)]
pub struct Nutrients {
    pub energy: Option<Quantity>,
    pub protein: Option<Quantity>,
    pub carbohydrate: Option<Quantity>,
    pub fat: Option<Quantity>,

    pub extra: Object,
}

/// Sums the energy of the entries that record it, in kilocalories, with the number that
/// contributed.
///
/// The count is deliberately not `entries.len()`. An entry with no `nutrients`, or one
/// whose unit is not an energy code, contributes nothing — and "nothing" must not be
/// mistaken for a recorded zero. Both callers turn on that distinction: [`summarise`]
/// needs to know whether a day has any energy figure at all, and validation needs to
/// avoid comparing a recorded total against the sum of an empty set.
///
/// [`summarise`]: crate::summarise
pub(crate) fn sum_energy_kcal(entries: &[FoodEntry]) -> (f64, usize) {
    let mut total = 0.0;
    let mut counted = 0usize;

    for entry in entries {
        if let Some(kcal) = entry
            .nutrients
            .as_ref()
            .and_then(|n| n.energy.as_ref())
            .and_then(crate::document::Quantity::kcal)
        {
            total += kcal;
            counted += 1;
        }
    }

    (total, counted)
}
