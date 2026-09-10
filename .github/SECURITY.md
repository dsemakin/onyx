# Security policy

## Reporting a vulnerability

Please report security issues privately through GitHub's
[private vulnerability reporting](https://github.com/dsemakin/onyx/security/advisories/new),
not as a public issue.

You should get an acknowledgement within a week. Please give a reasonable window for a
fix before disclosing publicly.

## Threat model

This engine parses files that arrive from untrusted sources — a diary someone was
emailed, downloaded, or restored from a backup of unknown origin. The relevant
guarantees are:

- `onyx-core` is `#![forbid(unsafe_code)]`.
- No panics on malformed input. Parse failures are values, not aborts.
- No network access and no filesystem access from the core library.
- Parsing is fuzzed two ways. `crates/onyx-core/tests/robustness.rs` runs 40,000
  deterministic mutations of real corpus documents on stable Rust in ordinary CI, on
  every push — 20,000 against the JSON reader and 20,000 against the typed layer —
  plus every truncated prefix of every corpus document. It asserts that nothing panics
  and that anything which parses round-trips unchanged. `fuzz/` holds coverage-guided libFuzzer targets for the parser and the
  typed layer, run on demand with `cargo +nightly fuzz run parse`; that crate is outside
  the workspace, so it never affects what anyone installs.
- Invalid UTF-8 cannot reach the parser: it takes `&str`, and both entry points validate
  first — the CLI through `read_to_string`, the WebAssembly binding through
  `from_utf8_lossy`.

Documents can be arbitrarily large and deeply nested, so resource exhaustion when
parsing hostile input is in scope and worth reporting.

The specification defines no signature or integrity mechanism. A document is trusted
exactly as far as its source is; this is stated in §6 of the spec and is not a
vulnerability in itself.

## Supported versions

Pre-release. Until 1.0.0 of the crates, only the latest release receives fixes.
