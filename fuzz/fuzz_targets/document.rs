// SPDX-License-Identifier: MIT OR Apache-2.0
#![no_main]

//! The typed layer: the identity gate, the codec, the validator and the summariser.
//!
//! Same property — nothing panics — over the code that reads members out of a document
//! rather than characters out of a string.

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };

    if let Ok(document) = onyx_core::parse(text) {
        let _ = onyx_core::validate(&document);
        let _ = onyx_core::summarise(&document);
        let _ = onyx_core::to_string(&document);
    }
});
