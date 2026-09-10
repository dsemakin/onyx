# Dependencies, and what happens if nobody maintains this

**This project has no dependencies.** `onyx-core` depends on nothing at all; `onyx-cli` and
`onyx-wasm` depend only on `onyx-core`. The npm package has none. The workflows use one
first-party GitHub Action.

That is not a boast, it is the design constraint. The goal is a project still useful years
after the last commit, and every dependency is something someone else must keep alive for
that to be true.

Linux is the usual comparison, and it is worth being precise about it: the kernel has no
*runtime* dependencies, but it needs a C compiler, binutils, and at build time perl, bc,
flex and bison. What it actually has is a small, stable, slow-moving set. This project
reaches the same place from the other direction, because the Rust standard library already
provides what a JSON engine needs.

## What was removed, and what replaced it

| Removed | Replaced by | Why |
|---|---|---|
| `serde`, `serde_json` | `json.rs` and `codec.rs`, about 1,300 lines | See below |
| `thiserror` | `Display` and `Error` written out in `error.rs` | A handful of error variants do not justify a proc-macro |
| `clap` | Hand-written argument parsing in `main.rs` | Three commands and a few flags do not justify a dependency tree |
| `wasm-bindgen` | A hand-written ABI and ~50 lines of JavaScript | `0.2.x`, and coupled to `wasm-pack`'s version |
| Six GitHub Actions | `rustup`, `npm` and `gh`, preinstalled on runners | Actions rotted faster than anything else here |

## Writing our own JSON reader

This was the last and largest removal, and the objection to it deserves a straight answer.

The objection is that hand-rolling a parser for untrusted input trades an audited
implementation for a new attack surface. In C that would be decisive. In safe Rust it is
much weaker, because the failure modes are different: this crate is
`#![forbid(unsafe_code)]`, so a hostile document cannot corrupt memory. The worst it can do
is produce a wrong value or a panic.

Both are addressable, structurally rather than by hoping:

- **Wrong values** are what tests are for. `json.rs` carries fifteen, including every
  escape form, surrogate pairs, duplicate members, and a loop asserting that every prefix
  of a valid document fails cleanly rather than panicking.
- **Stack exhaustion** — the one failure a caller genuinely cannot handle, because it
  aborts the process — is prevented by a hard nesting limit of 128. Real diaries nest about
  six levels deep.
- **The genuinely hard parts are not ours.** Floating-point parsing and UTF-8 validation go
  to `std`. Nobody should write those, and we did not.

What we gained: nothing to audit that someone else wrote, no release cadence to track, and
no possibility of a dependency changing under us. What we gave up is real too — `serde` is
excellent, heavily fuzzed, and used by half the ecosystem, and a Rust caller who wants to
embed our types in their own `serde` structures now has to go through JSON text to do it.
That trade was made deliberately.

## What protects a build from changing under you

**`Cargo.lock` is committed and every CI command passes `--locked`.** With no dependencies
this matters less than it did, but it stays: it is what makes a build in five years produce
the same binary as a build today.

**Rust itself does not break code.** Code that compiles today compiles in ten years —
editions are opt-in and old ones keep working, so `edition = "2024"` is not a clock. The
MSRV is pinned at 1.85 and tested in CI so it cannot drift upward by accident.

## The part that is not code at all

The deepest answer to "what if this is abandoned" is that the important half of this
repository is not code.

`spec/`, `corpus/` and `migrations/` are CC0 data with no dependencies, no build step and
no toolchain. If every crate stopped compiling tomorrow, the specification is still
readable, the conformance suite still runs against any implementation in any language, and
the format is still implementable from the repository alone.

That is why those three directories are data rather than Rust, and why the conformance
corpus is JSON rather than a test harness. The engine is a convenience. The format is the
thing that has to survive, and it does not depend on the engine existing.

## What actually rots, and how fast

| What | Timescale | Consequence |
|---|---|---|
| `actions/checkout` | 2–3 years | One line to bump; released artifacts unaffected |
| The Rust language | never, by policy | — |
| The WebAssembly ABI | never, by W3C standard | — |
| `spec/`, `corpus/`, `migrations/` | never | They are text files |

There is nothing else on the list. That was the point.

## The rule for contributors

A pull request that adds a dependency has to argue why the alternative is worse. "It is
convenient" is not that argument, and the bar five removals have set is high: if it can be
replaced by code someone can read in one sitting, replace it.
