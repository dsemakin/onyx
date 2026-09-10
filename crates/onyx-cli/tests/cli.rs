//! The command-line interface, exercised as a command line.
//!
//! This crate had no tests at all. Every behaviour asserted about it across six review
//! rounds — exit codes, `--json` payload shape, `--strict` — had only ever been checked by
//! someone running it by hand and reading the output. That is not a regression test, and it
//! is the interface scripts depend on: exit codes and payload keys are the contract, and
//! both are exactly the sort of thing a refactor breaks without touching a line of prose.
//!
//! `CARGO_BIN_EXE_onyx` is set by cargo for integration tests, so this runs the real binary
//! rather than calling into the library and hoping the wiring matches.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn corpus(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus")
        .join(relative)
}

fn onyx(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_onyx"))
        .args(args)
        .output()
        .expect("the binary runs")
}

fn code(output: &Output) -> i32 {
    output.status.code().expect("the process exited normally")
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// The three exit codes are the interface. A script distinguishes "your file is wrong" from
/// "I could not run" by this and nothing else.
#[test]
fn exit_codes_say_which_kind_of_failure_it_was() {
    let valid = corpus("valid/minimal.json");
    let valid = valid.to_str().unwrap();
    let invalid = corpus("invalid/duplicate-day.json");
    let invalid = invalid.to_str().unwrap();

    assert_eq!(
        code(&onyx(&["validate", valid])),
        0,
        "a conforming document"
    );
    assert_eq!(
        code(&onyx(&["validate", invalid])),
        1,
        "read and does not conform"
    );

    // Everything below is the tool failing, not the document.
    assert_eq!(code(&onyx(&["validate", "no-such-file.json"])), 2);
    assert_eq!(code(&onyx(&["frobnicate", valid])), 2, "unknown command");
    assert_eq!(code(&onyx(&["validate"])), 2, "no file given");
    assert_eq!(code(&onyx(&["validate", valid, "--nope"])), 2, "bad flag");
    assert_eq!(
        code(&onyx(&["migrate", valid, "--to", "nonsense"])),
        2,
        "a malformed --to is a bad argument, not a verdict on the document"
    );
    assert_eq!(
        code(&onyx(&["migrate", valid, "--to", "--json"])),
        2,
        "--to must not swallow the next flag"
    );
}

/// Every `--json` payload carries the same keys, on every path, including the ones where
/// the tool never reached a verdict.
#[test]
fn the_json_payload_has_one_shape() {
    const KEYS: &[&str] = &[
        "reportVersion",
        "conforming",
        "accepted",
        "strict",
        "specVersion",
        "producer",
        "findings",
    ];

    let valid = corpus("valid/minimal.json");
    let not_onyx = corpus("invalid/not-an-onyx-document.json");

    for args in [
        vec!["validate", valid.to_str().unwrap(), "--json"],
        vec!["validate", not_onyx.to_str().unwrap(), "--json"],
        vec!["validate", "no-such-file.json", "--json"],
    ] {
        let text = stdout(&onyx(&args));
        let value = onyx_core::json::parse(&text)
            .unwrap_or_else(|error| panic!("{args:?} did not print JSON: {error}\n{text}"));
        let object = value.as_object().expect("an object");

        for key in KEYS {
            assert!(
                object.contains_key(key),
                "{args:?} is missing {key:?}; the payload shape must not depend on the path"
            );
        }
    }
}

/// Key presence is not the contract; the values are. A tool error must report **null**
/// verdicts — not `false`, which claims the document was examined and found wanting.
#[test]
fn a_tool_error_reports_no_verdict_rather_than_a_negative_one() {
    let output = onyx(&["validate", "no-such-file.json", "--json"]);
    assert_eq!(code(&output), 2);

    let value = onyx_core::json::parse(&stdout(&output)).expect("JSON on the failure path");
    for key in ["conforming", "accepted", "specVersion", "producer"] {
        assert!(
            matches!(value.get(key), Some(onyx_core::json::Value::Null)),
            "{key} should be null when the file could not even be opened, got {:?}",
            value.get(key)
        );
    }

    // `strict` is knowable from argv, so it is reported rather than nulled.
    assert_eq!(
        value
            .get("strict")
            .and_then(onyx_core::json::Value::as_bool),
        Some(false)
    );
    let strict = onyx(&["validate", "no-such-file.json", "--json", "--strict"]);
    assert_eq!(
        onyx_core::json::parse(&stdout(&strict))
            .expect("JSON")
            .get("strict")
            .and_then(onyx_core::json::Value::as_bool),
        Some(true)
    );

    // And a document that WAS read and rejected says so, with a real false.
    let rejected = onyx(&[
        "validate",
        corpus("invalid/duplicate-day.json").to_str().unwrap(),
        "--json",
    ]);
    assert_eq!(
        onyx_core::json::parse(&stdout(&rejected))
            .expect("JSON")
            .get("conforming")
            .and_then(onyx_core::json::Value::as_bool),
        Some(false),
        "a document that was examined gets a verdict, not a null"
    );
}

/// `--strict` changes the verdict, not the document. The payload has to say both.
#[test]
fn strict_is_reported_as_well_as_acted_on() {
    let warns = corpus("valid/atwater-mismatch.json");
    let warns = warns.to_str().unwrap();

    let ordinary = onyx(&["validate", warns, "--json"]);
    let strict = onyx(&["validate", warns, "--json", "--strict"]);

    assert_eq!(code(&ordinary), 0, "warnings alone do not fail a document");
    assert_eq!(code(&strict), 1, "--strict rejects on warnings");

    let read = |output: &Output, key: &str| {
        onyx_core::json::parse(&stdout(output))
            .expect("JSON")
            .get(key)
            .and_then(onyx_core::json::Value::as_bool)
    };

    // The document conforms either way; only the run's verdict changes.
    assert_eq!(read(&ordinary, "conforming"), Some(true));
    assert_eq!(read(&strict, "conforming"), Some(true));
    assert_eq!(read(&ordinary, "accepted"), Some(true));
    assert_eq!(
        read(&strict, "accepted"),
        Some(false),
        "the payload must agree with the exit code"
    );
}

/// `migrate` prints to standard output and never touches the input file.
#[test]
fn migrate_writes_to_stdout_and_leaves_the_input_alone() {
    let path = corpus("valid/minimal.json");
    let before = std::fs::read_to_string(&path).expect("the fixture");

    let output = onyx(&["migrate", path.to_str().unwrap(), "--to", "1.0.0"]);
    assert_eq!(code(&output), 0);
    assert!(
        onyx_core::json::parse(&stdout(&output)).is_ok(),
        "migrate must print a document"
    );

    let after = std::fs::read_to_string(&path).expect("the fixture");
    assert_eq!(before, after, "migrate edited the file in place");
}

/// `--` ends the options, so a file whose name begins with `--` can still be named.
///
/// The first version of this test built the path with `temp_dir().join("--help")` and passed
/// that. An absolute path does not begin with `--`, so nothing about the collision was ever
/// exercised — the test passed against a CLI that had no marker support at all, and its own
/// assertion message said "an absolute path is not itself a flag", which is the observation
/// that should have sunk it. The argument has to *be* `--help`, so the process runs in the
/// directory instead.
#[test]
fn a_file_named_like_a_flag_can_still_be_named() {
    let dir = std::env::temp_dir().join("onyx-cli-end-of-options");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a temporary directory");
    std::fs::copy(corpus("valid/minimal.json"), dir.join("--help")).expect("the fixture");

    let in_dir = |args: &[&str]| {
        // Every argument here is checked to be the bare, flag-shaped name: an absolute path
        // would silently make this test prove nothing, which is exactly what happened.
        assert!(
            args.contains(&"--help"),
            "the argument must itself look like a flag"
        );
        Command::new(env!("CARGO_BIN_EXE_onyx"))
            .args(args)
            .current_dir(&dir)
            .output()
            .expect("the binary runs")
    };

    let escaped = in_dir(&["validate", "--", "--help"]);
    assert_eq!(
        code(&escaped),
        0,
        "`validate -- --help` must read the file, not print the help:
{}",
        stdout(&escaped)
    );
    assert!(
        !stdout(&escaped).contains("USAGE"),
        "the help was printed despite the marker"
    );

    // Without the marker it is a flag, which is what the marker exists to escape.
    let unescaped = in_dir(&["validate", "--help"]);
    assert!(
        stdout(&unescaped).contains("USAGE"),
        "without `--`, `--help` is the flag"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// Options belong to one command each. Accepting a misplaced one in silence reads as though
/// it had been applied.
#[test]
fn each_command_takes_only_its_own_options() {
    let valid = corpus("valid/minimal.json");
    let valid = valid.to_str().unwrap();

    assert_eq!(code(&onyx(&["inspect", valid, "--json"])), 2);
    assert_eq!(code(&onyx(&["inspect", valid, "--strict"])), 2);
    assert_eq!(code(&onyx(&["validate", valid, "--to", "1.0.0"])), 2);
    assert_eq!(
        code(&onyx(&["migrate", valid, "--to", "1.0.0", "--strict"])),
        2
    );
    assert_eq!(code(&onyx(&["migrate", valid])), 2, "migrate needs --to");
}
