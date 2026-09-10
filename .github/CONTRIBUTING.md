# Contributing

Thank you for looking. This project has a specific goal — make a personal food diary
portable between applications — and contributions that move that forward are welcome
from anyone.

## The most valuable contribution

**A second implementation.** ONYX is currently implemented by one producer. A reader or
writer in any language, which passes the conformance corpus with no vendor block
present, is what turns this from one app's export format into a format. If you have
built one, open a pull request adding it to the implementations list in the README.

**A conformance case** is a close second, and it needs no Rust at all. See below.

## Setting up

You need a Rust toolchain (install via [rustup](https://rustup.rs)) and
[`just`](https://github.com/casey/just).

```sh
git clone https://github.com/dsemakin/onyx
cd onyx
just ci
```

`just ci` runs formatting, lints, tests and the corpus, in the same order CI does. If it
passes, your change is ready to review. You should not need any other command.

Tests are hermetic — no network, no wall-clock, no randomness — so a green run locally
means a green run everywhere.

## Adding a conformance case

This is deliberately the easiest contribution to make, because it is the one that keeps
the format honest.

1. Write the document as a JSON file in the right directory:
   - `corpus/valid/` — must be accepted
   - `corpus/invalid/` — must be rejected, with the expected error
   - `corpus/consumer/` — a document plus the state a conforming consumer must restore
   - `corpus/roundtrip/` — every member must survive parse-then-serialize, and a second
     cycle must change nothing. Not byte-identical: known members are written in the order
     the specification documents them, so an unusually ordered document comes back
     canonically ordered. See docs/architecture.md.
2. Register it in `corpus/manifest.json`.
3. Run `just ci`.

No Rust required. If the engine fails a case you believe is correct, that is a bug in
the engine and a good issue to open.

## Adding a validation rule

Validation rules live in `onyx-core` and must ship with at least one `valid/` and one
`invalid/` corpus case. A rule with no corpus case cannot be tested by anyone else's
implementation, which defeats the point of having a corpus.

## Changing the specification

Specification changes follow the process in [GOVERNANCE.md](GOVERNANCE.md). In short:
open an issue with the **Specification change** template first, and expect to discuss
which documents already in the wild the change would affect.

`spec/v1/` is byte-identical to what is published and to what real exports point at. It
is never edited in place; new versions land as new directories.

## Style

- [Conventional Commits](https://www.conventionalcommits.org/): `feat:`, `fix:`,
  `docs:`, `test:`, `refactor:`, `chore:`, `spec:`.
- `cargo fmt` and `cargo clippy -D warnings` are enforced in CI.
- Comments explain **why**, not what. A comment restating the code below it is noise; a
  comment explaining a non-obvious decision is the most valuable line in the file.
- No `unwrap()` or `expect()` in library code. This crate parses files from untrusted
  sources and must not panic on them.

## AI tools

You may use them. A change made with one says so, with a trailer at the end of the commit
message naming the tool and the model:

```
Assisted-by: Claude Code (Claude Fable 5.1)
```

It is a disclosure, not an authorship claim, which is why it is not `Co-authored-by`: the
person who submits the change is its author, is the one who understands it, and is the one
certifying that it can be contributed under the licences below. A tool cannot make that
certification. The engine itself was built this way, and the first commit carries the tag.

## Coding agents

If you are an LLM or an agent working in this repository, read [AGENTS.md](../AGENTS.md)
first. It lists the invariants that are load-bearing but not obvious from the code.

## Licensing of contributions

Anything executable — `crates/`, `packages/`, `scripts/` and `examples/` — is licensed under
MIT OR Apache-2.0. The specification, the conformance corpus, the migration manifests and
the documentation are released under CC0-1.0.

The split is not arbitrary. CC0 waives copyright but explicitly does not grant patent
rights, so it is the wrong instrument for code; Apache-2.0 carries that grant, and offering
MIT alongside it keeps the crates usable by projects under GPLv2, which Apache-2.0 alone is
incompatible with. For data an implementer has to copy — test fixtures above all — CC0 is
right, because any attribution requirement is friction on exactly the activity this project
wants to encourage.

The full texts are in [`LICENSES/`](../LICENSES), and [`/LICENSE`](../LICENSE) explains
which applies where.

This project is maintained by one person and has no Code of Conduct yet. One arrives with
the community it governs, alongside the move to a neutral organisation described in
[GOVERNANCE.md](GOVERNANCE.md). In the meantime, raise anything through a GitHub issue.

There is no contributor licence agreement to sign. GitHub's Terms of Service already
establish that a contribution to a repository carrying a licence notice is offered under
that same licence, so opening a pull request is all there is to it.
