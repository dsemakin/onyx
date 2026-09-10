# Conformance corpus

Plain data. No code, no Rust, no build step. Any implementation in any language can run
these cases and prove it conforms.

This exists because a JSON Schema can only check the shape of a document. It cannot check
that a *reader* behaves correctly — that it ignores members it does not recognise, refuses
a newer major, or restores a usable diary when no vendor block is present. That last one
is the real test of the format, and only a corpus can enforce it.

## Groups

| Directory | The implementation must |
|---|---|
| `valid/` | Accept the document |
| `invalid/` | Reject it, reporting the rule named in `expect.rule` |
| `consumer/` | Restore the state described in `expected.json` |
| `roundtrip/` | Preserve every member through parse-then-serialize, idempotently |

`consumer/` cases are directories containing `document.json` and `expected.json`;
everything else is a single JSON file.

## `expect`

Optional per case.

- `rule` — for `invalid` cases, the rule the implementation must report. Rules rather than
  messages, so implementations are not held to one engine's wording. The full list is in
  [`../docs/conformance.md`](../docs/conformance.md).
- `warns` — rules that must be reported at warning severity. A warning does not make a
  document non-conforming.
- `accepted` — whether a conforming consumer must accept the document.
- `schemaValid` — whether the document validates against `spec/v1/log.schema.json`.

`accepted` and `schemaValid` are separate on purpose, because they can legitimately
disagree, and they disagree in both directions:

- `invalid/utc-normalised-timestamps.json` carries timestamps normalised to `Z`. Both the
  schema and the engine reject it, because the instant survives while the local wall clock
  does not — and that is what says which day an entry belongs to.
- `invalid/duplicate-day.json` is perfectly schema-valid and wrong: nothing in JSON Schema
  can express uniqueness across `days[]`.
- `invalid/unit-not-ucum.json` is schema-**invalid**, which surprises people. The
  dimension-specific `$defs` carry unit enums, so `"grams"` in a nutrient is caught. The gap
  is elsewhere: `invalid/goal-in-the-wrong-dimension.json` puts a target weight in `kcal`
  and passes the schema completely, because a goal's `value` uses the generic
  `$defs/quantity`, whose dimension depends on an open-vocabulary `type`.
- `invalid/spec-version-with-a-leading-zero.json` is schema-valid and malformed. §4 says
  `specVersion` is semver, which gives each version one spelling — no sign, no leading zero —
  and the schema's `\d+` pattern cannot say so. It is the same gap as
  `invalid/newer-major.json`, one member over.

Where `schemaValid` is absent it defaults to `true`, except for `invalid` cases where it
is unconstrained.

## Adding a case

1. Put the file in the right group.
2. Register it in `manifest.json`, with the specification section it exercises and a
   description saying *why* the case matters.
3. Run `just ci`.

No Rust required. If the engine disagrees with a case you believe is correct, that is a
bug in the engine and a good issue to open.

Cases are CC0. Please do not contribute real personal data — a food diary is sensitive,
and a synthetic document demonstrates a rule just as well.
