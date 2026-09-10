use crate::json::{Object, Value};

use crate::diary::Day;
use crate::profile::{Goal, Measurement, Subject};

/// A value with a UCUM unit code. Never a bare number whose unit is implied by the field
/// name, and never an invented string like `"grams"`.
///
/// # Why both members are optional
///
/// The schema requires `value` and `unit`, so a quantity missing either is malformed — but
/// the engine still has to read the document it was given rather than the one it wishes it
/// had. These were once `f64` and `String`, defaulted to `0.0` and `""` when a member was
/// absent, and that was a data-fabrication bug: `{"unit": "kcal"}` became a *recorded zero*.
/// A day with an unreadable energy value reported as a confident "0 kcal", and a malformed
/// weigh-in became a phantom "0 kg" — then the writer put the invented `0` back on disk.
///
/// `None` means the member was absent or not of the right type. Nothing is invented, the
/// document round-trips as it arrived, and [`validate`](crate::validate) reports it as
/// `quantity/incomplete`.
#[derive(Debug, Clone, PartialEq)]
pub struct Quantity {
    pub value: Option<f64>,
    pub unit: Option<String>,

    pub extra: Object,
}

impl Quantity {
    /// Convenience for building a complete quantity in code.
    pub fn new(value: f64, unit: impl Into<String>) -> Self {
        Self {
            value: Some(value),
            unit: Some(unit.into()),
            extra: Object::new(),
        }
    }

    /// True when both members are present, which is the only case the format defines.
    pub fn is_complete(&self) -> bool {
        self.value.is_some() && self.unit.is_some()
    }

    /// The value and unit together, or `None` if either is missing.
    ///
    /// Every conversion needs both, and reaching for them separately is how a half-read
    /// quantity turns into a number with an assumed unit.
    pub fn parts(&self) -> Option<(f64, &str)> {
        Some((self.value?, self.unit.as_deref()?))
    }

    /// The value in kilocalories, or `None` if the quantity is incomplete or not energy.
    pub fn kcal(&self) -> Option<f64> {
        let (value, unit) = self.parts()?;
        crate::units::to_kcal(value, unit)
    }

    /// The value in kilograms, or `None` if the quantity is incomplete or not a mass.
    pub fn kg(&self) -> Option<f64> {
        let (value, unit) = self.parts()?;
        crate::units::to_kg(value, unit)
    }

    /// The value in grams, or `None` if the quantity is incomplete or not a mass.
    pub fn grams(&self) -> Option<f64> {
        let (value, unit) = self.parts()?;
        crate::units::to_grams(value, unit)
    }
}

/// The application that wrote the document.
#[derive(Debug, Clone, PartialEq)]
pub struct Producer {
    /// Optional because absent and empty are different facts. `put_required` used to write
    /// neither, which meant a producer that wrote `"name": ""` got it deleted — the engine
    /// answering a malformed document by editing it rather than reporting it.
    pub name: Option<String>,
    pub version: Option<String>,
    pub platform: Option<String>,

    pub extra: Object,
}

/// An Onyx document.
///
/// # Preservation
///
/// Every struct in this crate carries an `extra` object holding members this build does
/// not know about. Reading takes the known members out of a working copy and keeps the
/// remainder; writing puts it back. This is not a convenience; it is the property that
/// makes the engine safe to put in a pipeline.
///
/// The specification requires a *consumer* to **ignore** members it does not recognise.
/// A *processor* — anything that reads a document and writes one back out — must go
/// further and **preserve** them. A processor that merely ignored unknown members would
/// silently strip every field added by a newer producer, so passing a 1.7 document
/// through a 1.0 tool would destroy exactly the data the must-ignore rule exists to
/// protect.
///
/// The guarantee this crate makes:
///
/// - **Preservation** — every member present in the input is present in the output with
///   an equal value, at every level of nesting.
/// - **Idempotence** — serializing a parsed document and parsing it again yields an equal
///   document; a second cycle changes nothing.
/// - **Canonical order** — known members are written in the order the specification
///   documents them, followed by unknown members in the relative order they arrived.
///
/// Order is canonical rather than preserved as-read. That was an open question in M0 and
/// is settled deliberately: canonical output makes two exports diffable against each
/// other regardless of which producer wrote them, whereas preserving each producer's
/// quirks would make every cross-tool diff noisy. Since preservation, not byte layout, is
/// the property that protects user data, canonical ordering costs nothing and buys
/// determinism. See `docs/architecture.md`.
#[derive(Debug, Clone, PartialEq)]
pub struct Document {
    /// A pointer to the schema, not an identity. It has moved before and will move again;
    /// never branch on it.
    pub schema: Option<String>,

    /// Identifies the format independently of file name or location.
    pub format: String,

    /// Semver for the specification, not for this crate.
    pub spec_version: String,

    /// RFC 3339 with an explicit offset.
    pub exported_at: Option<String>,

    /// IANA zone name. An offset alone cannot resolve which local day an instant belongs
    /// to, which is exactly how "which day was this meal on" becomes unanswerable.
    pub time_zone: Option<String>,

    /// Absent only in a malformed document; the format requires it.
    pub producer: Option<Producer>,

    pub subject: Option<Subject>,

    pub goals: Option<Vec<Goal>>,

    pub days: Option<Vec<Day>>,

    pub body_measurements: Option<Vec<Measurement>>,

    /// Vendor-private data keyed by reverse-DNS namespace.
    ///
    /// **Opaque by design.** This engine carries these through untouched and never looks
    /// inside. The moment it knows how to interpret any particular application's private
    /// block, the layering that makes ONYX a standard rather than one app's backup
    /// collapses — and the lock-in would then live in the open-source project, which is
    /// worse than having no split at all.
    pub extensions: Option<Object>,

    /// Top-level members this build does not type, preserved verbatim.
    pub extra: Object,
}

/// Reads `specVersion`, which the specification defines as semver: `MAJOR.MINOR.PATCH`,
/// each a canonical decimal.
///
/// Strict on purpose. `str::parse` takes `"+1"` and `"01"`, and `split('.')` alone would
/// accept `"1.0"`; the schema's pattern refuses the first and the third but cannot express
/// the second, and stating the rules the schema cannot is this crate's job. One reader
/// serves the identity gate and migration, so the two cannot disagree about which
/// documents are versioned at all — they did, when each had its own.
pub(crate) fn semver(version: &str) -> Option<(u64, u64, u64)> {
    let mut parts = version.split('.');
    let major = crate::json::canonical_decimal(parts.next()?)?;
    let minor = crate::json::canonical_decimal(parts.next()?)?;
    let patch = crate::json::canonical_decimal(parts.next()?)?;
    if parts.next().is_some() {
        return None;
    }
    Some((major, minor, patch))
}

impl Document {
    /// The MAJOR component of `specVersion`, or `None` when it is not semver-shaped.
    pub fn spec_major(&self) -> Option<u64> {
        semver(&self.spec_version).map(|(major, _, _)| major)
    }

    /// Days in the document, or an empty slice when it carries none.
    pub fn days(&self) -> &[Day] {
        self.days.as_deref().unwrap_or(&[])
    }

    /// Body measurements, or an empty slice when the document carries none.
    pub fn body_measurements(&self) -> &[Measurement] {
        self.body_measurements.as_deref().unwrap_or(&[])
    }

    /// A vendor block by reverse-DNS namespace, without interpreting it.
    ///
    /// Callers that own the namespace may parse the value themselves; this crate never
    /// does.
    pub fn extension(&self, namespace: &str) -> Option<&Value> {
        self.extensions.as_ref()?.get(namespace)
    }

    /// Borrows a top-level member this build does not type — a member added by a later
    /// minor version, for instance.
    pub fn member(&self, name: &str) -> Option<&Value> {
        self.extra.get(name)
    }
}
