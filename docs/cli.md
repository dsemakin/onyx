# `onyx`

## Commands

```sh
onyx validate <file|-> [--json] [--strict]   # check against the specification
onyx inspect  <file|->                       # print what a document contains
onyx migrate  <file|-> --to <version>         # rewrite as another spec version
```

`-` reads standard input, so `onyx` composes in a pipeline.

## Exit codes

Part of the interface. A script must be able to distinguish a non-conforming document
from a tool that could not run.

| Code | Meaning |
|---|---|
| `0` | The document conforms |
| `1` | The document was read and does not conform |
| `2` | The tool could not do its job — unreadable file, bad arguments |

Only `error` findings affect conformance. Warnings flag things the specification permits
but which are usually mistakes, so they are reported and do not change the exit code.
`--strict` makes warnings fail too, for callers who would rather stop.

## `--json` is a public API

Other tools shell out to this. The report carries its own version:

```json
{
  "reportVersion": 1,
  "conforming": false,
  "accepted": false,
  "strict": false,
  "specVersion": "1.0.0",
  "producer": "Some Tracker",
  "findings": [
    {
      "severity": "error",
      "rule": "unit/dimension",
      "path": "/days/0/entries/0/nutrients/protein/unit",
      "message": "\"grams\" is not a UCUM mass code. Expected one of: g, mg, ug, kg, [lb_av], [oz_av]."
    }
  ]
}
```

`conforming` is about the **document**: it is false when any `error` finding was raised.
`accepted` is about **this run**, and is what the exit code reports — the two differ only
under `--strict`, which rejects on warnings the specification permits. Every payload carries
the same members on every path, including the ones where the tool never reached a verdict:
if the file could not be read, `conforming`, `accepted`, `specVersion` and `producer` are
`null` rather than `false`, because no document was examined.

`path` is an RFC 6901 JSON Pointer, so an editor or a script can jump straight to the
member. `rule` is stable; `message` is not, and tooling should never match on it. The full
rule list is in [conformance.md](conformance.md).

Changes to this shape are additive only. Anything else bumps `reportVersion`. Consumers
should ignore members they do not recognise — the same rule the document format uses,
applied to the tool's own output.

Human-facing output carries no such promise and may change freely. That split is
deliberate: it is how git separates plumbing from porcelain, and it is what lets other
people build on this without being broken by cosmetic changes.

## migrate

Writes the migrated document to standard output; it never edits in place. A tool that
silently rewrites someone's diary file is not one anyone should have to trust.

Upgrading across a minor changes nothing but the version stamp, because minors only add
optional members. Downgrading parks anything the older version has no home for under a
reserved extension namespace, so migrating back restores it exactly. See
[../migrations/README.md](../migrations/README.md).

```sh
onyx migrate diary.json --to 1.1.0 > diary-1.1.0.json
```

One version of the specification exists today, so this build ships no manifests and
`migrate` refuses any target but the version already in hand. The machinery is there for
the first real version bump, and `onyx_core::migrate_with` takes a caller-supplied set for
anyone migrating across versions this build predates.

```sh
```

## What `validate` does not do

It checks the semantic rules the specification states and JSON Schema cannot express. It
does **not** check document shape against `spec/v1/log.schema.json` — use any JSON Schema
validator for that. Running both is the complete check, and neither subsumes the other:
several corpus cases are schema-valid and semantically wrong, and one is schema-invalid
and must still be accepted.
