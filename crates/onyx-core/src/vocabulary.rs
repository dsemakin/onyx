//! Open vocabularies.
//!
//! The v1.0.0 JSON Schema declares `mealType`, `source`, `sex` and `direction` as closed
//! `enum`s, which means a producer emitting a value outside the list writes a document
//! that fails validation. That is the one place the format is closed, and it sits
//! awkwardly against the principle that a minor version may add things without breaking
//! existing readers: adding `"brunch"` in 1.1 would invalidate the document for every
//! 1.0 validator in existence.
//!
//! The engine therefore does **not** reject an unrecognised value. It keeps the raw
//! string, reports it as unknown, and writes it back out unchanged. A consumer should
//! treat an unknown vocabulary value as *absent* rather than as an error — the same
//! must-ignore posture the format takes toward unknown members.
//!
//! The schema agrees: these are open strings with examples, not closed enums. The engine
//! and the schema enforce the same rule, which is the point.

/// Generates an open vocabulary: a Rust enum over the known values, plus `Other` for
/// anything else, serialising back to the exact string that came in.
macro_rules! open_vocabulary {
    (
        $(#[$meta:meta])*
        $name:ident { $( $(#[$doc:meta])* $variant:ident => $wire:literal ),+ $(,)? }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub enum $name {
            $( $(#[$doc])* $variant, )+
            /// A value this build does not define. Preserved verbatim; treat as absent
            /// rather than as an error.
            Other(String),
        }

        impl $name {
            /// The wire representation, identical to what was read.
            pub fn as_str(&self) -> &str {
                match self {
                    $( Self::$variant => $wire, )+
                    Self::Other(raw) => raw.as_str(),
                }
            }

            /// Whether this build defines the value.
            pub fn is_known(&self) -> bool {
                !matches!(self, Self::Other(_))
            }
        }

        impl From<String> for $name {
            fn from(raw: String) -> Self {
                match raw.as_str() {
                    $( $wire => Self::$variant, )+
                    _ => Self::Other(raw),
                }
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(self.as_str())
            }
        }

    };
}

open_vocabulary! {
    /// Which meal an entry belongs to.
    MealType {
        Breakfast => "breakfast",
        Lunch => "lunch",
        Dinner => "dinner",
        Snack => "snack",
    }
}

open_vocabulary! {
    /// How the *nutrition data* was obtained — not how it was typed.
    ///
    /// `Estimated` is the only value for which `confidence` is meaningful.
    Source {
        /// The subject entered the numbers.
        Manual => "manual",
        /// Exact product from a scanned barcode.
        Barcode => "barcode",
        /// Matched to a food database entry.
        Database => "database",
        /// Estimated, e.g. by a model, with no database match.
        Estimated => "estimated",
    }
}

open_vocabulary! {
    /// A single field conflating birth sex and gender, because that is what the
    /// nutrition formulas downstream consume. Anything more nuanced belongs in an
    /// extension until there is a real requirement.
    Sex {
        Male => "male",
        Female => "female",
        Other_ => "other",
        Unknown => "unknown",
    }
}

open_vocabulary! {
    /// Direction of a weight goal.
    GoalDirection {
        Loss => "loss",
        Maintain => "maintain",
        Gain => "gain",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_values_map_to_variants() {
        assert_eq!(MealType::from("breakfast".to_owned()), MealType::Breakfast);
        assert!(MealType::Breakfast.is_known());
    }

    #[test]
    fn unknown_values_survive_instead_of_failing() {
        // A later minor, or simply a producer with a richer vocabulary. Rejecting this
        // would make the format closed in the one place it cannot afford to be.
        let brunch = MealType::from("brunch".to_owned());
        assert_eq!(brunch, MealType::Other("brunch".to_owned()));
        assert!(!brunch.is_known());
        assert_eq!(brunch.as_str(), "brunch");
    }

    #[test]
    fn serialisation_returns_the_exact_string_that_came_in() {
        let value = MealType::from("second breakfast".to_owned());
        assert_eq!(value.as_str(), "second breakfast");
    }
}
