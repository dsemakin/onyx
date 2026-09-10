# @dsemakin/onyx

Check an [Onyx](https://github.com/dsemakin/onyx) diary with
nothing installed:

```sh
npx @dsemakin/onyx validate diary.json
```

See what a diary actually contains, restored from the portable layer alone:

```sh
npx @dsemakin/onyx summary diary.json
```

```
Onyx 1.0.0 from Some Tracker
  2026-08-10    1 entries  389 kcal  80.5 kg
  2026-08-11    3 entries  1804 kcal
```

## Why this is one artifact

The engine is Rust compiled to WebAssembly, so this package works on every platform and
every Node version from one build — no per-platform native binaries, no install scripts,
no build matrix to fall out of step. It also means the check runs entirely on your
machine: **a food diary is sensitive, and nothing here uploads it anywhere.**

## As a library

```js
const { load } = require("@dsemakin/onyx");
const onyx = load();

const report = onyx.validate(text);   // { conforming, findings: [...] }
const days   = onyx.summary(text);    // the diary restored from the portable layer
const out    = onyx.serialize(text);  // { document } — the engine's own text, verbatim
```

Useful in another project's test suite: build a document however you build it, then check
it against the reference engine rather than against a JSON Schema alone.

`serialize` is for the third role the specification names, the **processor** — anything
that reads a document and writes one back. Where a consumer may ignore members it does not
recognise, a processor must *preserve* them, and `out.document` is what lets you prove it:
feed it a document, and every member you put in must still be there. It is a string rather
than a parsed object on purpose, so what you compare is the engine's own output and not
your JSON printer's rendering of it.

## Exit codes

| Code | Meaning |
|---|---|
| `0` | The document conforms |
| `1` | The document was read and does not conform |
| `2` | The tool could not run |

Only `error` findings affect conformance. Warnings flag things the specification permits
but which are usually mistakes; pass `--strict` to fail on them too.

`--json` emits a machine-readable report whose shape is versioned by `reportVersion` and
changed additively. It is the same contract the standalone binary emits.

## Scope

This package is the zero-install checker: `validate` and `summary`, no Node dependencies,
nothing uploaded anywhere.

The standalone `onyx` binary — attached to every GitHub release, needing no Node at all —
is **not** a superset of it. The two overlap on `validate` and diverge after that:

| | npm package | standalone binary |
|---|---|---|
| `validate` | yes | yes |
| `summary` — the diary restored from the portable layer | yes | as `inspect`, which reports document shape rather than a restored diary |
| `migrate` — rewrite a document as another spec version | no | yes |

They share one engine and one report shape, so `--json` output is the same contract from
both. What differs is which commands exist.

## Building locally

```sh
just wasm       # builds pkg/onyx.wasm with plain cargo, no wasm-pack
just npm-test   # runs the conformance corpus through the binding
```
