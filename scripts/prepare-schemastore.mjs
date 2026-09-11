// SPDX-License-Identifier: MIT OR Apache-2.0
// Builds the exact file set to submit to SchemaStore, into target/schemastore/.
//
// Generated rather than committed, so the schema and its tests cannot drift from the ones
// in spec/ and corpus/. The staging directory is gitignored; run this immediately before
// opening the pull request.
//
// Which corpus cases belong in the positive set and which in the negative set is decided
// by actually validating them, not by trusting a list. That matters here: most of
// corpus/invalid/ is perfectly schema-valid and fails only at the engine's identity gate,
// so a naive "invalid means negative test" would submit tests that fail.
//
// Ajv is not a dependency of this project:
//   npm install --no-save ajv@8 && node scripts/prepare-schemastore.mjs

import { createRequire } from "node:module";
import { readFileSync, writeFileSync, mkdirSync, rmSync } from "node:fs";
import { join, dirname, basename } from "node:path";
import { fileURLToPath } from "node:url";

const require = createRequire(import.meta.url);
const Ajv = require("ajv/dist/2020");

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const NAME = "onyx-v1";

const schemaText = readFileSync(join(root, "spec/v1/log.schema.json"), "utf8");
const manifest = JSON.parse(readFileSync(join(root, "corpus/manifest.json"), "utf8"));

const ajv = new Ajv({ strict: false, allErrors: true });
const validate = ajv.compile(JSON.parse(schemaText));

const staging = join(root, "target/schemastore");
rmSync(staging, { recursive: true, force: true });
mkdirSync(join(staging, "test", NAME), { recursive: true });
mkdirSync(join(staging, "negative_test", NAME), { recursive: true });

// The schema itself. SchemaStore's Prettier will reorder its keys ($-prefixed first) and
// rewrap it before merging, which is why spec-drift.yml compares as JSON rather than bytes.
writeFileSync(join(staging, `${NAME}.json`), schemaText);

const positive = [];
const negative = [];

for (const testCase of manifest.cases) {
  const relative = testCase.file ?? join(testCase.dir, "document.json");
  const text = readFileSync(join(root, "corpus", relative), "utf8");
  const name = testCase.file ? basename(testCase.file) : `${basename(testCase.dir)}.json`;

  const bucket = validate(JSON.parse(text)) ? positive : negative;
  bucket.push(name);
  writeFileSync(join(staging, bucket === positive ? "test" : "negative_test", NAME, name), text);
}

const catalogEntry = readFileSync(join(root, "scripts/schemastore-catalog-entry.json"), "utf8");
writeFileSync(join(staging, "catalog-entry.json"), catalogEntry);

console.log(`staged target/schemastore/ for ${NAME}`);
console.log(`  ${NAME}.json                    -> src/schemas/json/`);
console.log(`  catalog-entry.json             -> merge into src/api/json/catalog.json`);
console.log(`  test/${NAME}/            (${positive.length}) -> src/test/`);
for (const name of positive) console.log(`      ${name}`);
console.log(`  negative_test/${NAME}/   (${negative.length}) -> src/negative_test/`);
for (const name of negative) console.log(`      ${name}`);

if (positive.length === 0 || negative.length === 0) {
  console.error("\nSchemaStore requires at least one test of each kind.");
  process.exit(1);
}
