//! A WebAssembly module with no dependencies beyond the engine itself.
//!
//! # Why this is hand-written
//!
//! The obvious way to expose Rust to JavaScript is `wasm-bindgen`. It is good, and it was
//! used here first. It was replaced because it is the wrong shape for a project that has
//! to keep working without maintenance: it is `0.2.x`, where Cargo treats the minor as the
//! breaking position, and it must agree on a version with `wasm-pack`. That is a toolchain
//! coupling, and toolchains rot faster than libraries.
//!
//! What is underneath it is far more stable. The WebAssembly binary format is a W3C
//! standard, and `wasm32-unknown-unknown` is a target the Rust project ships through
//! rustup. Neither has a version this crate has to chase.
//!
//! # The ABI
//!
//! WebAssembly can only pass numbers, so strings are passed as offsets into the module's
//! linear memory. Three functions in, one convention out:
//!
//! - `onyx_alloc(len) -> ptr` — the host asks for a buffer and writes UTF-8 into it.
//! - `onyx_free(ptr, len)` — the host returns a buffer.
//! - `onyx_validate(ptr, len) -> ptr`, and friends — the module answers with a pointer to a
//!   little-endian `u32` length followed by that many UTF-8 bytes.
//!
//! Prefixing the length means every entry point returns a single number, which is the one
//! thing every WebAssembly host agrees on. No multi-value returns, no globals, no
//! generated glue.
//!
//! # Safety
//!
//! This crate is the only place in the project with `unsafe`, and it is confined to the
//! four functions below — the host handing back a pointer it was given is a trust boundary
//! no type system spans. `onyx-core` remains `#![forbid(unsafe_code)]`; new unsafe anywhere
//! else in this crate is a compile error.

#![deny(unsafe_code)]

use onyx_core::REPORT_VERSION;

use onyx_core::json::Value;
use onyx_core::json_object;

/// The largest document this module will accept, in bytes.
///
/// A food diary is not this big. The cap exists because the workspace builds with
/// `panic = "abort"`, so a failed allocation does not unwind — it kills the instance, and
/// every later call on it traps. One oversized input would therefore take the module down
/// for the life of the process rather than being refused.
pub const MAX_INPUT_BYTES: usize = 64 * 1024 * 1024;

/// Reserves `len` bytes for the host to write UTF-8 into.
///
/// Returns null if `len` is over [`MAX_INPUT_BYTES`], which the host must check. The
/// buffer is otherwise deliberately leaked; ownership passes to the host, which must
/// return it through [`onyx_free`].
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn onyx_alloc(len: usize) -> *mut u8 {
    // Checked before allocating: with panic = "abort" there is no second chance.
    if len > MAX_INPUT_BYTES {
        return core::ptr::null_mut();
    }
    let mut buffer: Vec<u8> = Vec::with_capacity(len);
    let pointer = buffer.as_mut_ptr();
    core::mem::forget(buffer);
    pointer
}

/// Returns a buffer previously handed out by [`onyx_alloc`] or by an entry point.
///
/// # Safety
///
/// `pointer` must have come from this module and `len` must be the length it was created
/// with. Calling it with anything else is undefined behaviour.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn onyx_free(pointer: *mut u8, len: usize) {
    if pointer.is_null() || len == 0 {
        return;
    }
    // SAFETY: the contract above. Reconstructing with capacity == len matches how both
    // onyx_alloc and reply() create their buffers.
    drop(unsafe { Vec::from_raw_parts(pointer, len, len) });
}

/// Reads a host-provided UTF-8 string without taking ownership of it.
///
/// # Safety
///
/// `pointer` and `len` must describe a buffer inside this module's memory.
#[allow(unsafe_code)]
unsafe fn borrow(pointer: *const u8, len: usize) -> String {
    if pointer.is_null() || len == 0 {
        return String::new();
    }
    // SAFETY: the contract above.
    let bytes = unsafe { core::slice::from_raw_parts(pointer, len) };
    String::from_utf8_lossy(bytes).into_owned()
}

/// Hands a string back as a length-prefixed buffer the host must free.
#[allow(unsafe_code)]
fn reply(text: String) -> *mut u8 {
    let bytes = text.into_bytes();
    let mut buffer = Vec::with_capacity(4 + bytes.len());
    buffer.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
    buffer.extend_from_slice(&bytes);

    // Capacity is exactly 4 + len, which is what the host passes back to onyx_free.
    let pointer = buffer.as_mut_ptr();
    core::mem::forget(buffer);
    pointer
}

// ── entry points ─────────────────────────────────────────────────────────────

/// Validates a document. Returns a JSON report.
///
/// Never traps on bad input: a malformed document is a report with `conforming: false`.
/// A JavaScript caller should not need a try/catch to handle a bad file.
///
/// # Safety
///
/// `pointer` and `len` must describe a buffer obtained from [`onyx_alloc`].
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn onyx_validate(pointer: *const u8, len: usize) -> *mut u8 {
    // SAFETY: the contract above.
    let input = unsafe { borrow(pointer, len) };
    reply(validate_report(&input))
}

/// Restores the diary from the portable layer. Returns a JSON report.
///
/// # Safety
///
/// `pointer` and `len` must describe a buffer obtained from [`onyx_alloc`].
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn onyx_summary(pointer: *const u8, len: usize) -> *mut u8 {
    // SAFETY: the contract above.
    let input = unsafe { borrow(pointer, len) };
    reply(summary_report(&input))
}

/// Parses a document and serializes it again. Returns a JSON report whose `document`
/// member carries the engine's own output.
///
/// It is a **string**, not nested JSON, and that is the whole point. Preservation and
/// idempotence are properties of the text the engine writes: handing back a parsed tree
/// would let the host's own JSON printer stand in for the engine's, which is precisely the
/// substitution that would hide a serializer bug. A caller compares the string.
///
/// # Safety
///
/// `pointer` and `len` must describe a buffer obtained from [`onyx_alloc`].
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn onyx_serialize(pointer: *const u8, len: usize) -> *mut u8 {
    // SAFETY: the contract above.
    let input = unsafe { borrow(pointer, len) };
    reply(serialize_report(&input))
}

/// The specification version this build implements.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn onyx_spec_version() -> *mut u8 {
    reply(onyx_core::SPEC_VERSION.to_owned())
}

/// The report shape version, so a host can refuse a module it does not understand.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn onyx_report_version() -> u32 {
    REPORT_VERSION as u32
}

// ── the reports themselves ───────────────────────────────────────────────────
//
// Plain safe Rust. Keeping these separate from the ABI means the interesting logic is
// testable natively, without a WebAssembly host.
//
// `conforming` means the same thing in every report: the document raised no `error`
// finding, exactly as docs/conformance.md defines it. It is deliberately NOT "the call
// succeeded" — summary and serialize answer that with the presence of `days` and
// `document`. One member answering two questions is how a caller ends up trusting a
// document that a different entry point would have rejected.

fn unreadable(error: &onyx_core::Error) -> String {
    Value::Object(json_object! {
        "reportVersion" => REPORT_VERSION,
        "conforming" => false,
        "accepted" => false,
        "strict" => false,
        "specVersion" => Value::Null,
        "producer" => Value::Null,
        "findings" => vec![Value::Object(json_object! {
            "severity" => "error",
            "rule" => error.rule(),
            "path" => "",
            "message" => error.to_string(),
        })],
    })
    .to_string()
}

fn findings_of(report: &onyx_core::Report) -> Vec<Value> {
    report
        .findings
        .iter()
        .map(|finding| {
            Value::Object(json_object! {
                "severity" => finding.severity.name(),
                "rule" => finding.rule,
                "path" => finding.path.clone(),
                "message" => finding.message.clone(),
            })
        })
        .collect()
}

fn validate_report(input: &str) -> String {
    let document = match onyx_core::parse(input) {
        Ok(document) => document,
        Err(error) => return unreadable(&error),
    };

    let report = onyx_core::validate(&document);

    Value::Object(json_object! {
        "reportVersion" => REPORT_VERSION,
        "conforming" => report.is_conforming(),
        // `accepted` and `strict` exist so the two CLIs answer the same questions. The
        // module has no opinion about strictness — that is the host's flag — so it reports
        // the document's own verdict and lets the host narrow it.
        "accepted" => report.is_conforming(),
        "strict" => false,
        "specVersion" => document.spec_version.clone(),
        "producer" => document.producer.as_ref().map(|p| p.name.clone()),
        "findings" => findings_of(&report),
    })
    .to_string()
}

fn summary_report(input: &str) -> String {
    let document = match onyx_core::parse(input) {
        Ok(document) => document,
        Err(error) => return unreadable(&error),
    };

    let days: Vec<Value> = onyx_core::summarise(&document)
        .into_iter()
        .map(|day| {
            Value::Object(json_object! {
                "date" => day.date,
                "entryCount" => day.entry_count,
                "energyKcal" => day.energy_kcal,
                "bodyMassKg" => day.body_mass_kg,
            })
        })
        .collect();

    // `findings` is included for the same reason `validate` has it: a caller told the
    // document does not conform must be able to say what is wrong with it. Without this
    // the CLI printed an empty list and exited non-zero in silence.
    let report = onyx_core::validate(&document);

    Value::Object(json_object! {
        "reportVersion" => REPORT_VERSION,
        "conforming" => report.is_conforming(),
        "accepted" => report.is_conforming(),
        "strict" => false,
        "specVersion" => document.spec_version.clone(),
        "producer" => document.producer.as_ref().map(|p| p.name.clone()),
        "findings" => findings_of(&report),
        "days" => days,
    })
    .to_string()
}

fn serialize_report(input: &str) -> String {
    let document = match onyx_core::parse(input) {
        Ok(document) => document,
        Err(error) => return unreadable(&error),
    };

    // Once. Adding `accepted` by hand left three calls to `validate` and two copies of each
    // member — harmless, because the last write wins and they agreed, which is exactly what
    // makes it the kind of thing that survives a review.
    let conforming = onyx_core::validate(&document).is_conforming();

    Value::Object(json_object! {
        "reportVersion" => REPORT_VERSION,
        "conforming" => conforming,
        // The failure payload carries these, so the success payload must too — the shape a
        // caller sees cannot depend on which way the call went. The module has no `--strict`
        // of its own; that is the host's flag, so it reports the document's own verdict.
        "accepted" => conforming,
        "strict" => false,
        "specVersion" => document.spec_version.clone(),
        "producer" => document.producer.as_ref().map(|p| p.name.clone()),
        "document" => onyx_core::to_string_pretty(&document),
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINIMAL: &str = r#"{
      "format": "onyx",
      "specVersion": "1.0.0",
      "exportedAt": "2026-08-14T12:00:00+03:00",
      "timeZone": "Europe/Berlin",
      "producer": { "name": "Test" }
    }"#;

    fn read(text: &str) -> Value {
        onyx_core::json::parse(text).expect("a report must be valid JSON")
    }

    #[test]
    fn reports_are_plain_rust_and_testable_without_a_wasm_host() {
        let report = read(&validate_report(MINIMAL));
        assert_eq!(
            report.get("conforming").and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(report.get("producer").and_then(Value::as_str), Some("Test"));
        assert_eq!(report.get("reportVersion").and_then(Value::as_i64), Some(1));
    }

    #[test]
    fn a_bad_document_is_a_report_rather_than_a_trap() {
        let report = read(&validate_report("{ nope"));
        assert_eq!(
            report.get("conforming").and_then(Value::as_bool),
            Some(false)
        );
        let rule = report
            .get("findings")
            .and_then(|findings| findings.get("0"))
            .and_then(|finding| finding.get("rule"))
            .and_then(Value::as_str);
        assert_eq!(rule, Some("document/unreadable"));
    }

    /// Parses cleanly, but `Z` loses the local day and §2.6 makes that an error.
    const NON_CONFORMING: &str = r#"{
      "format": "onyx",
      "specVersion": "1.0.0",
      "exportedAt": "2026-08-14T12:00:00Z",
      "timeZone": "Europe/Berlin",
      "producer": { "name": "Test" }
    }"#;

    /// One member, one meaning. A caller that reaches for `summary` or `serialize` must not
    /// be told a document is fine when `validate` would reject it.
    #[test]
    fn conforming_answers_the_same_question_in_every_report() {
        for (entry_point, text) in [
            ("validate", validate_report(NON_CONFORMING)),
            ("summary", summary_report(NON_CONFORMING)),
            ("serialize", serialize_report(NON_CONFORMING)),
        ] {
            assert_eq!(
                read(&text).get("conforming").and_then(Value::as_bool),
                Some(false),
                "{entry_point} called a non-conforming document conforming"
            );
        }
    }

    /// Non-conforming is not unreadable. A processor still has to be able to carry a flawed
    /// document through without losing it — that is the whole point of the role.
    #[test]
    fn a_non_conforming_document_can_still_be_serialized() {
        let report = read(&serialize_report(NON_CONFORMING));
        let text = report
            .get("document")
            .and_then(Value::as_str)
            .expect("a document that parses must still come back out");
        assert!(text.contains("2026-08-14T12:00:00Z"));
    }

    #[test]
    fn serialize_hands_back_the_engines_own_text_and_is_idempotent() {
        let once = read(&serialize_report(MINIMAL));
        let text = once.get("document").and_then(Value::as_str).unwrap();

        // The engine must accept its own output, and writing it again must not change it.
        let twice = read(&serialize_report(text));
        assert_eq!(twice.get("conforming").and_then(Value::as_bool), Some(true));
        assert_eq!(twice.get("document").and_then(Value::as_str), Some(text));

        // A string, not a nested object: the point is the bytes the engine wrote.
        assert!(text.starts_with('{'), "document must be serialized text");
    }

    #[test]
    fn serialize_reports_a_bad_document_rather_than_trapping() {
        let report = read(&serialize_report("{ nope"));
        assert_eq!(
            report.get("conforming").and_then(Value::as_bool),
            Some(false)
        );
        assert!(report.get("document").is_none());
    }

    #[test]
    fn summary_restores_days_from_the_portable_layer() {
        let report = read(&summary_report(MINIMAL));
        assert_eq!(
            report.get("conforming").and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(
            report
                .get("days")
                .and_then(Value::as_array)
                .map(<[Value]>::len),
            Some(0)
        );
    }
    /// Every entry point answers with the same members, whichever way the call went.
    ///
    /// The CLI has a test for exactly this and the module did not, so `summary` and
    /// `serialize` quietly omitted `accepted` and `strict` while the shared failure payload
    /// always carried them — the same shape-inconsistency the CLI test exists to prevent,
    /// one layer down.
    #[test]
    fn every_entry_point_answers_with_the_same_members() {
        const SHARED: &[&str] = &[
            "reportVersion",
            "conforming",
            "accepted",
            "strict",
            "specVersion",
            "producer",
        ];

        let payloads = [
            ("validate", validate_report(MINIMAL)),
            ("summary", summary_report(MINIMAL)),
            ("serialize", serialize_report(MINIMAL)),
            ("validate/unreadable", validate_report("{ nope")),
            ("summary/unreadable", summary_report("{ nope")),
            ("serialize/unreadable", serialize_report("{ nope")),
        ];

        for (name, text) in &payloads {
            let value = read(text);
            let object = value.as_object().expect("an object");
            for member in SHARED {
                assert!(
                    object.contains_key(member),
                    "{name} is missing {member:?}; the payload shape must not depend on the entry point"
                );
            }
        }
    }
}
