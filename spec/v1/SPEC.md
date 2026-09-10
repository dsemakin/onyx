# Onyx — Specification v1.0.0

**Onyx** is the *Open Nutrition Information eXchange*, with the Y a stylization of the I
in the way cURL capitalizes URL.

**Status:** Draft. Implemented by one producer (Burnin). Seeking a second implementer.
**Media type:** `application/vnd.onyx+json`
**File extension:** `.onyx.json`, or `.onx.json`. Both keep the `.json` suffix so
editors and tooling still see JSON; neither is normative, since a document identifies
itself from the inside.
**Schema:** `https://www.schemastore.org/onyx-v1.json`
**Formerly:** Published as `open-nutrition-log` before being renamed. Readers should
accept that identifier in the `format` member alongside `onyx`; writers emit only `onyx`.
**License:** This specification is released under CC0. Implement it freely, fork it, rename it.

---

## 1. Why this exists

There is no interchange format for a personal food diary.

Every consumer calorie tracker exports a proprietary CSV, and none of them import each other's. MyFitnessPal emails a zip of three CSVs. Cronometer writes a `servings` CSV; its users have asked in public since 2024 what JSON it would accept, and there is no answer. MacroFactor exports a spreadsheet and imports nothing from a file. Lifesum has no self-serve export at all. Column sets, nutrient vocabularies, serving models, and date encodings differ across all of them.

The formal standards do not cover this either:

- **Open mHealth** and **IEEE 1752** have body weight, activity, sleep, blood pressure — around 113 schemas — and **no food, meal, or macronutrient schema at all**. `calories-burned` is expenditure, not intake. IEEE 1752.1 is published, 1752.2 is in progress for cardiovascular/respiratory measures, and there is no nutrition part in existence or planned. This is the closest prior art and it stopped one step short.
- **HL7 FHIR** `NutritionIntake` did not exist before R5 (2023), where it is Trial Use and modelled on inpatient tray monitoring — the canonical example records consumption as "100% of the item served". R6 finally adds the structure a food log needs, and R6 is not published. No consumer app implements it.
- **Apple Health** export is a de facto format with no published schema, a malformed inline DTD, non-ISO-8601 dates, and — critically — **HealthKit has no food name field**, so item identity is lost.
- **Health Connect** has a good `NutritionRecord` type (it does have `name`), but its export is an undocumented SQLite backup with no stability commitment.
- **schema.org** `NutritionInformation` describes what is in a recipe, never who ate what when. `EatAction` has no quantity property.
- **Open Food Facts** and **USDA FoodData Central** solved the *food database* problem as a commons. Neither defines a consumption log.

The gap is not an oversight. The database layer was solved, and the transport layer was absorbed by HealthKit and Health Connect — on-device APIs, not files — which is good enough that no vendor feels pain. Incumbents have a positive incentive against portability.

The result is that a person's own multi-year eating history is the least portable data they own.

## 2. Design principles

These are the constraints this format holds itself to. They are stated so a second implementer can hold it to them.

1. **Neutrally named.** The most consequential decision Google made with GTFS was renaming it from *Google* Transit Feed Specification to *General*, which is what let competitors adopt it. Nothing in this specification is named after any app.
2. **Self-describing.** `format` and `specVersion` are inside the document. A file identifies itself without its filename, its URL, or any out-of-band knowledge.
3. **Open for extension.** The top level is **not** a closed schema. Consumers **MUST** ignore members they do not recognise. This is what makes a minor version non-breaking, and it is the single most common way formats become unextendable.
4. **Vendor data is namespaced.** App-private data lives under `extensions`, keyed by reverse-DNS. A producer can round-trip its own data losslessly without pushing any of it into the shared schema.
5. **Units are explicit and standard.** Every quantity is `{ value, unit }` with a **UCUM** code. Never a bare number whose unit is implied by the field name, and never an invented string like `"grams"`.
6. **Time is unambiguous.** Timestamps are RFC 3339 carrying **the subject's local offset at that moment**, never normalised to UTC. `Z` keeps the instant and discards the local wall clock, and the local wall clock is what answers which day a meal belonged to. The document also carries an **IANA time zone name** as context.
7. **Food identity is a bag, not a key.** A food carries any subset of barcode, USDA id, and Open Food Facts id, plus a plain-text name that always survives when none of them resolve.
8. **A diary entry degrades gracefully.** Everything except a timestamp is optional. A file with nothing but names, times and calories is a valid, useful log.

## 3. Document structure

```json
{
  "$schema": "https://www.schemastore.org/onyx-v1.json",
  "format": "onyx",
  "specVersion": "1.0.0",
  "exportedAt": "2026-08-16T09:12:00+03:00",
  "timeZone": "Europe/Berlin",
  "producer": { "name": "Burnin", "version": "1.2.0", "platform": "android" },

  "subject": {
    "sex": "female",
    "age": { "value": 34, "unit": "a" },
    "height": { "value": 165, "unit": "cm" },
    "preferredUnits": { "mass": "kg", "energy": "kcal" }
  },

  "goals": [
    { "type": "targetWeight", "value": { "value": 62, "unit": "kg" } },
    { "type": "weightDirection", "direction": "loss" }
  ],

  "days": [
    {
      "date": "2026-08-10",
      "energyTarget":   { "value": 2100, "unit": "kcal" },
      "energyConsumed": { "value": 1800, "unit": "kcal" },
      "entries": [
        {
          "loggedAt": "2026-08-10T08:30:00+03:00",
          "mealType": "breakfast",
          "name": "Oats",
          "identifiers": { "fdcId": 169705, "gtin": "5011234567890" },
          "quantity": { "value": 100, "unit": "g" },
          "servingDescription": "1 cup",
          "nutrients": {
            "energy":       { "value": 389, "unit": "kcal" },
            "protein":      { "value": 16.9, "unit": "g" },
            "carbohydrate": { "value": 66, "unit": "g" },
            "fat":          { "value": 6.9, "unit": "g" }
          },
          "source": "database",
          "confidence": 0.9
        }
      ]
    }
  ],

  "bodyMeasurements": [
    { "observedAt": "2026-08-10T07:00:00+03:00", "type": "bodyMass",
      "value": { "value": 80.4, "unit": "kg" } }
  ],

  "extensions": {
    "ltd.bein.burnin": { "blockVersion": 1, "entries": { "…": "…" } }
  }
}
```

### 3.1 Required members

Only four: `format`, `specVersion`, `exportedAt`, `producer`. Everything else is optional. A document with no `days` and no `extensions` is well-formed but carries nothing; consumers may reject it as empty.

### 3.2 `days[].date`

A **local calendar date**, `YYYY-MM-DD`. This is the day the subject considers the entry to belong to, which is not always derivable from an instant — a meal logged at 01:00 may belong to the previous day in the user's mind. Producers **MUST NOT** compute this by converting a timestamp to UTC.

### 3.3 `nutrients`

Values are **for the portion actually consumed**, never per 100 g and never per serving. This is the single most common source of silent error when converting between trackers, so it is stated normatively rather than left to convention.

v1.0 defines `energy`, `protein`, `carbohydrate`, `fat`. Micronutrients are a deliberate omission from v1.0 and are the most likely content of v1.1; they will be added as optional members of `nutrients`, which under the must-ignore rule is not a breaking change.

### 3.4 `quantity`

The amount actually consumed, as `{ value, unit }` like every other quantity.

A dimensional unit is that amount directly: `{ "value": 100, "unit": "g" }` is a hundred grams. The dimensionless UCUM code `1` is a **count of the servings named by `servingDescription`**, so `{ "value": 2, "unit": "1" }` beside `"servingDescription": "slice"` means two slices. A producer using `1` **SHOULD** provide `servingDescription`, or the count refers to nothing.

Consumers must not assume a dimension. Both forms are legal and a producer may emit either, so read the unit before doing arithmetic.

### 3.5 `source`

How the *nutrition data* was obtained, not how it was typed:

| Value | Meaning |
|---|---|
| `manual` | The subject entered the numbers |
| `barcode` | Exact product from a scanned barcode |
| `database` | Matched to a food database entry |
| `estimated` | Estimated, e.g. by a model, with no database match |

`confidence` (0..1) is only meaningful for `estimated`.

The values above are **not exhaustive**, and neither are those of `mealType`, `sex` or a goal's `direction`. A consumer **MUST** treat a value it does not recognise as absent rather than as an error. A closed vocabulary would invalidate a document the moment a later MINOR added a term, which is the one place this format cannot afford to be closed.

### 3.6 `bodyMeasurements`

A separate observation series rather than a property of a day, because a weigh-in is not a food event and a subject may record several. `type` is `bodyMass` in v1.0.

Consumers fold these onto local days using **the offset `observedAt` carries**, not the document `timeZone`. A single document-level zone cannot describe a subject who travelled: a 08:00 weigh-in in Tokyo, exported later from Berlin, would convert to 01:00, and one taken at 00:30 would move to the previous day. The offset knows where the subject was; the document zone does not. This also makes the rule implementable without a time zone database.

### 3.7 `extensions`

Keys are reverse-DNS (`ltd.bein.burnin`). Consumers **MUST** ignore namespaces they do not own. A producer reading back its own document **SHOULD** prefer its own namespace, since that is lossless; it **MUST** still be able to restore from the portable layer alone, or the format is a backup rather than a standard.

## 4. Versioning

`specVersion` is semver for the specification.

- **MAJOR** — meanings changed. Consumers **MUST** refuse a major they do not know rather than misread it.
- **MINOR** — members added. Consumers **MUST** accept any minor of a major they know, ignoring unknown members.
- **PATCH** — editorial only.

The must-ignore rule is what makes this work: because unknown members are ignored, adding a field usually needs no version bump at the consumer at all.

Vendor blocks version themselves independently (`blockVersion`), so an app changing its internal storage never forces a spec bump.

## 5. Conformance

A **conforming producer** writes a document valid against the schema, with UCUM units and RFC 3339 offsets, and puts nothing app-specific outside `extensions`.

A **conforming consumer** accepts any document whose MAJOR it knows; ignores unknown members at every level; ignores foreign `extensions` namespaces; and restores a usable diary from the portable layer with no vendor block present.

The second half of that is the real test. A consumer that only reads its own files has implemented a backup.

## 6. Known gaps in v1.0

Stated plainly, because a spec that hides its gaps wastes implementers' time.

- **No micronutrients.** Cronometer tracks 80+ nutrients; this tracks 4. Highest-priority addition.
- **No recipes or composite foods.** A dish is a flat entry, so its ingredients are lost.
- **No exercise or energy expenditure.** Deliberate: Open mHealth and IEEE 1752 already cover activity well, and duplicating them would be worse than referencing them.
- **No hydration.** Trivial to add, not yet needed.
- **`age` rather than birth date.** Age is what a tracker typically stores, but it silently ages out of date. A future minor should add optional `birthDate`.
- **No signature or integrity check.** A document is trusted as far as its source is.
- **`sex` is a single field** conflating birth sex and gender, because that is what nutrition formulas consume. Anything more nuanced belongs in an extension until there is a real requirement.

## 7. Adoption

A spec alone never wins. The recipe-format graveyard is proof: `schema.org/Recipe` and h-recipe succeeded because Google conditioned search visibility on them, while every purely community-driven attempt over twenty-five years failed despite far more hobbyist energy than food diaries attract.

So the useful next steps are about giving a *second* implementer a reason, not about polishing the schema:

1. **Ship converters, not just a spec.** Importers for MyFitnessPal, Cronometer and MacroFactor CSV would make ONYX valuable at n=1, before anyone else adopts it.
2. **Court the FOSS trackers first.** OpenNutriTracker, Waistline and wger all have open export code, documented user demand for diary import, and no incentive to lock users in. They are the realistic second implementers; the incumbents are not.
3. **Move governance off one party.** The schema is served by SchemaStore rather than a vendor domain, which removes the worst of the problem. If a second implementer adopts the format, changes should need more than one party's approval — the GTFS lesson again.
4. **Watch the EU.** The European Health Data Space (published March 2025) creates a voluntary interoperability label for wellness applications under Article 31, registered publicly under Article 32. That is currently the only external forcing function on the horizon for this domain.

## 8. Prior art consulted

- HL7 FHIR R5 / R6-ballot `NutritionIntake`, `NutritionProduct`, `Observation` (LOINC 29463-7 body weight)
- Open mHealth schema library; IEEE 1752.1-2021, P1752.2
- Apple HealthKit nutrition type identifiers and the Health export XML
- Android Health Connect `NutritionRecord`, `WeightRecord`
- Open Food Facts and USDA FoodData Central identifier schemes
- UCUM unit codes; RFC 3339; RFC 6838/6839 media type registration; JSON Schema `$id` conventions
- ActivityStreams 2.0 extension and promotion model
- GTFS governance history; GPX
