# Working in this repository

Guidance for coding agents and for anyone new. Read this before making a change.

## What this project is

Onyx is an interchange format for a personal food and body-weight
diary, plus a Rust engine that reads, validates and migrates it. The format is the
product. The engine exists to make the format provable, not the other way round.

## Verify with one command

```sh
just ci
```

That runs formatting, lints, tests and the conformance corpus, in the same order CI
does. If `just ci` passes, the change is ready to review. You should not need any other
command, and you should not have to reason about which subset to run.

`just` is only a runner. Every recipe in `justfile` is a plain `cargo`, `node` or
`python3` command, so on a machine without it, run those in the order `ci` lists them.

Tests are **hermetic**: no network, no wall-clock, no randomness. If you find yourself
wanting any of those in a test, that is a design problem in the code under test, not a
missing test utility.

## Engine invariants

These are load-bearing. Breaking one is a correctness bug even when every test passes.

1. **Preserve unknown members.** Every struct carries an `extra` object holding members
   this build does not know. A consumer may *ignore* members it does not recognise; a
   processor must *preserve* them. If the engine drops an unknown field, every document
   that passes through it silently loses data.

   The guarantee is **preservation and idempotence, not byte-identity**: every member
   comes back with an equal value, a second cycle changes nothing, and known members are
   written in the order the specification documents them — so an unusually ordered document
   returns canonically ordered. See `docs/architecture.md`.
2. **Never interpret `extensions`.** Vendor namespaces are opaque bytes that get carried
   through untouched. The moment this engine knows how to parse any particular app's
   private block, the layering that makes ONYX a standard collapses.
3. **Never hardcode the schema `$id`.** It is configuration. It will move.
4. **Identity comes from `format` and `specVersion`**, never from `$schema`, never from
   the filename. Documents in the wild carry several different `$schema` values and all
   of them are valid.
5. **Refuse an unknown MAJOR, accept any MINOR.** A newer major may have changed
   meanings, so misreading it is worse than refusing it. A newer minor only adds
   members, which invariant 1 already handles.
6. **No wall-clock, no randomness, no filesystem, no network in `onyx-core`.** Time is an
   injected parameter. This is what keeps the engine testable and the wasm build small.
7. **No panics on untrusted input.** This crate parses files people were emailed. Errors
   are values; `unwrap()` and `expect()` do not belong in library code.

8. **Add no dependencies.** `onyx-core` depends on nothing at all; the CLI and the
   WebAssembly bindings depend only on `onyx-core`. A pull request that introduces a crate
   needs to argue why the alternative is worse, and "it is convenient" is not that
   argument. The one exception is `fuzz/`, which is outside the workspace and never
   installed. See docs/dependencies.md.

## Format invariants

Rules of the specification itself, which the engine enforces rather than defines:

- Nutrient values are **for the portion actually consumed**, never per 100 g and never
  per serving. This is the single most common source of silent corruption when moving
  between trackers.
- `days[].date` is a **local calendar date**. It must never be derived by converting a
  timestamp to UTC.
- Every quantity is `{ value, unit }` with a UCUM code. Never a bare number, never an
  invented unit string like `"grams"`.
- Timestamps are RFC 3339 **with an offset**, and the document carries an IANA time zone
  name. An offset alone cannot answer which local day an instant belongs to.

## Do not edit `spec/v1/`

Those files are published and byte-identical to documents already in users' hands.
Changes to the format go through the process in [GOVERNANCE.md](.github/GOVERNANCE.md) and land
as a new version directory, never as an edit in place. A pull request that modifies
`spec/v1/log.schema.json` will be rejected on sight.

## Repository map

| Path | What it is | License |
|---|---|---|
| `spec/` | Specification and versioned schemas — **do not edit in place** | CC0-1.0 |
| `corpus/` | Conformance suite: plain data, no code | CC0-1.0 |
| `migrations/` | Version-migration manifests: declarative data, not Rust | CC0-1.0 |
| `crates/onyx-core/` | Parse, preserve, validate, migrate | MIT OR Apache-2.0 |
| `crates/onyx-cli/` | The `onyx` binary | MIT OR Apache-2.0 |
| `crates/onyx-wasm/` | WebAssembly bindings; the only crate with `unsafe` | MIT OR Apache-2.0 |
| `docs/` | Architecture, conformance, dependencies, releasing | CC0-1.0 |
| `.github/` | CONTRIBUTING, SECURITY, GOVERNANCE, workflows | — |
| `LICENSES/` | Full texts, named by SPDX identifier; `/LICENSE` explains the split | — |

Respect the licensing boundary. Code does not move into the CC0 directories — those hold
the specification, the corpus and the migration manifests, and they must stay implementable
without reading any Rust. Everything executable, including `examples/`, is MIT OR
Apache-2.0: CC0 waives copyright but explicitly does not grant patent rights, which is a
gap that matters for code and not for data.

## Common tasks

**Add a conformance case.** Drop a JSON file into the right `corpus/` subdirectory and
register it in `corpus/manifest.json`. No Rust required. This is the highest-value
contribution available and it is deliberately the easiest one to make.

**Add a typed field.** Update `from_object` *and* `to_json` in `codec.rs`, then add the
member to the `COMPLETE` fixture in that file's tests. The two tests there cover both
halves of the mapping — that reading claims the member, and that writing puts it back —
but only for members the fixture carries. A field added and left out of it is invisible,
and a `to_json` that forgets a field its `from_object` consumed is silent data loss.

**Add a validation rule.** It belongs in `onyx-core`, with at least one `valid/` and one
`invalid/` corpus case demonstrating it. A rule with no corpus case is unenforceable by
anyone else's implementation, which defeats the purpose.

**Change the CLI's `--json` output.** That output is a public API — other tools shell
out to it. Additive changes only; bump `reportVersion` for anything else.

## Commits and pull requests

[Conventional Commits](https://www.conventionalcommits.org/), so history stays greppable by
kind of change: `feat:`, `fix:`, `docs:`, `test:`, `refactor:`, `chore:`, `spec:`. `spec:`
marks a change to the format; everything else is the engine.

A commit you help write ends with `Assisted-by: <tool> (<model>)`, for example
`Assisted-by: Claude Code (Claude Fable 5.1)`. Never add `Co-authored-by` or
`Signed-off-by` for yourself: the human submitting the change is its author and the one
certifying the licence. See CONTRIBUTING.md.

Write comments that explain **why**, not what. The existing code does this and it is the
main reason the codebase is legible; match it. A comment restating the line below it is
noise, but a comment explaining why a non-obvious choice was made is the most valuable
thing in the file.
