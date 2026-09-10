// SPDX-License-Identifier: MIT OR Apache-2.0
// Checks each corpus case against spec/v1/log.schema.json, honouring the `schemaValid`
// expectation the manifest declares.
//
// Separate from the Rust corpus runner on purpose. That runner checks how the *engine*
// behaves; this checks what the *schema* accepts, and the two legitimately disagree —
// several cases are schema-valid and semantically wrong, and one is schema-invalid and
// must still be accepted. Neither check subsumes the other.
//
// Ajv is not a dependency of this project. CI installs it transiently with
// `npm install --no-save ajv@8`; nothing is committed.

import { createRequire } from "node:module";
import { readFileSync, existsSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const require = createRequire(import.meta.url);
const Ajv = require("ajv/dist/2020");

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const schema = JSON.parse(readFileSync(join(root, "spec/v1/log.schema.json"), "utf8"));
const manifest = JSON.parse(readFileSync(join(root, "corpus/manifest.json"), "utf8"));

const ajv = new Ajv({ strict: false, allErrors: true });
const validate = ajv.compile(schema);

let failures = 0;

for (const testCase of manifest.cases) {
  const relative = testCase.file ?? join(testCase.dir, "document.json");
  const path = join(root, "corpus", relative);

  if (!existsSync(path)) {
    console.log(`MISSING  ${relative}`);
    failures += 1;
    continue;
  }

  const valid = validate(JSON.parse(readFileSync(path, "utf8")));

  // Default: everything must validate, except `invalid` cases — several of those are
  // perfectly schema-valid and fail only at the engine's identity gate, which is exactly
  // why the corpus exists. A case may override with an explicit `schemaValid`.
  let expected;
  if (testCase.expect && "schemaValid" in testCase.expect) expected = testCase.expect.schemaValid;
  else if (testCase.group === "invalid") expected = null;
  else expected = true;

  const ok = expected === null || valid === expected;
  if (!ok) failures += 1;

  const detail = valid ? "" : `  <- ${ajv.errorsText(validate.errors, { separator: "; " })}`;
  const note = expected === null ? "unconstrained" : `expected ${expected}`;
  console.log(`${ok ? "ok  " : "FAIL"}  [${testCase.group}] ${relative}  schemaValid=${valid} (${note})${detail}`);
}

console.log(failures === 0 ? "\nall schema expectations met" : `\n${failures} failure(s)`);
process.exit(failures === 0 ? 0 : 1);
