//! A processor must not add members either.
//!
//! `corpus.rs` asserts every member of the input survives. That is only half of
//! preservation: it says nothing about members appearing that were never written. An engine
//! that invents `"date": ""` for a day that had none passes containment completely, and has
//! still edited someone's document.

use std::path::{Path, PathBuf};

use onyx_core::json::{self, Value};

fn corpus_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus")
}

/// Reports every path present in `after` and absent from `before`.
fn additions(before: &Value, after: &Value, path: &str, found: &mut Vec<String>) {
    match (before, after) {
        (Value::Object(before), Value::Object(after)) => {
            for (key, value) in after.iter() {
                match before.get(key) {
                    None => found.push(format!("{path}/{key} = {value}")),
                    Some(original) => additions(original, value, &format!("{path}/{key}"), found),
                }
            }
        }
        (Value::Array(before), Value::Array(after)) => {
            // A length change was previously swallowed by the wildcard arm below, so an
            // engine that appended a whole extra element reported zero additions. The guard
            // would have passed a build that fabricated an entire day.
            if after.len() > before.len() {
                for (index, extra) in after.iter().enumerate().skip(before.len()) {
                    found.push(format!("{path}/{index} = {extra}"));
                }
            } else if after.len() < before.len() {
                found.push(format!(
                    "{path} lost {} element(s): {} became {}",
                    before.len() - after.len(),
                    before.len(),
                    after.len()
                ));
            }
            for (index, (b, a)) in before.iter().zip(after.iter()).enumerate() {
                additions(b, a, &format!("{path}/{index}"), found);
            }
        }
        // A member that changed type entirely is neither an addition nor a loss of a
        // member, and `corpus.rs`'s containment check is what covers it.
        _ => {}
    }
}

#[test]
fn the_engine_adds_no_member_the_document_did_not_have() {
    let dir = corpus_dir();
    let manifest =
        json::parse(&std::fs::read_to_string(dir.join("manifest.json")).expect("manifest"))
            .expect("manifest is JSON");

    let mut failures = Vec::new();

    for case in manifest["cases"].as_array().expect("cases") {
        let relative = match case["file"].as_str() {
            Some(file) => file.to_owned(),
            None => format!("{}/document.json", case["dir"].as_str().unwrap_or_default()),
        };
        let text = match std::fs::read_to_string(dir.join(&relative)) {
            Ok(text) => text,
            Err(error) => {
                failures.push(format!("{relative}: {error}"));
                continue;
            }
        };

        // Only documents the engine will actually read.
        let Ok(document) = onyx_core::parse(&text) else {
            continue;
        };
        let written = onyx_core::to_string_pretty(&document);

        let before = json::parse(&text).expect("the fixture is JSON");
        let after = json::parse(&written).expect("the engine writes JSON");

        let mut added = Vec::new();
        additions(&before, &after, "", &mut added);
        if !added.is_empty() {
            failures.push(format!("{relative}\n    {}", added.join("\n    ")));
        }
    }

    assert!(
        failures.is_empty(),
        "the engine changed the membership of {} document(s):\n\n  {}\n\nA member that was not written must not appear, and one that was must not vanish. Both directions are preservation.\n",
        failures.len(),
        failures.join("\n\n  "),
    );
}
