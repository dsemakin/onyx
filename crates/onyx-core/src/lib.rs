#![forbid(unsafe_code)]
#![doc = include_str!("../README.md")]

//! # Invariants
//!
//! See `AGENTS.md` at the repository root. The load-bearing ones are: unknown members
//! are preserved rather than dropped, vendor `extensions` are never interpreted, the
//! schema `$id` is configuration rather than identity, and nothing here reads the clock,
//! the network or the filesystem.

mod codec;
mod diary;
mod document;
mod error;
pub mod json;
mod migrate;
mod profile;
mod summary;
pub mod timestamp;
pub mod units;
mod validate;
mod vocabulary;

pub use diary::{Day, FoodEntry, FoodIdentifiers, Nutrients};
pub use document::{Document, Producer, Quantity};
pub use error::{Error, Result};
pub use migrate::{DEMOTED_NAMESPACE, migrate, migrate_with};
pub use profile::{Goal, Measurement, PreferredUnits, Subject};
pub use summary::{DaySummary, summarise};
pub use validate::{Finding, Report, Severity, validate};
pub use vocabulary::{GoalDirection, MealType, Sex, Source};

/// Identifies the format regardless of file name, URL or location.
///
/// A document that does not carry this value is not an ONYX document, whatever its
/// extension or `$schema` says.
pub const FORMAT: &str = "onyx";

/// The identifier this format carried before it was renamed to Onyx.
///
/// One producer shipped exports under the old name, and those files sit on people's
/// devices and in their backups. A reader accepts both; a writer only ever emits
/// [`FORMAT`]. This is a transitional courtesy rather than part of the format, and it is
/// deliberately absent from the conformance corpus so that a new implementer inherits
/// none of this project's renaming history.
pub const LEGACY_FORMAT: &str = "open-nutrition-log";

/// The specification version this build implements.
pub const SPEC_VERSION: &str = "1.0.0";

/// The MAJOR this build understands.
///
/// Consumers accept any MINOR of a MAJOR they know — minor versions only add members,
/// and unknown members are preserved — and refuse a newer MAJOR, whose members may mean
/// something different.
pub const SPEC_MAJOR: u64 = 1;

/// Version of the machine-readable report the CLI and the WebAssembly binding emit.
///
/// One contract, two transports, so it lives here rather than being declared twice.
/// Additive changes only; anything else bumps this.
pub const REPORT_VERSION: i64 = 1;

/// Registered under the vendor tree with the RFC 6839 `+json` structured syntax suffix.
pub const MEDIA_TYPE: &str = "application/vnd.onyx+json";

/// The `$schema` value written into new documents.
///
/// Configuration, not identity — never branch on it, and never compare against it when
/// deciding whether a document is ONYX. Governance is moving off this host, so documents
/// carrying a different value are expected and valid.
pub const DEFAULT_SCHEMA_ID: &str = "https://www.schemastore.org/onyx-v1.json";

/// Reads a document, checking only that it is one and that this build can understand it.
///
/// This is the identity gate, not the validator: a document that parses here may still
/// violate rules the specification states but JSON Schema cannot express, such as
/// nutrients recorded per 100 g rather than for the portion consumed. Full validation
/// lands in M2.
pub fn parse(input: &str) -> Result<Document> {
    let object = match json::parse(input)? {
        json::Value::Object(object) => object,
        // A document is an object. An array or a bare scalar is well-formed JSON and
        // still not an Onyx document.
        _ => {
            return Err(Error::NotOnyx {
                found: "a JSON value that is not an object".to_owned(),
                expected: FORMAT,
            });
        }
    };
    let document = Document::from_object(object);

    if document.format != FORMAT && document.format != LEGACY_FORMAT {
        return Err(Error::NotOnyx {
            found: document.format,
            expected: FORMAT,
        });
    }

    let major = document
        .spec_major()
        .ok_or_else(|| Error::MalformedVersion(document.spec_version.clone()))?;

    if major > SPEC_MAJOR {
        return Err(Error::UnsupportedMajor {
            found: major,
            supported: SPEC_MAJOR,
        });
    }

    Ok(document)
}

/// Serializes a document, preserving every member that arrived through an `extra` object.
///
/// Infallible: every value in a `Document` is representable, so there is no error for a
/// caller to handle.
pub fn to_string_pretty(document: &Document) -> String {
    document.to_json().to_pretty()
}

/// Serializes a document compactly.
pub fn to_string(document: &Document) -> String {
    document.to_json().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A document from a hypothetical later minor: unknown members at every level, an
    /// unrecognised meal type, a micronutrient the four-nutrient v1.0.0 model has no
    /// field for, and a vendor namespace belonging to somebody else.
    const FUTURE: &str = r#"{
      "$schema": "https://example.invalid/onyx/v1/log.schema.json",
      "format": "onyx",
      "specVersion": "1.7.0",
      "exportedAt": "2026-08-10T09:12:00+03:00",
      "timeZone": "Europe/Berlin",
      "producer": { "name": "Later Minor", "quirk": "unknown member" },
      "subject": { "sex": "female", "height": { "value": 165, "unit": "cm" } },
      "goals": [{ "type": "weightDirection", "direction": "loss" }],
      "days": [
        {
          "date": "2026-08-10",
          "mood": "unknown member",
          "entries": [
            {
              "loggedAt": "2026-08-10T08:30:00+03:00",
              "mealType": "brunch",
              "name": "Oats",
              "identifiers": { "gtin": "05011234567890", "fdcId": 169705 },
              "nutrients": {
                "energy": { "value": 389, "unit": "kcal" },
                "selenium": { "value": 34, "unit": "ug" }
              },
              "source": "database"
            }
          ]
        }
      ],
      "bodyMeasurements": [
        { "observedAt": "2026-08-10T07:00:00+03:00", "type": "bodyMass",
          "value": { "value": 80.4, "unit": "kg" } }
      ],
      "hydration": [{ "observedAt": "2026-08-10T09:00:00+03:00" }],
      "extensions": { "com.example.tracker": { "blockVersion": 4 } }
    }"#;

    #[test]
    fn accepts_a_newer_minor_of_a_known_major() {
        let document = parse(FUTURE).expect("a newer minor must be readable");
        assert_eq!(document.spec_major(), Some(1));
    }

    /// `specVersion` is semver, and semver has one spelling per version. `str::parse` would
    /// take a sign and leading zeros, and splitting on dots alone would take two components;
    /// the identity gate and migration used to disagree about exactly these.
    #[test]
    fn the_identity_gate_reads_spec_version_as_strict_semver() {
        let with_version = |version: &str| {
            format!(
                r#"{{ "format": "onyx", "specVersion": "{version}",
                     "exportedAt": "2026-08-14T12:00:00+03:00", "producer": {{ "name": "t" }} }}"#
            )
        };
        for accepted in ["1.0.0", "0.9.0", "1.10.0"] {
            assert!(
                parse(&with_version(accepted)).is_ok(),
                "{accepted} is semver"
            );
        }
        for refused in ["+1.0.0", "01.0.0", "1.0", "1.0.0.0", "1..0", "1.0.0-rc1"] {
            assert!(
                matches!(
                    parse(&with_version(refused)),
                    Err(Error::MalformedVersion(_))
                ),
                "{refused} is not semver-shaped"
            );
        }
    }

    #[test]
    fn types_the_diary() {
        let document = parse(FUTURE).unwrap();
        let day = &document.days()[0];
        assert_eq!(day.date.as_deref(), Some("2026-08-10"));

        let entry = &day.entries.as_ref().unwrap()[0];
        assert_eq!(entry.name.as_deref(), Some("Oats"));
        assert_eq!(entry.source, Some(Source::Database));
        assert_eq!(
            entry.nutrients.as_ref().unwrap().energy,
            Some(Quantity::new(389.0, "kcal"))
        );

        // A barcode is a string: leading zeros are significant, and a numeric type would
        // silently destroy them.
        let identifiers = entry.identifiers.as_ref().unwrap();
        assert_eq!(identifiers.gtin.as_deref(), Some("05011234567890"));
        assert_eq!(identifiers.fdc_id, Some(169705));

        assert_eq!(
            document.body_measurements()[0].kind.as_deref(),
            Some("bodyMass")
        );
        assert_eq!(document.subject.as_ref().unwrap().sex, Some(Sex::Female));
    }

    #[test]
    fn preserves_members_it_does_not_know() {
        let document = parse(FUTURE).unwrap();

        // A top-level member added by a later minor.
        assert!(document.member("hydration").is_some());
        // Unknown members nested inside structs this build does type.
        assert!(
            document
                .producer
                .as_ref()
                .unwrap()
                .extra
                .contains_key("quirk")
        );
        assert!(document.days()[0].extra.contains_key("mood"));
        // A micronutrient, which is the most likely content of 1.1.
        let nutrients = document.days()[0].entries.as_ref().unwrap()[0]
            .nutrients
            .as_ref()
            .unwrap();
        assert!(nutrients.extra.contains_key("selenium"));
    }

    #[test]
    fn keeps_a_vocabulary_value_it_does_not_define() {
        let document = parse(FUTURE).unwrap();
        let entry = &document.days()[0].entries.as_ref().unwrap()[0];

        // Rejecting this would make the format closed in the one place it cannot afford
        // to be. The value is unknown, not invalid.
        let meal_type = entry.meal_type.as_ref().unwrap();
        assert!(!meal_type.is_known());
        assert_eq!(meal_type.as_str(), "brunch");
    }

    #[test]
    fn never_interprets_a_vendor_block() {
        let document = parse(FUTURE).unwrap();
        // Available to whoever owns the namespace, opaque to this crate.
        assert!(document.extension("com.example.tracker").is_some());
        assert!(document.extension("ltd.bein.burnin").is_none());
    }

    #[test]
    fn round_trip_preserves_and_is_idempotent() {
        let once = parse(FUTURE).unwrap();
        let text = to_string_pretty(&once);
        let twice = parse(&text).unwrap();

        // Preservation: nothing was dropped on the way through.
        assert_eq!(once, twice);
        // Idempotence: a second cycle changes nothing.
        assert_eq!(text, to_string_pretty(&twice));
    }

    #[test]
    fn round_trip_loses_no_json_member_at_any_depth() {
        // Structural proof rather than a field-by-field assertion: every member of the
        // input must still be reachable in the output, whatever this build knows about
        // it. This is the test that would catch a struct added later without a flattened
        // `extra` map.
        let text = to_string_pretty(&parse(FUTURE).unwrap());

        let before = json::parse(FUTURE).unwrap();
        let after = json::parse(&text).unwrap();

        assert_contained(&before, &after, "");
    }

    /// Asserts every member of `before` survives in `after`, at any depth.
    fn assert_contained(before: &json::Value, after: &json::Value, path: &str) {
        match before {
            json::Value::Object(members) => {
                let after = after
                    .as_object()
                    .unwrap_or_else(|| panic!("{path} stopped being an object"));
                for (key, value) in members.iter() {
                    let found = after
                        .get(key)
                        .unwrap_or_else(|| panic!("{path}/{key} was dropped"));
                    assert_contained(value, found, &format!("{path}/{key}"));
                }
            }
            json::Value::Array(items) => {
                let after = after
                    .as_array()
                    .unwrap_or_else(|| panic!("{path} stopped being an array"));
                assert_eq!(items.len(), after.len(), "{path} changed length");
                for (index, item) in items.iter().enumerate() {
                    assert_contained(item, &after[index], &format!("{path}/{index}"));
                }
            }
            scalar => assert_eq!(scalar, after, "{path} changed value"),
        }
    }

    #[test]
    fn identity_comes_from_format_not_from_schema_url() {
        // An unrecognised `$schema` is expected and valid: governance moves hosts, and
        // documents already in the wild point at more than one.
        let document = parse(FUTURE).unwrap();
        assert_eq!(
            document.schema.as_deref(),
            Some("https://example.invalid/onyx/v1/log.schema.json")
        );
        assert_eq!(document.format, FORMAT);
    }

    #[test]
    fn refuses_a_newer_major_rather_than_misreading_it() {
        let text = FUTURE.replace("\"1.7.0\"", "\"2.0.0\"");
        assert!(matches!(
            parse(&text),
            Err(Error::UnsupportedMajor {
                found: 2,
                supported: 1
            })
        ));
    }

    #[test]
    fn still_reads_documents_written_under_the_former_name() {
        // Burnin shipped exports before the rename. Those files exist on real devices and
        // must keep opening; nothing else about them changed.
        let text = FUTURE.replace("\"format\": \"onyx\"", "\"format\": \"open-nutrition-log\"");
        let document = parse(&text).expect("a document under the former name must still read");
        assert_eq!(document.days().len(), 1);
    }

    #[test]
    fn rejects_an_object_that_is_not_onyx() {
        let text = FUTURE.replace("\"format\": \"onyx\"", "\"format\": \"some-other-format\"");
        assert!(matches!(parse(&text), Err(Error::NotOnyx { .. })));
    }

    #[test]
    fn a_document_with_only_the_required_members_is_well_formed() {
        // Everything except the four required members is optional. A file carrying
        // nothing but names, times and calories is a valid, useful log.
        let minimal = r#"{
          "format": "onyx",
          "specVersion": "1.0.0",
          "exportedAt": "2026-08-10T09:12:00+03:00",
          "producer": { "name": "Minimal" }
        }"#;
        let document = parse(minimal).unwrap();
        assert!(document.days().is_empty());
        assert!(document.body_measurements().is_empty());
        // Absent sections must not be materialised as empty ones on the way out.
        assert!(!to_string(&document).contains("days"));
    }

    #[test]
    fn malformed_input_returns_an_error_rather_than_panicking() {
        assert!(matches!(parse("{ not json"), Err(Error::Json(_))));
        assert!(parse("[]").is_err());
        assert!(parse("").is_err());
    }
}
