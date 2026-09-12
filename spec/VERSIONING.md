# Versioning

Three version numbers exist in this project and they are deliberately independent.
Conflating them is the most common way a format's compatibility story falls apart.

| Number | Tracks | Where it lives |
|---|---|---|
| `specVersion` | The format | Inside every document; `spec/<major>/` |
| Crate versions | The engine | `Cargo.toml` |
| `reportVersion` | The CLI's `--json` output shape | `crates/onyx-cli` |

A patch release of `onyx-core` implies nothing about the format, and a new specification
version does not force a major bump of the engine.

## Specification semver

| | Meaning | Consumer obligation |
|---|---|---|
| MAJOR | Meanings changed | MUST refuse a major it does not know |
| MINOR | Members added | MUST accept any minor of a major it knows, ignoring unknown members |
| PATCH | Editorial only | No change required |

The must-ignore rule is what makes this work. Because unknown members are ignored,
adding a field usually requires no change at the consumer at all — which is why a
document written against 1.0.0 stays readable by a 1.7 consumer, and a 1.7 document
stays readable by a 1.0.0 consumer, with no migration step in either direction.

Vendor blocks under `extensions` version themselves independently via `blockVersion`, so
an application changing its internal storage never forces a specification bump.

## Published versions are immutable

`spec/v1/` is the schema served at the canonical URL and the one real exports already point
at; a weekly workflow checks that the served copy is the same schema. It is never edited in
place, and CI rejects any pull request that modifies a published one. A statement in a
published version that turns out to be wrong is corrected in [ERRATA.md](ERRATA.md), and
substantive corrections land as a new version directory.

## Migrations

Mechanical transformations between versions live in `migrations/` as declarative
manifests rather than as Rust. An implementation in any language can apply them without
linking this engine, which is the point — a format whose only migration path is one
binary is a format with one implementation.

Because minor versions are additive and unknown members are preserved, most migrations
are no-ops in the upgrade direction. The direction that needs real work is downgrade,
where members that the target version does not define must be parked rather than
dropped, so that downgrading and upgrading again is lossless.
