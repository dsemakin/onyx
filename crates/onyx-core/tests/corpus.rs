//! Runs every case in `corpus/manifest.json` against the engine.
//!
//! The corpus is plain data and deliberately not Rust-specific: this file is one runner
//! for it, and an implementation in any other language is expected to write its own. What
//! matters is that the cases, the expectations and the rule names live in the repository
//! as JSON, so nobody has to read this file to conform.
//!
//! Every case is attempted before anything fails, so one broken fixture reports itself
//! alongside the others rather than hiding them.

use std::path::{Path, PathBuf};

use onyx_core::json::{self, Value};

fn corpus_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus")
}

/// Floats survive a JSON round trip exactly, but unit conversion does not, so comparisons
/// of converted values need a tolerance rather than equality.
fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-6 * a.abs().max(b.abs()).max(1.0)
}

/// The schema's own `$id`, the constant the engine writes into documents, and the URL the
/// SchemaStore catalog points at must all be the same string.
///
/// They live in three files that are edited at different times for different reasons,
/// which is exactly how a document ends up pointing at a schema that is not the one it was
/// written against.
#[test]
fn the_schema_url_agrees_everywhere() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");

    let schema = json::parse(
        &std::fs::read_to_string(root.join("spec/v1/log.schema.json")).expect("the schema"),
    )
    .expect("the schema is valid JSON");
    let declared = schema["$id"].as_str().expect("the schema declares an $id");

    let catalog = json::parse(
        &std::fs::read_to_string(root.join("scripts/schemastore-catalog-entry.json"))
            .expect("the catalog entry"),
    )
    .expect("the catalog entry is valid JSON");
    let published = catalog["url"].as_str().expect("the catalog names a url");

    assert_eq!(
        declared,
        onyx_core::DEFAULT_SCHEMA_ID,
        "schema $id vs engine constant"
    );
    assert_eq!(declared, published, "schema $id vs SchemaStore catalog url");
}

#[test]
fn every_case_in_the_manifest_holds() {
    let dir = corpus_dir();
    let manifest = json::parse(
        &std::fs::read_to_string(dir.join("manifest.json")).expect("corpus/manifest.json"),
    )
    .expect("manifest is valid JSON");

    let cases = manifest["cases"]
        .as_array()
        .expect("manifest has a `cases` array");
    let mut failures: Vec<String> = Vec::new();

    for case in cases {
        let group = case["group"].as_str().unwrap_or("(missing group)");
        let (name, path) = match (case["file"].as_str(), case["dir"].as_str()) {
            (Some(file), _) => (file.to_owned(), dir.join(file)),
            (None, Some(folder)) => (
                format!("{folder}/document.json"),
                dir.join(folder).join("document.json"),
            ),
            _ => {
                failures.push("a case names neither `file` nor `dir`".to_owned());
                continue;
            }
        };

        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) => {
                failures.push(format!("[{group}] {name}\n    unreadable: {error}"));
                continue;
            }
        };

        if let Err(message) = check_expectations_are_understood(case) {
            failures.push(format!(
                "[{group}] {name}
    {message}"
            ));
            continue;
        }

        let outcome = match group {
            "valid" => check_valid(&text, case),
            "invalid" => check_invalid(&text, case),
            "roundtrip" => check_roundtrip(&text),
            "consumer" => {
                let expected = dir
                    .join(case["dir"].as_str().unwrap())
                    .join("expected.json");
                check_consumer(&text, &expected)
            }
            other => Err(format!("unknown group {other:?}")),
        };

        if let Err(message) = outcome {
            failures.push(format!("[{group}] {name}\n    {message}"));
        }
    }

    assert!(!cases.is_empty(), "the corpus is empty");
    assert!(
        failures.is_empty(),
        "{} of {} corpus cases failed:\n\n{}\n",
        failures.len(),
        cases.len(),
        failures.join("\n\n")
    );
}

/// The document must be accepted, must report no errors, and must report every warning
/// the manifest names.
/// Every `expect` key this runner knows how to check.
///
/// A runner that ignores an expectation it does not implement reports a pass it never
/// established. That is not hypothetical: `notes` was added to the manifest and the Python
/// runner went on ignoring it, so the fixture written to make a message reachable passed
/// against a reader that does nothing of the kind. Each runner declares what it can check
/// and fails on anything else, so the next expectation added to the manifest breaks every
/// implementation that has not implemented it — loudly, which is the point.
const UNDERSTOOD: &[&str] = &["warns", "notes", "rule", "schemaValid", "accepted"];

fn check_expectations_are_understood(case: &Value) -> Result<(), String> {
    let Some(expect) = case["expect"].as_object() else {
        return Ok(());
    };
    for (key, _) in expect.iter() {
        if !UNDERSTOOD.contains(&key) {
            return Err(format!(
                "the manifest expects {key:?}, which this runner does not know how to check"
            ));
        }
    }
    Ok(())
}

fn check_valid(text: &str, case: &Value) -> Result<(), String> {
    let document = onyx_core::parse(text).map_err(|error| format!("rejected: {error}"))?;
    let report = onyx_core::validate(&document);

    let errors: Vec<&str> = report
        .findings
        .iter()
        .filter(|finding| finding.severity == onyx_core::Severity::Error)
        .map(|finding| finding.rule)
        .collect();
    if !errors.is_empty() {
        return Err(format!("expected no errors, got: {}", errors.join(", ")));
    }

    // `notes` exists because a case whose only interesting finding is Info had no way to
    // assert it: `warns: []` iterates zero times and verifies nothing at all, which is how a
    // fixture written specifically to make a message reachable ended up proving nothing.
    for expected in case["expect"]["notes"].as_array().unwrap_or_default() {
        let rule = expected.as_str().unwrap_or_default();
        let found = report
            .findings
            .iter()
            .any(|finding| finding.rule == rule && finding.severity == onyx_core::Severity::Info);
        if !found {
            let saw: Vec<String> = report
                .findings
                .iter()
                .map(|finding| format!("{}({})", finding.rule, finding.severity.name()))
                .collect();
            return Err(format!(
                "expected a note {rule:?}; findings were: {}",
                if saw.is_empty() {
                    "none".to_owned()
                } else {
                    saw.join(", ")
                }
            ));
        }
    }

    for expected in case["expect"]["warns"].as_array().unwrap_or_default() {
        let rule = expected.as_str().unwrap_or_default();
        let found = report.findings.iter().any(|finding| {
            finding.rule == rule && finding.severity == onyx_core::Severity::Warning
        });
        if !found {
            let saw: Vec<String> = report
                .findings
                .iter()
                .map(|finding| format!("{}({})", finding.rule, finding.severity.name()))
                .collect();
            return Err(format!(
                "expected a warning {rule:?}; findings were: {}",
                if saw.is_empty() {
                    "none".to_owned()
                } else {
                    saw.join(", ")
                }
            ));
        }
    }

    Ok(())
}

/// The document must be rejected, reporting the rule the manifest names — either at the
/// identity gate or as an error from the validator. Both share one rule namespace so a
/// case does not have to know which layer will catch it.
fn check_invalid(text: &str, case: &Value) -> Result<(), String> {
    let expected = case["expect"]["rule"]
        .as_str()
        .ok_or("an `invalid` case must name expect.rule")?;

    match onyx_core::parse(text) {
        Err(error) => {
            if error.rule() == expected {
                Ok(())
            } else {
                Err(format!(
                    "expected {expected:?}, the gate reported {:?}",
                    error.rule()
                ))
            }
        }
        Ok(document) => {
            let report = onyx_core::validate(&document);
            let errors: Vec<&str> = report
                .findings
                .iter()
                .filter(|finding| finding.severity == onyx_core::Severity::Error)
                .map(|finding| finding.rule)
                .collect();

            if errors.contains(&expected) {
                Ok(())
            } else if errors.is_empty() {
                Err(format!(
                    "expected error {expected:?}, but the document was accepted"
                ))
            } else {
                Err(format!(
                    "expected error {expected:?}, got: {}",
                    errors.join(", ")
                ))
            }
        }
    }
}

/// Parse then serialize must lose nothing and must be idempotent.
fn check_roundtrip(text: &str) -> Result<(), String> {
    let once = onyx_core::parse(text).map_err(|error| format!("rejected: {error}"))?;
    let serialised = onyx_core::to_string_pretty(&once);
    let twice = onyx_core::parse(&serialised)
        .map_err(|error| format!("the engine rejected its own output: {error}"))?;

    if once != twice {
        return Err("parsing the engine's own output produced a different document".to_owned());
    }

    let again = onyx_core::to_string_pretty(&twice);
    if serialised != again {
        return Err(
            "serializing twice produced different text; the cycle is not idempotent".to_owned(),
        );
    }

    // Structural rather than field-by-field: this is what catches a struct added later
    // without a flattened `extra` map, which is the silent way preservation breaks.
    let before = json::parse(text).map_err(|error| error.to_string())?;
    let after = json::parse(&serialised).map_err(|error| error.to_string())?;
    contained(&before, &after, "")
}

/// Asserts every member of `before` survives somewhere in `after`, at any depth.
fn contained(before: &Value, after: &Value, path: &str) -> Result<(), String> {
    match before {
        Value::Object(members) => {
            let after = after
                .as_object()
                .ok_or_else(|| format!("{path} stopped being an object"))?;
            for (key, value) in members.iter() {
                let found = after
                    .get(key)
                    .ok_or_else(|| format!("{path}/{key} was dropped"))?;
                contained(value, found, &format!("{path}/{key}"))?;
            }
            Ok(())
        }
        Value::Array(items) => {
            let after = after
                .as_array()
                .ok_or_else(|| format!("{path} stopped being an array"))?;
            if items.len() != after.len() {
                return Err(format!("{path} changed length"));
            }
            for (index, item) in items.iter().enumerate() {
                contained(item, &after[index], &format!("{path}/{index}"))?;
            }
            Ok(())
        }
        scalar => {
            if scalar == after {
                Ok(())
            } else {
                Err(format!("{path} changed value"))
            }
        }
    }
}

/// The document must restore to the canonical projection in `expected.json`.
///
/// This is §5's real test. `summarise` reads only the portable layer — it has no way to
/// look inside a vendor block — so a passing case is proof that the diary survives
/// without one.
fn check_consumer(text: &str, expected_path: &Path) -> Result<(), String> {
    let document = onyx_core::parse(text).map_err(|error| format!("rejected: {error}"))?;

    let raw = std::fs::read_to_string(expected_path)
        .map_err(|error| format!("expected.json: {error}"))?;
    let expected = json::parse(&raw).map_err(|error| format!("expected.json: {error}"))?;
    let wanted = expected["days"]
        .as_array()
        .ok_or_else(|| "expected.json needs a `days` array".to_owned())?;

    let restored = onyx_core::summarise(&document);
    if restored.len() != wanted.len() {
        return Err(format!(
            "expected {} day(s), restored {}",
            wanted.len(),
            restored.len()
        ));
    }

    for (index, want) in wanted.iter().enumerate() {
        let got = &restored[index];
        let date = want["date"].as_str().unwrap_or_default();

        if got.date != date {
            return Err(format!(
                "day {index}: expected {date:?}, restored {:?}",
                got.date
            ));
        }

        let entries = want["entryCount"].as_i64().unwrap_or_default() as usize;
        if got.entry_count != entries {
            return Err(format!(
                "{date}: expected {entries} entr(ies), restored {}",
                got.entry_count
            ));
        }

        compare(got.energy_kcal, want.get("energyKcal"), "energyKcal", date)?;
        compare(got.body_mass_kg, want.get("bodyMassKg"), "bodyMassKg", date)?;
    }

    Ok(())
}

fn compare(got: Option<f64>, want: Option<&Value>, field: &str, date: &str) -> Result<(), String> {
    let want = want.and_then(Value::as_f64);
    match (got, want) {
        (None, None) => Ok(()),
        (Some(got), Some(want)) if close(got, want) => Ok(()),
        _ => Err(format!(
            "{date} {field}: expected {want:?}, restored {got:?}"
        )),
    }
}
