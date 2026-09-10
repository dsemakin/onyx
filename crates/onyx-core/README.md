# onyx-core

Reference engine for the [Onyx](https://github.com/dsemakin/onyx)
interchange format — a portable personal food and body-weight diary.

```rust
let text = r#"{
  "format": "onyx",
  "specVersion": "1.0.0",
  "exportedAt": "2026-08-10T09:12:00+03:00",
  "producer": { "name": "Example Tracker" }
}"#;

let document = onyx_core::parse(text)?;

// `producer` is required by the format and still an `Option` here. Every member the
// engine could fail to read is one, because the alternative is inventing a value — and a
// document this crate cannot read is one it must report, not quietly complete.
let producer = document.producer.expect("the format requires a producer");
assert_eq!(producer.name.as_deref(), Some("Example Tracker"));
# Ok::<(), onyx_core::Error>(())
```

This crate is deliberately narrow: no filesystem, no network, no wall-clock. Time is an
injected parameter, which is what keeps it testable and the wasm build small.

Licensed under MIT OR Apache-2.0.
