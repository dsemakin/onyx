# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

The **engine crates** and the **specification** are versioned independently. Entries
tagged `spec:` refer to the format; everything else refers to the engine.

The engine and the specification both read 1.0.0 for this first release, because the engine
reached 1.0.0 by implementing the 1.0.0 specification completely. They version
independently from here and are expected to diverge.

## [Unreleased]

Initial release. There is no earlier version to compare against, so this entry describes
what ships rather than what changed. Pre-release history lives in git.

### Added

**The format.**

- `spec/v1/`: the 1.0.0 specification and JSON Schema, vendored byte-identical to what was
  published. Never edited in place; a new version is a new directory.
- Wire identity is `format: "onyx"` with media type `application/vnd.onyx+json`. The schema
  `$id` is `https://www.schemastore.org/onyx-v1.json`.
- A reader also accepts `format: "open-nutrition-log"`, the name one producer shipped
  under before the rename, because those files sit on people's devices. A writer only ever
  emits `onyx`. This alias is deliberately absent from the conformance corpus, so a new
  implementer inherits none of the renaming history.
- `spec:` The reserved extension namespace `io.github.dsemakin.onyx.demoted`, where a
  downgrade parks members the older version has no home for, keyed by the JSON Pointer each
  came from. An upgrade restores them and clears the block. This belongs to the
  specification rather than to one engine's behaviour, because it is what makes
  1.1 → 1.0 → 1.1 lossless for every implementation.

**The conformance corpus** — `corpus/`, CC0, plain data.

- Four groups: `valid` (must be accepted), `invalid` (must be rejected, with the expected
  rule), `roundtrip` (every member must survive parse-then-serialize, and a second cycle
  must change nothing) and `consumer` (a document plus the diary a conforming consumer must
  restore from it). `corpus/manifest.json` registers each case with its expectations and
  the specification section it exercises.
- Every runner declares which expectations it can check and fails on any it cannot, so an
  expectation added to the manifest breaks every implementation that has not caught up.
- The corpus is run four ways in CI: natively by the engine, through the WebAssembly
  binding, against the standard-library Python reader in `examples/reader-py/` (which
  shares no code with the engine), and against the JSON Schema with Ajv.

**The engine** — `crates/onyx-core`, MIT OR Apache-2.0, no dependencies.

- A typed model of every member in 1.0.0. Each struct carries an `extra` object for
  members this build does not know, at every level of nesting, so a document passing
  through the engine keeps everything it arrived with.
- Open vocabularies for `mealType`, `source`, `sex` and `direction`. An unrecognised value
  is kept verbatim and reported as `vocabulary/unknown` rather than rejected, so a later
  minor adding a term cannot break this build.
- A semantic validator for the rules the specification states and JSON Schema cannot
  express, graded `error` / `warning` / `info`. Among them: UCUM codes checked by
  dimension per field; an Atwater cross-check of energy against macronutrients, which
  catches per-100 g nutrients beside a per-portion energy; `loggedAt` compared against the
  day it is filed under, and a document-wide check for the one-day drift that betrays a
  date derived from UTC; duplicate and impossible dates; timestamps normalised to `Z`;
  missing or implausible IANA zone names; negative or incomplete quantities; day totals
  that disagree with their entries; `confidence` outside `0..1` or on a value that was not
  estimated. `docs/conformance.md` lists every rule, and a test holds that table to the
  code in both directions.
- Findings carry an RFC 6901 JSON Pointer and a stable rule id, so tooling can locate and
  filter without matching on prose.
- `summarise()`: a canonical restoration of a diary — per local day, the entry count,
  energy in kcal and body mass in kg — independent of any application's data model. Body
  measurements fold onto local days by the offset each timestamp carries, never by the
  document `timeZone`; when two fall on one day, the later instant wins.
- Migration between specification versions, driven by declarative manifests in
  `migrations/`. `migrate` uses the manifests this build ships — none, because one version
  of the specification exists — and `migrate_with` takes a caller-supplied set, which is
  the hook a converter for a foreign format will use. A downgrade that cannot park a member
  refuses rather than losing it, and an upgrade refuses to restore a parked member over one
  written in the meantime.
- RFC 3339 and civil-date arithmetic, UCUM unit tables and a JSON reader and writer,
  all written in-crate. Nesting is bounded so hostile input cannot exhaust the stack;
  integers stay integers so a food id is never rewritten as a float; object member order
  is preserved.

**The tools.**

- `onyx` (`crates/onyx-cli`): `validate`, `inspect` and `migrate`. Exit codes are stable —
  0 conforming, 1 read but not conforming, 2 the tool could not do its job. `validate
  --json` is a versioned public contract with one shape on every path, including failure;
  `--strict` makes warnings fail the run and is reported in the payload as `accepted` and
  `strict` alongside `conforming`, which keeps its one meaning. `--` ends option parsing.
- `@dsemakin/onyx` on npm: `npx @dsemakin/onyx validate diary.json` with nothing installed,
  on every platform from one WebAssembly artifact. `validate` and `summary`, with the same
  `--json` contract and exit codes as the binary. Nothing leaves the machine.
- `crates/onyx-wasm`: a hand-written C ABI — `validate`, `summary`, `serialize`, plus
  allocation and version calls — driven by about fifty lines of dependency-free JavaScript.
  Input is capped at 64 MiB; a failed allocation is reported rather than trapping the
  instance.
- Standalone `onyx` binaries for five targets, attached to each GitHub release.
- The crates are deliberately not on crates.io: nothing in this release needs the registry,
  and a publish there would freeze the public Rust API before anyone outside the project
  has used it. `onyx-core` is usable as a git dependency. See `docs/releasing.md`.

**For implementers and maintainers.**

- `examples/reader-py`: an Onyx reader in standard-library Python. It passes every corpus
  group, which turns two claims into facts — that the format is implementable from
  `spec/` and `corpus/` alone, and that the corpus is passable by an implementation that
  shares nothing with the engine.
- A generated SchemaStore submission (`scripts/prepare-schemastore.mjs`), which chooses
  its positive and negative tests by validating corpus cases rather than by group name.
- `docs/architecture.md`, `docs/conformance.md`, `docs/dependencies.md`,
  `docs/releasing.md`, and `AGENTS.md` for coding agents.

### Design decisions

- **No dependencies.** `onyx-core` depends on nothing; the CLI and the wasm bindings depend
  only on `onyx-core`; the npm package has none; the workflows use one first-party GitHub
  Action. The JSON layer, the error types, the argument parser and the wasm ABI are written
  here. In safe Rust a parser's worst failure is a wrong value or a panic, never memory
  corruption, and a hard nesting limit removes the one failure a caller cannot handle. What
  this costs is argued in `docs/dependencies.md`. `onyx-wasm` is the only crate with
  `unsafe`, confined to the host boundary; `onyx-core` is `#![forbid(unsafe_code)]`.
- **The round-trip guarantee is preservation plus idempotence with canonical member order**,
  not byte-for-byte stability. Every member comes back with an equal value, a second cycle
  changes nothing, and known members are written in the order the specification documents
  them. Canonical output keeps cross-producer diffs clean; preservation is the property that
  protects user data.
- **The engine never invents a member.** A required member that is absent stays absent and
  the document reports as malformed; it is not silently filled in. A member of the wrong
  shape is left in `extra` rather than discarded.
- **Warnings do not affect conformance.** Several flag things the specification explicitly
  permits; rejecting them would make the validator wrong rather than strict.
- **Parse failures and validator findings share one `category/rule` namespace**, so a
  corpus case names either the same way.
- **`%` as a portion unit is accepted with a warning** (`quantity/undefined-portion-unit`)
  rather than rejected. §3.4 gives it no meaning, but the published schema lists it, and an
  engine that rejects a schema-valid document blames a producer who trusted the schema.
  Tightening the schema is a format change and goes through GOVERNANCE.md.
- **`specVersion` is strict semver at the identity gate.** `+1.0.0`, `01.0.0` and `1.0` are
  refused as malformed, because the specification says semver and the schema's pattern can
  say only part of that. One reader serves the gate and migration, so they cannot disagree
  about which documents are versioned at all. `invalid/spec-version-with-a-leading-zero.json`
  pins it for every implementation, and records that the schema accepts what the engine
  refuses — the same kind of gap as `newer-major`.
- **Re-issued as 1.0.0 rather than bumped to 2.0.0** after the rename. A change of identity
  is a MAJOR by §4, but the only documents in the wild came from one producer and the
  `open-nutrition-log` alias keeps them readable.
- **Array-element demotion is unsupported rather than half-supported.** Removing one
  element renumbers every pointer after it, so a manifest that addresses one is refused.

### Known gaps

- No migration manifests ship: one version of the specification exists, so there is nothing
  to migrate between. The machinery is exercised only by tests.
- The weekly spec-drift check is manual until the schema is registered with SchemaStore;
  the canonical URL, which every document written by this engine carries as `$schema`,
  resolves only once that submission is accepted.
- `corpus/roundtrip/` is synthetic. Proving preservation against real, anonymised producer
  exports is the outstanding piece, and it needs a producer rather than this repository.

[Unreleased]: https://github.com/dsemakin/onyx/commits/main
