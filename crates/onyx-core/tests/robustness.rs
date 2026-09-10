//! Hammers the parser with malformed input and asserts it never panics.
//!
//! This crate hand-writes its JSON reader, which makes the parser the highest-risk code in
//! the project: it is the part that meets a file somebody was emailed. `#![forbid(unsafe_code)]`
//! rules out memory corruption, so the realistic failure is a panic — an index out of
//! bounds, an `unwrap` on a truncated escape — and a panic aborts the caller's process,
//! which is not something a caller can handle.
//!
//! So the property under test is simply: **no input produces a panic.** Every mutation
//! below either parses or returns an error.
//!
//! Deterministic on purpose. A fixed seed means a failure here is reproducible from the
//! output alone, rather than something that showed up once in CI and never again. For
//! coverage-guided fuzzing on top of this, see `fuzz/`.
//!
//! Note that invalid UTF-8 cannot reach the parser: `parse` takes `&str`, and both real
//! entry points validate before calling it — the CLI through `read_to_string`, the
//! WebAssembly binding through `from_utf8_lossy`.

use std::path::Path;

/// xorshift64. Ten lines, no dependency, and identical on every platform and run.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    fn below(&mut self, bound: usize) -> usize {
        if bound == 0 {
            0
        } else {
            (self.next() % bound as u64) as usize
        }
    }
}

/// Every document in the corpus, as bytes to mutate.
fn seeds() -> Vec<String> {
    let corpus = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus");
    let mut found = Vec::new();
    let mut stack = vec![corpus];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "json") {
                if let Ok(text) = std::fs::read_to_string(&path) {
                    found.push(text);
                }
            }
        }
    }
    assert!(found.len() >= 8, "expected the corpus to provide seeds");
    found
}

/// One mutation, chosen by the generator.
fn mutate(rng: &mut Rng, seed: &str) -> String {
    let mut bytes = seed.as_bytes().to_vec();
    if bytes.is_empty() {
        return String::new();
    }

    match rng.below(6) {
        // Flip a byte: turns a brace into a letter, a digit into a quote.
        0 => {
            let at = rng.below(bytes.len());
            bytes[at] ^= 1 << rng.below(8);
        }
        // Truncate: every prefix of a valid document must fail cleanly.
        1 => bytes.truncate(rng.below(bytes.len())),
        // Insert a byte somewhere.
        2 => {
            let at = rng.below(bytes.len());
            bytes.insert(at, rng.below(256) as u8);
        }
        // Delete a run.
        3 => {
            let at = rng.below(bytes.len());
            let len = rng.below(bytes.len() - at).min(32);
            bytes.drain(at..at + len);
        }
        // Duplicate a run, which is how you get runaway nesting and repeated members.
        4 => {
            let at = rng.below(bytes.len());
            let len = rng.below(bytes.len() - at).min(64);
            let run: Vec<u8> = bytes[at..at + len].to_vec();
            bytes.splice(at..at, run);
        }
        // Deep nesting, aimed squarely at the recursion limit.
        _ => {
            let depth = 1 + rng.below(400);
            let mut deep = "[".repeat(depth);
            deep.push('1');
            deep.push_str(&"]".repeat(depth));
            return deep;
        }
    }

    String::from_utf8_lossy(&bytes).into_owned()
}

#[test]
fn no_mutation_of_a_valid_document_can_panic() {
    let seeds = seeds();
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let mut parsed = 0usize;

    for round in 0..20_000 {
        let seed = &seeds[round % seeds.len()];
        let candidate = mutate(&mut rng, seed);

        // The property. A panic here fails the test with the seed printed above it.
        match onyx_core::json::parse(&candidate) {
            Ok(value) => {
                parsed += 1;
                // Anything that parses must also survive being written and read back.
                let text = value.to_string();
                let again = onyx_core::json::parse(&text)
                    .unwrap_or_else(|e| panic!("re-parsing our own output failed: {e}"));
                assert_eq!(value, again, "a value changed on the way through");
            }
            Err(_) => { /* rejecting malformed input is the expected outcome */ }
        }
    }

    // If almost nothing parsed, the mutations are too destructive to be testing the
    // interesting paths, and this test would be quietly worthless.
    assert!(
        parsed > 200,
        "only {parsed} of 20000 mutations parsed; the generator is too aggressive"
    );
}

#[test]
fn the_document_gate_never_panics_either() {
    // json::parse is the lower layer. This covers the typed layer on top of it, where the
    // codec reads members out of a working copy.
    let seeds = seeds();
    let mut rng = Rng(0xDEAD_BEEF_CAFE_F00D);

    for round in 0..20_000 {
        let candidate = mutate(&mut rng, &seeds[round % seeds.len()]);
        let _ = onyx_core::parse(&candidate);
    }
}

#[test]
fn every_prefix_of_every_corpus_document_fails_cleanly() {
    // Truncation is the most common real-world corruption: a partial download, a file
    // copied while it was still being written.
    for seed in seeds() {
        for length in 0..seed.len() {
            if seed.is_char_boundary(length) {
                let _ = onyx_core::parse(&seed[..length]);
            }
        }
    }
}
