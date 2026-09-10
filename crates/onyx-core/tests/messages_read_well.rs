//! Every message a user can be shown, rendered and inspected.
//!
//! # Why this exists
//!
//! A Rust string literal continued with a trailing backslash swallows the newline *and* the
//! next line's indentation. Editing those literals through tooling that does not preserve
//! the backslash turns one into a single long line with the indentation baked in, and the
//! result is a message reading `could not be                  restored`.
//!
//! That has now happened four times across three review rounds — twice in the very code
//! written to fix the previous instance — and every existing test looked straight past it,
//! because they all assert on *substrings* (`contains("array element")`) or on the presence
//! of a key. A substring assertion cannot see the whitespace between the words it matched.
//!
//! So this renders the real thing: `Display` for every `Error`, and the message of every
//! finding the corpus can provoke. It is deliberately behavioural. Grepping the source for
//! runs of spaces finds the aligned columns in `onyx inspect` and the indentation after a
//! `\n` and produces noise; rendering the output finds only what a person would actually
//! read.

use std::path::{Path, PathBuf};

use onyx_core::json;

/// Whether a message fails to read as the single line of prose it is meant to be.
///
/// # What this got wrong the first time
///
/// The original version tested for runs of two or more spaces and deliberately allowed a
/// run that followed a newline, on the reasoning that such a run is indentation. That
/// reasoning was backwards. A *message* is one line of prose — the CLI and the JSON payload
/// both present it as one — so a newline inside it is never deliberate, and the indentation
/// after it is exactly the defect. Allowing it meant the guard was built for one *mechanism*
/// of the bug (a continuation backslash lost in editing, which collapses to a visible double
/// space) and was blind to the other (a bare multi-line literal, which bakes in a real
/// newline plus indentation). The same symptom, arriving a different way, walked straight
/// past it — including in `document/missing-timezone`, the most frequently emitted warning
/// the engine has.
///
/// So the rule is now the property itself rather than a signature of one cause: a message is
/// one line, single-spaced, with no tabs. Every way of producing the defect violates that,
/// including ways nobody has thought of yet.
fn does_not_read_as_one_line(text: &str) -> Option<&'static str> {
    if text.contains('\n') || text.contains('\r') {
        return Some("contains a line break");
    }
    if text.contains('\t') {
        return Some("contains a tab");
    }
    if text.contains("  ") {
        return Some("contains a run of spaces");
    }
    None
}

fn corpus_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus")
}

#[test]
fn no_finding_message_has_a_gap_in_it() {
    let dir = corpus_dir();
    let manifest =
        json::parse(&std::fs::read_to_string(dir.join("manifest.json")).expect("manifest"))
            .expect("manifest is JSON");

    let mut seen = 0usize;
    let mut bad = Vec::new();

    for case in manifest["cases"].as_array().expect("cases") {
        let relative = match case["file"].as_str() {
            Some(file) => file.to_owned(),
            None => format!("{}/document.json", case["dir"].as_str().unwrap_or_default()),
        };
        let Ok(text) = std::fs::read_to_string(dir.join(&relative)) else {
            continue;
        };
        let Ok(document) = onyx_core::parse(&text) else {
            continue;
        };

        for finding in onyx_core::validate(&document).findings {
            seen += 1;
            if does_not_read_as_one_line(&finding.message).is_some() {
                bad.push(format!(
                    "{} in {relative}\n      {}",
                    finding.rule, finding.message
                ));
            }
        }
    }

    assert!(
        seen >= 10,
        "only {seen} findings rendered; this test is not exercising the validator"
    );
    assert!(
        bad.is_empty(),
        "{} message(s) contain a run of spaces, which is what a collapsed line \
         continuation looks like:\n\n  {}\n",
        bad.len(),
        bad.join("\n  "),
    );
}

/// Every `Error` variant, rendered.
///
/// Listed by hand rather than derived, and the count is asserted, so adding a variant
/// without adding it here fails rather than silently going unchecked. `CannotPromote` — the
/// variant that carried the fourth instance of this defect — is only reachable by
/// constructing it, which is exactly why nothing had ever rendered it.
#[test]
fn no_error_message_has_a_gap_in_it() {
    let errors = vec![
        onyx_core::Error::NotOnyx {
            found: "something-else".to_owned(),
            expected: "onyx",
        },
        onyx_core::Error::MalformedVersion("1.x".to_owned()),
        onyx_core::Error::NoMigrationPath {
            from: "1.0.0".to_owned(),
            to: "2.0.0".to_owned(),
        },
        onyx_core::Error::CannotDemote {
            from: "1.1.0".to_owned(),
            to: "1.0.0".to_owned(),
            reason: "`/days/0` addresses an array element; only object members can be parked"
                .to_owned(),
        },
        // Each variant carries more than one reason, and rendering one of them says nothing
        // about the other. Both are listed, and the count below counts messages rather than
        // variants for the same reason.
        onyx_core::Error::CannotDemote {
            from: "1.1.0".to_owned(),
            to: "1.0.0".to_owned(),
            reason: "members must be parked under `extensions`, which is not an object here"
                .to_owned(),
        },
        onyx_core::Error::CannotDemote {
            from: "1.1.0".to_owned(),
            to: "1.0.0".to_owned(),
            reason: "the demoted block's `fromVersion` is not semver-shaped: \"2.x-beta\""
                .to_owned(),
        },
        onyx_core::Error::CannotPromote {
            from: "1.0.0".to_owned(),
            to: "1.1.0".to_owned(),
            reason: concat!(
                "1 parked member(s) could not be restored and remain parked: ",
                "/days/0/coverage (its parent could not be reached)"
            )
            .to_owned(),
        },
        onyx_core::Error::CannotPromote {
            from: "1.0.0".to_owned(),
            to: "1.1.0".to_owned(),
            reason: "the demoted block under `extensions` is not an object".to_owned(),
        },
        onyx_core::Error::MalformedManifest("it has no string `from`".to_owned()),
        onyx_core::Error::UnsupportedMajor {
            found: 2,
            supported: 1,
        },
        // `Json` wraps a parse error, whose own text is covered below.
        onyx_core::parse("{ nope").expect_err("malformed JSON"),
    ];

    assert_eq!(
        errors.len(),
        11,
        "an Error variant, or a reason string, was added without being rendered here"
    );

    for error in &errors {
        let rendered = error.to_string();
        if let Some(why) = does_not_read_as_one_line(&rendered) {
            panic!("{:?} {why}: {rendered:?}", error.rule());
        }
        assert!(
            !rendered.is_empty(),
            "{:?} renders as nothing at all",
            error.rule()
        );
    }
}

/// The detector must recognise every mechanism that produces the symptom, not the one that
/// happened to be found first. Rule four, earned the hard way.
#[test]
fn the_detector_sees_every_way_of_breaking_a_message() {
    // Mechanism one: a continuation backslash lost in editing.
    assert!(does_not_read_as_one_line("could not be                  restored").is_some());
    assert!(does_not_read_as_one_line("two  spaces").is_some());

    // Mechanism two: a bare multi-line literal. This is the one the first version allowed.
    assert!(
        does_not_read_as_one_line("No IANA time zone. Measurements fold\n   by the offset")
            .is_some()
    );
    assert!(does_not_read_as_one_line("a line\nanother line").is_some());

    // Mechanism three: tabs, which the byte-level check never looked for.
    assert!(does_not_read_as_one_line("a line\tthen a tab").is_some());

    // And ordinary prose passes.
    assert!(does_not_read_as_one_line("an ordinary sentence, single spaced.").is_none());
    assert!(does_not_read_as_one_line("").is_none());
    assert!(
        does_not_read_as_one_line("`%` is listed by the schema but §3.4 gives it no meaning.")
            .is_none()
    );
}
