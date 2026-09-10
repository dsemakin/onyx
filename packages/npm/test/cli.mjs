// Runs the npm CLI as a process, which nothing did.
//
// smoke.mjs exercises the wasm binding through `load()`. That is the engine, not the
// command: exit codes, flag parsing and the `--json` payload live in bin/onyx.js and had
// never been executed by a test. The consequence was visible — the Rust CLI grew a JSON
// payload for unreadable files and this one did not, and the two drifted apart on the
// contract they are documented as sharing.
//
// No dependencies. Requires `just wasm` to have run.

import { spawnSync } from "node:child_process";
import { existsSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, "../../..");
const cli = join(here, "../bin/onyx.js");

if (!existsSync(join(here, "../pkg/onyx.wasm"))) {
  console.error("wasm build missing. Run `just wasm` from the repository root first.");
  process.exit(2);
}

const failures = [];
const run = (...args) => spawnSync(process.execPath, [cli, ...args], { encoding: "utf8" });
const check = (label, actual, expected) => {
  if (actual !== expected) failures.push(`${label}: expected ${expected}, got ${actual}`);
};

const valid = join(root, "corpus/valid/minimal.json");
const invalid = join(root, "corpus/invalid/duplicate-day.json");
const warns = join(root, "corpus/valid/atwater-mismatch.json");

// The three exit codes are the interface.
check("conforming document", run("validate", valid).status, 0);
check("non-conforming document", run("validate", invalid).status, 1);
check("missing file", run("validate", "no-such-file.json").status, 2);
check("unknown command", run("frobnicate", valid).status, 2);
check("no file given", run("validate").status, 2);
check("unknown flag", run("validate", valid, "--nope").status, 2);

// --strict narrows the verdict without changing what the document is.
check("warnings alone pass", run("validate", warns).status, 0);
check("--strict rejects on warnings", run("validate", warns, "--strict").status, 1);

// One payload shape on every path, including the ones with no verdict.
const KEYS = ["reportVersion", "conforming", "accepted", "strict", "specVersion", "producer", "findings"];
for (const args of [
  ["validate", valid, "--json"],
  ["validate", invalid, "--json"],
  ["validate", "no-such-file.json", "--json"],
]) {
  const result = run(...args);
  let payload;
  try {
    payload = JSON.parse(result.stdout);
  } catch {
    failures.push(`${args.join(" ")}: printed no JSON (stdout was ${JSON.stringify(result.stdout.slice(0, 80))})`);
    continue;
  }
  for (const key of KEYS) {
    if (!(key in payload)) failures.push(`${args.join(" ")}: payload is missing ${key}`);
  }
}

// A tool error reports no verdict, rather than a negative one.
const unreadable = JSON.parse(run("validate", "no-such-file.json", "--json").stdout);
for (const key of ["conforming", "accepted", "specVersion", "producer"]) {
  if (unreadable[key] !== null) failures.push(`unreadable file: ${key} should be null, got ${unreadable[key]}`);
}
check("unreadable file echoes strict", unreadable.strict, false);

// --strict is reported as well as acted on.
const strict = JSON.parse(run("validate", warns, "--json", "--strict").stdout);
check("--strict: conforming is about the document", strict.conforming, true);
check("--strict: accepted is about the run", strict.accepted, false);

// Options belong to one command each. `summary --strict` was accepted in silence, which
// reads as though strictness had been applied to something.
check("summary rejects --strict", run("summary", valid, "--strict").status, 2);
// A document with a timeZone and no warnings at all: `minimal.json` warns about the
// missing zone, so --strict rightly rejects it and it cannot show that the flag is allowed.
const quiet = join(root, "corpus/valid/goal-type-outside-the-vocabulary.json");
check("validate accepts --strict", run("validate", quiet, "--strict").status, 0);

// `--` ends the options, so a file whose name looks like a flag can still be named.
//
// The argument must literally be `--help`. The first version passed join(tmpdir, "--help"),
// an absolute path that begins with a drive letter or a slash — so it tested nothing, and a
// CLI whose help scan ignored the marker entirely passed it.
import { mkdtempSync, copyFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
const dir = mkdtempSync(join(tmpdir(), "onyx-npm-cli-"));
copyFileSync(valid, join(dir, "--help"));
const inDir = (...args) => {
  if (!args.includes("--help")) throw new Error("the argument must itself look like a flag");
  return spawnSync(process.execPath, [cli, ...args], { encoding: "utf8", cwd: dir });
};

const escaped = inDir("validate", "--", "--help");
check("`validate -- --help` reads the file", escaped.status, 0);
if (escaped.stdout.includes("USAGE")) failures.push("the help was printed despite the marker");

// Without the marker it is a flag, which is what the marker exists to escape.
if (!inDir("validate", "--help").stdout.includes("USAGE")) {
  failures.push("without `--`, `--help` should still be the flag");
}
rmSync(dir, { recursive: true, force: true });

// Every tool-error path answers in JSON when JSON was asked for, not only the file-read one.
const engineless = spawnSync(process.execPath, [cli, "validate", valid, "--json"], {
  encoding: "utf8",
  env: { ...process.env, ONYX_WASM_PATH: join(dir, "nonexistent.wasm") },
});
check("engine-unavailable exits 2", engineless.status, 2);
try {
  const payload = JSON.parse(engineless.stdout);
  for (const key of KEYS) {
    if (!(key in payload)) failures.push(`engine-unavailable payload is missing ${key}`);
  }
  if (payload.conforming !== null) failures.push("engine-unavailable should report no verdict");
} catch {
  failures.push(`engine-unavailable printed no JSON under --json (stdout: ${JSON.stringify(engineless.stdout.slice(0, 60))})`);
}

if (failures.length > 0) {
  console.error(`${failures.length} failure(s) in the npm CLI:\n`);
  for (const failure of failures) console.error(`  ${failure}`);
  process.exit(1);
}

console.log("npm CLI behaves as documented");
