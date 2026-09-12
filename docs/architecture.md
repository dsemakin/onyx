# Architecture

## Three layers

**Data** — `spec/`, `corpus/`, `migrations/`. CC0, no code. Everything needed to
implement Onyx in any language, and to prove you did it right.

**Engine** — `crates/onyx-core`. Parses, preserves, validates, migrates. No filesystem,
no network, no wall-clock; time is an injected parameter.

**Distribution** — `crates/onyx-cli`, `crates/onyx-wasm`, `packages/npm`. Every artifact is
carried by infrastructure someone else already operates: GitHub Releases, npm, and jsDelivr
for the schema. This project hosts nothing.

The ordering matters. If the only way to implement the format were to link the engine,
the format's adoption cost would be higher than it is today, and adoption is the only
thing that matters for a format with one producer.

## Consumers, producers, processors

The specification defines a conforming producer and a conforming consumer. The engine is
a third thing, and it needs a stronger rule.

A **consumer** must *ignore* members it does not recognise. A **processor** — anything
that reads a document and writes one back out — must *preserve* them. A processor that
merely ignored unknown members would silently strip every field added by a newer
producer, so passing a 1.7 document through a 1.0 tool would destroy exactly the data the
must-ignore rule exists to protect.

In practice this means every struct carries an `extra` object for the members it does not
know, at every level of nesting, populated by hand in `codec.rs` — there is no serde in
this crate. It has been there from the first commit: retrofitting it later is painful and
the failure mode is silent.

## The round-trip guarantee

Three properties, stated precisely because "byte-identical" is the obvious phrase and it
is not quite what holds:

- **Preservation** — every member in the input is present in the output with an equal
  value, at every level.
- **Idempotence** — serializing a parsed document and parsing it again yields an equal
  document; a second cycle changes nothing.
- **Canonical order** — known members are written in the order the specification
  documents them, followed by unknown members in the relative order they arrived.

Order is canonical rather than preserved as-read. This was left open in M0 and settled in
M1: canonical output makes two exports diffable against each other regardless of which
producer wrote them, whereas preserving each producer's quirks would make every
cross-tool diff noisy. Preservation, not byte layout, is the property that protects user
data, so canonical ordering costs nothing and buys determinism. `json.rs`'s `Object`
keeps insertion order, so unknown members keep their relative order within that scheme.

`round_trip_loses_no_json_member_at_any_depth` proves the first property structurally
rather than field by field, which is what will catch a struct added later without an
`extra` map.

## Open vocabularies

`mealType`, `source`, `sex` and `direction` are open strings with examples, not closed enums.
A closed vocabulary would invalidate a document the moment a later minor added a term —
`"brunch"` in 1.1 would break every 1.0 validator in existence — which is the one place
this format cannot afford to be closed.

The engine keeps an unrecognised value as its raw string, reports it as unknown through
`is_known()`, and writes it back unchanged. Section 3.5 requires a consumer to treat such a
value as *absent* rather than as an error, and `corpus/valid/open-vocabulary-value.json`
pins that behaviour.

The first draft did use closed enums, and the corpus recorded the disagreement as data —
`"schemaValid": false` on that case — until the schema was corrected. That is what a
machine-readable expectation buys: the case moved to the positive set on its own, with
nobody editing a list.

## What the engine is actually for

Most rules in the specification are not machine-checked today. Nine are enforced by the
JSON Schema and all nine are structural; every semantic rule, and the entire consumer
half of §5, is honour-system.

The engine's job is to move rules out of that column:

- UCUM codes validated by dimension per field, so `"grams"` stops being accepted
- The Atwater cross-check, `energy ≈ 4·protein + 4·carbohydrate + 9·fat`, which catches
  per-100 g nutrients paired with a per-portion energy — the highest-consequence silent
  error in the format
- `loggedAt`, read in the offset it carries, must fall on the day it is filed under, which
  is the only way to catch a date derived via UTC. The document's `timeZone` is deliberately
  not used for this: it is the subject's home zone, not necessarily where they were when
  they ate
- IANA zone names, duplicate dates, and timestamps normalised to `Z` in violation of the
  intent behind offsets

Findings are graded: `error` for a specification violation, `warning` for something
almost certainly a bug, `info` for lossy or unusual.

## Roadmap

| | |
|---|---|
| **M0** | Scaffold, licensing, CI, `spec/v1/` vendored unchanged — *done* |
| **M1** | Typed document model, open vocabularies, preservation proven — *done* |
| **M2** | Validator: the semantic rules above, graded error/warning/info — *done* |
| **M3** | Corpus runner over `corpus/manifest.json`, all four groups — *done* |
| **M4** | CLI: plumbing/porcelain split, stable exit codes, versioned `--json` — *done* |
| **M5** | wasm to npm; `npx` working — *done, corpus passes through the binding* |
| **M6** | Migration framework, injectable manifests — *done* |
| **M7** | Release: runbook, generated SchemaStore submission, workflow — *ready to run* |
| **M8** | `examples/reader-py` passing the corpus — *done, and verified* |

Every milestone is done and verified: the workspace compiles on stable and on the declared
MSRV, the test suite passes, and the conformance corpus passes four ways — natively,
through the WebAssembly binding, against the standard-library Python reader that shares no
code with the engine, and against the JSON Schema.

One item remains that needs a producer rather than this repository: proving preservation
against **real** frozen exports. `corpus/roundtrip/producer-export-with-vendor-block.json` is
shaped exactly like a real producer's output, but it is synthetic. Capturing a set of
genuine anonymised exports and freezing them here is the outstanding piece, and it gets
harder the longer it waits.
