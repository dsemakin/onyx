# Onyx

A portable interchange format for a personal food and body-weight diary, and the
engine that reads, validates and migrates it.

[![CI](https://github.com/dsemakin/onyx/actions/workflows/ci.yml/badge.svg)](https://github.com/dsemakin/onyx/actions/workflows/ci.yml)
[![Spec](https://img.shields.io/badge/spec-1.0.0-blue)](spec/v1/SPEC.md)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue)](#licensing)

## Why

There is no interchange format for a personal food diary. Every consumer calorie
tracker exports a proprietary CSV and none of them import each other's. Open mHealth
and IEEE 1752 cover body weight, activity and sleep across roughly 113 schemas and
have no food schema at all. HL7 FHIR `NutritionIntake` was modelled on inpatient tray
monitoring and no consumer app implements it. Apple Health has no food name field, so
item identity is lost on export.

The result is that a person's own multi-year eating history is the least portable data
they own — and "I have years of data in there" is the single most common reason people
do not move between trackers.

[**Read the specification →**](spec/v1/SPEC.md)

## Status

The specification is at **1.0.0** and is implemented by one producer. This repository
is building the reference engine; it is pre-release and the crates are not yet
published. A second implementer is actively wanted — see [GOVERNANCE.md](.github/GOVERNANCE.md).

## Quick start

Validate a diary without installing anything:

```sh
npx @dsemakin/onyx validate diary.json
```

Or download the standalone `onyx` binary for your platform from the
[releases page](https://github.com/dsemakin/onyx/releases) — no runtime needed:

```sh
onyx validate diary.json
```

Use it as a library:

```toml
[dependencies]
onyx-core = { git = "https://github.com/dsemakin/onyx", tag = "v1.0.0" }
```

The crates are not on crates.io yet; that is a deliberate choice explained in
[docs/releasing.md](docs/releasing.md#why-not-cratesio).

## Editor support

A document that carries the schema URL gets validation, autocomplete and hover
documentation in VS Code and every JetBrains IDE with nothing installed:

```json
{
  "$schema": "https://cdn.jsdelivr.net/gh/dsemakin/onyx@v1.0.0/spec/v1/log.schema.json",
  "format": "onyx",
  "specVersion": "1.0.0"
}
```

The reference engine writes that line into every document it produces. For files that
lack it, VS Code can be told by file name, in `settings.json`:

```json
"json.schemas": [
  { "fileMatch": ["*.onyx.json", "*.onx.json"],
    "url": "https://cdn.jsdelivr.net/gh/dsemakin/onyx@v1.0.0/spec/v1/log.schema.json" }
]
```

## What is in here

| Path | Contents | License |
|---|---|---|
| `spec/` | The specification and versioned JSON Schemas | CC0-1.0 |
| `corpus/` | Language-agnostic conformance suite | CC0-1.0 |
| `migrations/` | Declarative version-migration manifests | CC0-1.0 |
| `crates/` | Rust engine, CLI and wasm bindings | MIT OR Apache-2.0 |
| `packages/npm/` | The `npx`-able distribution | MIT OR Apache-2.0 |
| `examples/` | Small reference readers in other languages | MIT OR Apache-2.0 |
| `docs/` | Architecture, conformance, releasing | CC0-1.0 |
| `scripts/` | Repository checks; no build step | MIT OR Apache-2.0 |
| `.github/` | Contributing, security, governance, CI | — |
| `LICENSES/` | Full licence texts, named by SPDX identifier | — |

The first three directories are deliberately **data, not code**. Anyone can implement
Onyx from them without reading a line of Rust, and the conformance corpus
will tell them whether they got it right. That is the point: a format only one program
can read is a backup, not a standard.

## Dependencies

**None.** `onyx-core` depends on nothing; the CLI and the WebAssembly bindings depend only
on `onyx-core`. The npm package has no dependencies and the workflows use one first-party
GitHub Action.

The JSON reader, the error types, the argument parser and the WebAssembly ABI are all
written here. That is a deliberate trade, argued in full — including what it costs — in
[docs/dependencies.md](docs/dependencies.md).

## Implementations

| Implementation | Role | Conformance |
|---|---|---|
| [Burnin](https://bein.ltd) | Producer, consumer | Corpus pending |
| [examples/reader-py](examples/reader-py/) | Consumer | Passes all four groups |

The Python reader is a reference example rather than an independent implementer — it proves
the corpus is passable and the specification complete, not that anyone else has adopted the
format.

One producer is not a format. If you have built a reader or a writer in any language, run
the [conformance corpus](corpus/) against it and open a pull request adding it here — that
is the contribution this project wants most. A consumer that only reads its own files has
implemented a backup.

## Contributing

See [CONTRIBUTING.md](.github/CONTRIBUTING.md). Adding a conformance case requires no Rust at
all; it is a JSON file in `corpus/`. If you are a coding agent, start with
[AGENTS.md](AGENTS.md).

## Licensing

Code is dual-licensed under [MIT](LICENSES/MIT.txt) or [Apache-2.0](LICENSES/Apache-2.0.txt) at your
option, the Rust ecosystem convention. The specification, schemas, conformance corpus
and migration manifests are released under [CC0-1.0](LICENSES/CC0-1.0.txt) — implement them
freely, fork them, rename them. A standard that constrains its own adoption is not a
standard.
