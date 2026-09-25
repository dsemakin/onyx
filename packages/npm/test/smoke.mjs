// Runs the conformance corpus through the wasm build.
//
// The Rust corpus runner already proves the engine is correct. This proves the *binding*
// is correct — that strings survive the marshalling in both directions and that the JSON
// the JavaScript side receives says what the Rust side meant. Those are different
// failures, and only this test catches the second one.
//
// Requires `just wasm` to have run. Uses no dependencies.

import { createRequire } from "node:module";
import { readFileSync, existsSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const require = createRequire(import.meta.url);
const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, "../../..");
const wasmFile = join(here, "../pkg/onyx.wasm");

if (!existsSync(wasmFile)) {
  console.error("wasm build missing. Run `just wasm` from the repository root first.");
  process.exit(2);
}

const wasm = require(join(here, "../lib/onyx-wasm.js")).load(wasmFile);
const corpus = join(root, "corpus");
const manifest = JSON.parse(readFileSync(join(corpus, "manifest.json"), "utf8"));

const failures = [];
const close = (a, b) => Math.abs(a - b) <= 1e-6 * Math.max(Math.abs(a), Math.abs(b), 1);

// Every `expect` key this runner knows how to check. A runner that skips an expectation it
// does not implement reports a pass it never established. Round ten added this list to the
// Rust and Python runners after `notes` went unchecked in one of them; this runner had the
// same gap — it never checked `notes` at all — and nothing noticed, because nothing asked.
const UNDERSTOOD = new Set(["warns", "notes", "rule", "schemaValid", "accepted", "exhaustive"]);

function expectationsAreUnderstood(testCase, name) {
  const unknown = Object.keys(testCase.expect ?? {}).filter((key) => !UNDERSTOOD.has(key));
  for (const key of unknown) failures.push(`${name}: the manifest expects "${key}", which this runner cannot check`);
  return unknown.length === 0;
}

function checkValid(report, testCase, name) {
  const findings = report.findings ?? [];
  const errors = findings.filter((f) => f.severity === "error");
  if (!report.conforming || errors.length > 0) {
    failures.push(`${name}: expected acceptance, got ${errors.map((f) => f.rule).join(", ") || "conforming=false"}`);
    return;
  }
  const expect = testCase.expect ?? {};
  for (const [key, severity, noun] of [["warns", "warning", "a warning"], ["notes", "info", "a note"]]) {
    for (const rule of expect[key] ?? []) {
      const found = findings.some((f) => f.rule === rule && f.severity === severity);
      if (!found) failures.push(`${name}: expected ${noun} "${rule}"`);
    }
  }
  // `warns` and `notes` say what must be reported, never what must not. With `exhaustive`,
  // the listed findings are the only ones allowed.
  if (expect.exhaustive === true) {
    const allowed = new Set([
      ...(expect.warns ?? []).map((rule) => `warning ${rule}`),
      ...(expect.notes ?? []).map((rule) => `info ${rule}`),
    ]);
    const extra = findings.filter((f) => !allowed.has(`${f.severity} ${f.rule}`));
    if (extra.length > 0) {
      failures.push(`${name}: \`exhaustive\`, but also reported ${extra.map((f) => `${f.rule}(${f.severity}) at ${f.path}`).join(", ")}`);
    }
  }
}

function checkInvalid(report, testCase, name) {
  const expected = testCase.expect?.rule;
  if (!expected) {
    failures.push(`${name}: an invalid case must name expect.rule`);
    return;
  }
  const errors = (report.findings ?? []).filter((f) => f.severity === "error").map((f) => f.rule);
  if (!errors.includes(expected)) {
    failures.push(`${name}: expected error "${expected}", got ${errors.join(", ") || "none"}`);
  }
}

// Every member of `before` must survive somewhere in `after`, at any depth. Structural
// rather than field-by-field, so it still catches a member the engine learns to parse but
// forgets to write — the silent way preservation breaks.
function contained(before, after, path, name) {
  if (before !== null && typeof before === "object" && !Array.isArray(before)) {
    if (after === null || typeof after !== "object" || Array.isArray(after)) {
      failures.push(`${name}: ${path} stopped being an object`);
      return;
    }
    for (const [key, value] of Object.entries(before)) {
      if (!(key in after)) {
        failures.push(`${name}: ${path}/${key} was dropped`);
        continue;
      }
      contained(value, after[key], `${path}/${key}`, name);
    }
    return;
  }

  if (Array.isArray(before)) {
    if (!Array.isArray(after)) {
      failures.push(`${name}: ${path} stopped being an array`);
      return;
    }
    if (before.length !== after.length) {
      failures.push(`${name}: ${path} changed length`);
      return;
    }
    before.forEach((item, index) => contained(item, after[index], `${path}/${index}`, name));
    return;
  }

  if (before !== after) failures.push(`${name}: ${path} changed value`);
}

// Parse then serialize must lose nothing and must be idempotent — the same three
// assertions the Rust corpus runner makes, but through the binding, where the document
// crosses the boundary as text in both directions.
function checkRoundtrip(text, name) {
  // Keyed on `document`, not on `conforming`: a document may be non-conforming and still
  // have to survive a processor intact. `conforming` answers a different question.
  const once = wasm.serialize(text);
  if (typeof once.document !== "string") {
    failures.push(`${name}: the binding could not serialize the document`);
    return;
  }

  const twice = wasm.serialize(once.document);
  if (typeof twice.document !== "string") {
    failures.push(`${name}: the engine could not read back its own output`);
    return;
  }
  if (twice.document !== once.document) {
    failures.push(`${name}: serializing twice produced different text; the cycle is not idempotent`);
    return;
  }

  contained(JSON.parse(text), JSON.parse(once.document), "", name);
}

function checkConsumer(text, folder, name) {
  const expected = JSON.parse(readFileSync(join(corpus, folder, "expected.json"), "utf8"));
  const restored = wasm.summary(text);

  if (!Array.isArray(restored.days)) {
    failures.push(`${name}: summary could not read the document`);
    return;
  }
  if (restored.days.length !== expected.days.length) {
    failures.push(`${name}: expected ${expected.days.length} day(s), restored ${restored.days.length}`);
    return;
  }
  expected.days.forEach((want, index) => {
    const got = restored.days[index];
    if (got.date !== want.date) failures.push(`${name}: day ${index} expected ${want.date}, got ${got.date}`);
    if (got.entryCount !== want.entryCount) failures.push(`${name}: ${want.date} entryCount ${got.entryCount} != ${want.entryCount}`);
    for (const field of ["energyKcal", "bodyMassKg"]) {
      const a = got[field] ?? null;
      const b = want[field] ?? null;
      const same = a === null && b === null ? true : a !== null && b !== null && close(a, b);
      if (!same) failures.push(`${name}: ${want.date} ${field} expected ${b}, restored ${a}`);
    }
  });
}

for (const testCase of manifest.cases) {
  const name = testCase.file ?? `${testCase.dir}/document.json`;
  const text = readFileSync(join(corpus, name), "utf8");

  if (!expectationsAreUnderstood(testCase, name)) continue;

  if (testCase.group === "consumer") {
    checkConsumer(text, testCase.dir, name);
    continue;
  }

  if (testCase.group === "roundtrip") {
    checkRoundtrip(text, name);
    continue;
  }

  const report = wasm.validate(text);
  if (report.reportVersion !== 1) failures.push(`${name}: unexpected reportVersion ${report.reportVersion}`);

  if (testCase.group === "valid") checkValid(report, testCase, name);
  else if (testCase.group === "invalid") checkInvalid(report, testCase, name);
}

// The binding must agree with the engine about which version it implements.
if (typeof wasm.specVersion !== "function") failures.push("specVersion is not exported");
else if (!/^\d+\.\d+\.\d+$/.test(wasm.specVersion())) failures.push(`specVersion returned ${wasm.specVersion()}`);

if (failures.length > 0) {
  console.error(`${failures.length} failure(s) through the wasm binding:\n`);
  for (const failure of failures) console.error(`  ${failure}`);
  process.exit(1);
}

console.log(`${manifest.cases.length} corpus cases passed through the wasm binding`);
