// SPDX-License-Identifier: MIT OR Apache-2.0
#![no_main]

//! The JSON reader, against arbitrary input.
//!
//! The property is that no input panics. `onyx-core` forbids unsafe, so memory corruption
//! is off the table; a panic is the realistic failure and it aborts the caller's process,
//! which no caller can handle.
//!
//! Anything that parses must also survive being written and read back, so a value that
//! round-trips into a *different* value is a failure too.

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // parse takes &str; both real entry points validate UTF-8 before calling it.
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };

    if let Ok(value) = onyx_core::json::parse(text) {
        let written = value.to_string();
        let reparsed = onyx_core::json::parse(&written).expect("our own output must parse");
        assert_eq!(value, reparsed, "a value changed on the way through");
    }
});
