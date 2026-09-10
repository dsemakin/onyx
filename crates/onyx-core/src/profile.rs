//! The subject, their goals, and body measurements.

use crate::json::Object;

use crate::document::Quantity;
use crate::vocabulary::{GoalDirection, Sex};

/// Who the diary belongs to. Everything is optional.
#[derive(Debug, Clone, PartialEq)]
pub struct Subject {
    pub sex: Option<Sex>,

    /// UCUM `a` (years).
    ///
    /// Age rather than birth date, because age is what a tracker typically stores — but
    /// it silently goes out of date, so a future minor should add an optional
    /// `birthDate`. Recorded here as a known limitation rather than a design choice.
    pub age: Option<Quantity>,

    pub height: Option<Quantity>,

    pub preferred_units: Option<PreferredUnits>,

    pub extra: Object,
}

/// Units the subject prefers to *read*.
///
/// Explicitly **not** the units used in this file. A document may record kilograms while
/// its owner reads pounds; a consumer that conflates the two will convert twice.
#[derive(Debug, Clone, PartialEq)]
pub struct PreferredUnits {
    pub mass: Option<String>,
    pub energy: Option<String>,

    pub extra: Object,
}

/// A goal. `kind` is an open string rather than a vocabulary, since v1.0.0 gives only
/// two examples and the set is expected to grow.
#[derive(Debug, Clone, PartialEq)]
pub struct Goal {
    /// `targetWeight`, `weightDirection`, or something a later version defines.
    pub kind: Option<String>,

    pub value: Option<Quantity>,

    pub direction: Option<GoalDirection>,

    pub extra: Object,
}

/// An observation about the body, kept as its own series rather than as a property of a
/// day.
///
/// A weigh-in is not a food event, and a subject may record several in one day. Consumers
/// fold these onto local days using **the offset `observed_at` itself carries** — never the
/// reader's own zone, and never the document's `timeZone`, which is the subject's home zone
/// and may not be where they were standing. §3.6 is explicit about this, and
/// `corpus/consumer/measurement-folds-by-its-own-offset` is the case that tells the three
/// apart: a Berlin subject weighing in at 00:30 in Tokyo belongs to the later day.
#[derive(Debug, Clone, PartialEq)]
pub struct Measurement {
    /// RFC 3339 with an explicit offset.
    pub observed_at: Option<String>,

    /// `bodyMass` is the only type v1.0.0 defines. Left an open string so a later
    /// version can add others without this build refusing the document.
    pub kind: Option<String>,

    /// Optional for the same reason a [`Quantity`]'s members are: a measurement without one
    /// is malformed, and the answer to that is to report it, not to invent a 0 kg weigh-in.
    pub value: Option<Quantity>,

    pub extra: Object,
}
