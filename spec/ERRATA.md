# Errata

Corrections to published specification versions. A published version under `spec/` is
never edited, so a statement in it that has turned out to be wrong is corrected here, with
the date and the reason. An entry stays for as long as the version it corrects is
published, which is forever; a later version incorporates the correction in its own text.

## 1.0.0

### E1 — 2026-09-12 — where the schema is served

**`SPEC.md` says** (header, "Schema") that the schema is served at
`https://www.schemastore.org/onyx-v1.json`, and §7 says it "is served by SchemaStore rather than
a vendor domain". `log.schema.json` carries that URL as its `$id`.

**What is true.** SchemaStore declined to list the schema on 2026-09-11, because their
catalog only takes formats that are already widely used, and this one had been public for
a day. The URL therefore does not resolve. The schema is served from this repository
through the jsDelivr CDN, pinned to the release tag so the bytes cannot change:

```
https://cdn.jsdelivr.net/gh/dsemakin/onyx@v1.0.0/spec/v1/log.schema.json
```

**What this changes, and what it does not.** New documents written by the reference
engine carry the served URL in `$schema`, so editors validate them with nothing configured.
The schema's `$id` is unchanged: it is the name the specification gave it, not a location,
and JSON Schema does not require a `$id` to resolve. A document's identity has never been
its `$schema` line (§2.2), so documents already carrying the SchemaStore URL remain valid
and are read exactly as before. The submission stays prepared; it will be resubmitted when
there is usage to point at, and if it is accepted the served URL can move again in the same
way, by changing what new documents carry.

### E2 — 2026-09-25 — `confidence` in the example document

**`SPEC.md` shows** (§3, the example document) an entry with `"source": "database"` and
`"confidence": 0.9`.

**What is true.** §3.5 says `confidence` "is only meaningful for `estimated`". The example
contradicts the section that defines the member, and a reader following the example writes
documents the reference engine warns about (`entry/confidence-without-estimate`). The rule
stands and the example is wrong: `confidence` belongs only on an entry whose `source` is
`estimated`. The example is kept verbatim in `corpus/valid/specification-example.json`, which
pins that warning as one of exactly two findings the example produces.
