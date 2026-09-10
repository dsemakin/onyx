# Releasing

Two things in this repository are versioned, and they move independently.

| | Versioned by | Changing it means |
|---|---|---|
| The **specification** | `specVersion` inside every document; `spec/<major>/` | The format changed. Follow [GOVERNANCE.md](../.github/GOVERNANCE.md). |
| The **engine** | crate versions in `Cargo.toml` | The implementation changed. This document. |

A patch release of `onyx-core` implies nothing about the format, and a new specification
version does not force a major bump of the engine.

Both currently read **1.0.0**, and that is a coincidence of the first release rather than a
rule: the engine reached 1.0.0 at the moment it implemented the 1.0.0 specification
completely. They are expected to diverge — the engine will reach 1.1.0 for a feature the
format did not change to allow — so read the two numbers separately even while they agree.

## What a release produces

One tag produces three things, in this order, and each later step waits for the earlier
ones so that nothing irreversible happens while something reversible can still fail:

1. A **GitHub release** named after the tag, linking to `CHANGELOG.md`.
2. **Standalone `onyx` binaries** for five targets, attached to that release. This is how
   someone with neither Rust nor Node gets the CLI.
3. The **npm package** `@dsemakin/onyx`, built from the wasm module and run against the
   corpus first. This is `npx @dsemakin/onyx validate` and `require("@dsemakin/onyx")`.

The crates are **not published to crates.io**. See [Why not crates.io](#why-not-cratesio).

## Before anything

**npm publishes are permanent.** A version can be unpublished within 72 hours and after
that it can only be deprecated, never reused or replaced. Treat it as a one-way door, and
treat the dry run below as the actual gate rather than a formality.

## Checklist

1. **Everything is green locally.**
   ```sh
   just ci        # fmt, clippy, tests, corpus, cargo package — hermetic
   just schema    # corpus against the JSON Schema; needs network for Ajv
   just npm-test  # corpus through the wasm binding
   ```

2. **`Cargo.lock` is current and committed.** `just ci` passes `--locked`, so a stale lock
   fails rather than silently resolving something new.

3. **Bump the version** in `[workspace.package]` in the root `Cargo.toml`. All three crates
   inherit it. Bump `packages/npm/package.json` to match — the npm package tracks the
   engine, not the specification.

4. **Update `CHANGELOG.md`.** Move `[Unreleased]` to the new version with a date, and open
   a fresh `[Unreleased]`.

5. **Dry run the publish.** This is the real check; nothing after it is reversible.
   ```sh
   just wasm
   cd packages/npm && npm publish --dry-run
   ```
   Read the file list npm prints. `files` in `package.json` is `bin/`, `lib/` and `pkg/`;
   if `pkg/onyx.wasm` is missing, the published package is inert.

6. **Tag and push.**
   ```sh
   git tag -a v1.0.0 -m "onyx 1.0.0"
   git push origin v1.0.0
   ```

   The tag triggers [`.github/workflows/release.yml`](../.github/workflows/release.yml), which
   runs the tests again, creates the release, attaches the binaries, builds the wasm module,
   runs the corpus through it, and publishes to npm.

7. **Verify from the outside**, as a stranger would:
   ```sh
   npx --yes @dsemakin/onyx@latest validate some-diary.json
   ```
   and download one binary from the release page and run `onyx --version`.

## Secrets the workflow needs

| Secret | Where to get it | Used for |
|---|---|---|
| `NPM_TOKEN` | npmjs.com → Access Tokens → Granular, publish scope, limited to `@dsemakin` | Publishing the npm package |

`GITHUB_TOKEN` is provided automatically and is what creates the release and uploads the
binaries. Granular npm tokens expire; when a release fails at the publish step with an
authentication error, that is the first thing to check.

## Re-running a failed release

Every publishing step skips work that already happened: the release is created only if it
does not exist, an already-uploaded binary is replaced, and npm is skipped if that version
is already there. Re-running the workflow for the same tag is the normal way to finish a
release that a transient failure interrupted.

## Why not crates.io

Nothing in the first release needs the registry. The binaries deliver the CLI, and the wasm
module carries the engine to JavaScript. Against that, a crates.io publish is permanent, and
at 1.0.0 the public Rust API is under semver from the moment it is published — before anyone
outside the project has used it. So the crates stay unpublished until someone asks for
`cargo add onyx-core`, and until then the engine is usable as a git dependency.

`just ci` still runs `cargo package` on `onyx-core`, so the crate stays publishable and the
day it is wanted is a workflow change rather than a repair. Two things to know on that day:

- **Order and a pause.** `onyx-cli` depends on `onyx-core`, and crates.io rejects the second
  publish until it has indexed the first. Poll `cargo info onyx-core@<version>` rather than
  sleeping a fixed time, and make both publishes skip an already-published version so a
  re-run is safe.
- **`onyx-cli` cannot be dry-run before `onyx-core` is on the registry.** It depends on its
  sibling by version as well as by path, and packaging resolves that from the registry.
  `--no-verify` does not avoid it. This is true of every multi-crate workspace.

## The first release also needs

**The npm scope.** The package is `@dsemakin/onyx`. On npm the scope matching your username
is yours automatically; any other scope is an organisation that has to exist before the
first publish. The names `onyx` on npm and `onyx` on crates.io both belong to other people,
which is why the package is scoped.

**The SchemaStore submission.** See [schemastore.md](schemastore.md). Doing it once gets
every VS Code and JetBrains user validation, autocomplete and hover docs on ONYX files
automatically, forever, at no ongoing cost. It is the highest adoption-per-effort item in
the project. It also matters sooner than it looks: the engine already writes
`https://www.schemastore.org/onyx-v1.json` into every new document, and that URL answers
404 until the listing lands. Once it has, re-enable the weekly schedule in
[`.github/workflows/spec-drift.yml`](../.github/workflows/spec-drift.yml).

## After a specification release

A new specification version is a different act from an engine release, and it lands first:

1. New directory under `spec/`. Published versions are never edited in place, and CI
   rejects a pull request that tries.
2. A migration manifest in `migrations/` if any member moved. Even when the answer is
   "nothing changed", the empty manifest is what tells an implementation the versions are
   connected.
3. Corpus cases for anything newly required, forbidden, or newly meaningful.
4. Register the new schema with SchemaStore alongside the old one. Old versions stay
   listed: documents pointing at them exist.
