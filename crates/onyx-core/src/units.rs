//! UCUM unit codes, grouped by the physical dimension each field expects.
//!
//! The v1.0.0 schema types `unit` by dimension where it can: `energy`, `mass`, `length`,
//! `duration` and `portion` each carry an enum, so `"grams"` in a nutrient is rejected by
//! the schema alone. Two places escape it — a goal's and a body measurement's `value` point
//! at the generic `$defs/quantity`, whose dimension depends on a sibling `type` that is an
//! open vocabulary, so no enum can be written there.
//!
//! That is the gap this module closes. Checking by dimension rather than against one flat
//! list also catches the subtler error the schema could never see: a perfectly valid code
//! in the wrong place, such as energy recorded in `kg`.

/// The physical dimension a field expects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dimension {
    Energy,
    Mass,
    Length,
    Volume,
    /// Elapsed time, as used by `subject.age`.
    Duration,
    /// A count or ratio: UCUM `1` and `%`.
    /// A dimensionless count of servings. §3.4 gives `1` exactly that meaning.
    Count,

    /// `%`. The schema's `$defs/portion` lists it, and §3.4 gives it no meaning at all —
    /// fifty per cent of nothing in particular. Removing it from this list made the engine
    /// reject a document the published schema accepts, which is worse than the gap it
    /// closed: a producer following the schema is not wrong to have believed it. So it is
    /// accepted here and reported as `quantity/undefined-portion-unit` instead.
    Fraction,
}

impl Dimension {
    /// Human name, for findings.
    pub fn name(self) -> &'static str {
        match self {
            Self::Energy => "energy",
            Self::Mass => "mass",
            Self::Length => "length",
            Self::Volume => "volume",
            Self::Duration => "duration",
            Self::Count => "count",
            Self::Fraction => "fraction",
        }
    }

    /// The codes this build accepts for the dimension, for use in a finding's message.
    pub fn codes(self) -> &'static [&'static str] {
        match self {
            Self::Energy => &["kcal", "kJ"],
            Self::Mass => &["g", "mg", "ug", "kg", "[lb_av]", "[oz_av]"],
            Self::Length => &["mm", "cm", "m", "[in_i]", "[ft_i]"],
            Self::Volume => &["mL", "L", "[foz_us]"],
            Self::Duration => &["a", "mo", "d"],
            Self::Count => &["1"],
            Self::Fraction => &["%"],
        }
    }

    /// Whether `unit` is a UCUM code this build recognises for the dimension.
    ///
    /// Case-sensitive on purpose: UCUM codes are, and `"Kcal"` being quietly accepted is
    /// how a vocabulary stops being a vocabulary.
    pub fn accepts(self, unit: &str) -> bool {
        self.codes().contains(&unit)
    }
}

/// A food portion may be a mass, a volume, or a dimensionless count of the servings
/// described by `servingDescription`.
///
/// That third case is why v1.0.0 leaves `quantity` under-specified: the schema example
/// shows `{ "value": 100, "unit": "g" }` while a real producer writes
/// `{ "value": 2, "unit": "1" }` alongside `"2 slices"`, and nothing states which is
/// meant. Section 3.4 now defines it: a dimensional unit is the amount directly, and the
/// dimensionless code `1` is a count of the servings named by `servingDescription`.
pub const PORTION_DIMENSIONS: &[Dimension] = &[
    Dimension::Mass,
    Dimension::Volume,
    Dimension::Count,
    Dimension::Fraction,
];

/// Energy in kilocalories, whatever the document recorded it in.
pub fn to_kcal(value: f64, unit: &str) -> Option<f64> {
    match unit {
        "kcal" => Some(value),
        // The thermochemical calorie, which is the definition nutrition labelling uses.
        "kJ" => Some(value / 4.184),
        _ => None,
    }
}

/// Mass in kilograms, whatever the document recorded it in.
///
/// A consumer must convert rather than trust the number: a document is free to record
/// pounds while its reader displays kilograms, and Principle 5 exists precisely so that
/// this conversion is always possible.
pub fn to_kg(value: f64, unit: &str) -> Option<f64> {
    match unit {
        "kg" => Some(value),
        "g" => Some(value / 1_000.0),
        "mg" => Some(value / 1_000_000.0),
        "ug" => Some(value / 1_000_000_000.0),
        "[lb_av]" => Some(value * 0.453_592_37),
        "[oz_av]" => Some(value * 0.028_349_523_125),
        _ => None,
    }
}

/// Mass in grams. Convenience for the macronutrient arithmetic.
pub fn to_grams(value: f64, unit: &str) -> Option<f64> {
    to_kg(value, unit).map(|kg| kg * 1_000.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_an_invented_unit_string() {
        assert!(!Dimension::Mass.accepts("grams"));
        assert!(!Dimension::Mass.accepts("lbs"));
        assert!(Dimension::Mass.accepts("g"));
    }

    #[test]
    fn rejects_a_valid_code_in_the_wrong_dimension() {
        // `kg` is a perfectly good UCUM code and completely wrong for energy.
        assert!(!Dimension::Energy.accepts("kg"));
        assert!(Dimension::Energy.accepts("kcal"));
    }

    #[test]
    fn is_case_sensitive_because_ucum_is() {
        assert!(!Dimension::Energy.accepts("Kcal"));
        assert!(!Dimension::Energy.accepts("KJ"));
        assert!(Dimension::Energy.accepts("kJ"));
    }

    #[test]
    fn converts_energy_and_mass() {
        assert_eq!(to_kcal(100.0, "kcal"), Some(100.0));
        let from_kj = to_kcal(418.4, "kJ").unwrap();
        assert!((from_kj - 100.0).abs() < 1e-9);

        let pounds = to_kg(177.5, "[lb_av]").unwrap();
        assert!((pounds - 80.512_645_675).abs() < 1e-9);
        assert_eq!(to_kg(500.0, "g"), Some(0.5));
        assert_eq!(to_kg(1.0, "stone"), None);
    }
}
