# Proving an implementation conforms

The corpus is plain data. You do not need this engine, this language, or this build
system to use it — clone the repository, read `corpus/manifest.json`, and run the cases
against your own reader.

## What each group asserts

| Group | Assertion |
|---|---|
| `valid` | The document is accepted |
| `invalid` | The document is rejected, reporting the rule named in `expect.rule` |
| `consumer` | The restored state matches `expected.json` |
| `roundtrip` | Every member survives parse-then-serialize, idempotently |

## Severity

Findings are graded, and only one grade decides conformance.

| Severity | Meaning | Affects conformance |
|---|---|---|
| `error` | A violation of the specification | Yes |
| `warning` | Almost certainly a bug, but the specification permits it | No |
| `info` | Lossy, unusual, or worth knowing | No |

Warnings deliberately do not make a document non-conforming. Several of them flag things
§3.3 explicitly allows — a day total that differs from the sum of its entries, energy and
macronutrients that do not reconcile — and rejecting those would make the validator wrong
rather than strict. Callers who would rather stop on them can pass `--strict`.

A timestamp normalised to `Z` is **not** in that category, though it reads like it should
be. §2.6 requires an offset, the schema's pattern rejects `Z`, and the local day a meal
belonged to cannot be recovered once it is gone — so it is an error, and the corpus asserts
rejection.

## Rules

Cases name a rule rather than a message, so implementations are held to the behaviour and
not to this engine's wording.

| Rule | Severity | Specification |
|---|---|---|
| `document/unreadable` | error | Not well-formed JSON |
| `document/not-onyx` | error | §2.2 — `format` is not `onyx` |
| `document/malformed-version` | error | §4 — `specVersion` is not semver-shaped |
| `document/unsupported-major` | error | §4 — a MAJOR newer than the implementation knows |
| `migration/no-path` | error | §4 — no manifest chain reaches the requested version |
| `migration/malformed-manifest` | error | A caller-supplied migration manifest is unreadable |
| `migration/cannot-demote` | error | A downgrade has members to park and `extensions` is not an object |
| `migration/cannot-promote` | error | An upgrade could not restore a parked member; it stays parked |
| `document/missing-timezone` | warning | §2.6 — no IANA zone naming the subject's home |
| `document/suspicious-timezone` | warning | §2.6 — an offset or abbreviation in place of a zone name |
| `unit/dimension` | error | §2.5 — not a UCUM code for the field's dimension |
| `quantity/incomplete` | error | §2.5 — a quantity needs both `value` and `unit` |
| `quantity/undefined-portion-unit` | warning | §3.4 — `%` is schema-legal but names no whole |
| `quantity/negative` | warning | No quantity in this format should be negative |
| `time/malformed` | error | §2.6 — not RFC 3339 with an offset |
| `time/utc-normalised` | error | §2.6 — `Z` keeps the instant and loses the local day |
| `time/day-mismatch` | info / warning | §3.2 — `loggedAt` falls on a different day than `date`; a note at one day, a warning beyond, never an error |
| `time/systematic-day-drift` | warning | §3.2 — many one-day gaps leaning the same way |
| `day/malformed-date` | error | §3.2 — not a real calendar date |
| `day/duplicate-date` | error | §3.2 — two records for one local day |
| `day/energy-mismatch` | info | §3.3 — the total may differ from its entries |
| `nutrients/atwater` | warning | §3.3 — energy and macros disagree |
| `entry/confidence-range` | error | §3.5 — confidence is defined on 0..1 |
| `entry/confidence-without-estimate` | warning | §3.5 — confidence only means something for an estimate |
| `vocabulary/unknown` | info | §3.5 — an unrecognised value is absent, not invalid |

## What the schema cannot do

Four of the corpus cases are worth reading side by side, because they are all **valid
against `spec/v1/log.schema.json`** and all wrong:

- `invalid/newer-major.json` — §4 refusal is not expressible in JSON Schema
- `invalid/goal-in-the-wrong-dimension.json` — a target weight in `kcal`, which the generic
  `$defs/quantity` cannot rule out
- `invalid/duplicate-day.json` — no uniqueness constraint exists across `days[]`
- `valid/atwater-mismatch.json` — per-100 g macros beside a per-portion energy

`invalid/unit-not-ucum.json` is deliberately **not** in that list, though it was for a long
time. `unit` carries an enum in the dimension-specific `$defs` — `energy`, `mass`, `length`,
`duration`, `portion` — so an invented code in a nutrient or a portion is caught by the
schema. It is **not** caught in a goal or a body measurement, whose `value` points at the
generic `$defs/quantity`, which cannot carry an enum: the dimension there depends on a
sibling `type` that is an open vocabulary, so the schema has no way to know whether `kg` or
`kcal` is the right answer.

And one case runs the other way: `valid/open-vocabulary-value.json` is schema-**invalid**
and must still be accepted, because a closed enum on `mealType` is a defect in v1.0.0.

That gap in both directions is the whole argument for a conformance corpus.

## The case that matters most

`consumer/foreign-document-no-vendor-block` carries no vendor namespace, so it can only be
restored from the portable layer. An implementation that passes every other case but fails
this one has built a backup format, not an interchange format — §5 says so directly, and
it is the distinction the whole project rests on.

## Claiming conformance

Open a pull request adding your implementation to the README, with a link to a passing
run. This is the most useful contribution the project can receive; the format needs a
second implementer far more than it needs more features.
