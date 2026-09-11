# SchemaStore submission

[SchemaStore](https://www.schemastore.org) is the catalog VS Code and every JetBrains IDE
consult to decide which JSON Schema applies to a file. Getting listed means anyone editing
an Onyx document gets validation, autocomplete and hover documentation
automatically, with nothing installed and nothing configured.

It is the highest adoption-per-effort item in the project, and it is a one-time pull
request to someone else's repository.

## Why they host the file

SchemaStore keeps contributed schemas in its own repository and serves them from
`https://www.schemastore.org/<name>.json`. That suits a project that deliberately hosts
nothing: no server, no domain, no uptime obligation. The trade is that the canonical URL
carries their name rather than the format's — a real cost, accepted knowingly. Because §2.2
puts a document's identity in `format` and `specVersion` rather than in `$schema`, moving
that URL later is a schema edit and a redirect, never a break.

Their repository runs Prettier with a key-sorting plugin over every contributed file, so
the hosted copy has `$`-prefixed keys first and different line wrapping. It is the same
schema; `spec-drift.yml` compares the two as JSON for exactly this reason. Their check also
rejects a catalog `description` ending in a full stop, and flags draft 2020-12 as a "high"
schema version, which is waived by listing the file under `highSchemaVersion` in
`src/schema-validation.jsonc`. Both are handled in the submission.

## Submitting

Build the exact file set first, so nothing drifts from `spec/` and `corpus/`:

    npm install --no-save --no-package-lock ajv@8
    node scripts/prepare-schemastore.mjs

That writes `target/schemastore/` (gitignored) and prints where each file goes. It decides
which corpus cases are positive tests and which are negative by **actually validating**
them rather than trusting the group name — most of `corpus/invalid/` is perfectly
schema-valid and fails only at the engine identity gate, so submitting those as negative
tests would fail their suite.

The sorting is derived, not hard-coded, so it stays correct as the schema changes. When
the vocabularies opened up, `open-vocabulary-value.json` moved from the negative set to
the positive one without anybody editing a list.

Then, against [SchemaStore/schemastore](https://github.com/SchemaStore/schemastore):

1. Copy `target/schemastore/onyx-v1.json` to `src/schemas/json/`.
2. Merge `target/schemastore/catalog-entry.json` into `src/api/json/catalog.json`, in alphabetical order.
3. Copy `target/schemastore/test/onyx-v1/` to `src/test/`.
4. Copy `target/schemastore/negative_test/onyx-v1/` to `src/negative_test/`.
5. Run their test suite and open the pull request.

## File naming

The catalog entry matches `*.onyx.json` and `*.onx.json`. That is an editor
convenience, **not** a normative requirement: an ONYX document identifies itself from the
inside, and a file called `export.json` is exactly as valid.

## When 1.1 lands

Register it alongside v1 rather than replacing it. Documents pointing at the v1 URL exist
and will keep existing.
