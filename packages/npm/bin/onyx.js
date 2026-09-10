#!/usr/bin/env node
// Thin shim over the wasm build. Deliberately contains no validation logic of its own:
// there must be exactly one implementation of the rules, and it is not this file.
//
// This package is the zero-install checker. The full `onyx` binary from crates.io has more
// commands; what lives here is what someone needs to answer "is my file right?" without
// installing anything.

const { readFileSync } = require("node:fs");

const EXIT_CONFORMING = 0;
const EXIT_NON_CONFORMING = 1;
const EXIT_TOOL_ERROR = 2;

const HELP = `onyx — Onyx checker

USAGE
    npx @dsemakin/onyx validate <file|-> [--json] [--strict]
    npx @dsemakin/onyx summary  <file|-> [--json]

COMMANDS
    validate    Check a document against the specification
    summary     Restore the diary from the portable layer and print it

OPTIONS
    --json          Emit a machine-readable report
    --strict        Treat warnings as failures when choosing the exit code
    --              End of options; everything after is a file name
    -h, --help      Print this help
    -V, --version   Print the version

EXIT CODES
    0   the document conforms
    1   the document was read and does not conform
    2   the tool could not do its job

\`-\` reads standard input. Nothing is uploaded anywhere; the check runs on this machine.`;

function fail(message) {
  console.error(`onyx: ${message}`);
  process.exit(EXIT_TOOL_ERROR);
}

// The same payload the Rust CLI emits for a tool error, including the null verdict fields.
// `conforming` and `accepted` are null rather than false because no verdict was reached —
// reporting one the tool never made is worse than saying nothing. The two CLIs share a
// documented contract, and it has to hold on the failure paths too, which is where this
// previously diverged.
function reportVersion() {
  // Asked of the module rather than hardcoded, so a module speaking a newer report shape
  // is not misreported as speaking this one. When the module is what failed to load there
  // is nobody to ask, and 1 is the only honest guess left.
  try {
    return wasm.reportVersion();
  } catch {
    return 1;
  }
}

function failJson(rule, message, strict) {
  console.log(
    JSON.stringify({
      reportVersion: reportVersion(),
      conforming: null,
      accepted: null,
      strict,
      specVersion: null,
      producer: null,
      findings: [{ severity: "error", rule, path: "", message }],
    })
  );
  console.error(`onyx: ${message}`);
  process.exit(EXIT_TOOL_ERROR);
}

const argv = process.argv.slice(2);

// Scanned only up to `--`, like the option parsing below. Scanning the whole of argv meant
// `onyx validate -- --help` printed the help instead of reading the file called `--help`,
// which is precisely what the marker exists to prevent.
const marker = argv.indexOf("--");
const beforeMarker = marker === -1 ? argv : argv.slice(0, marker);

if (beforeMarker.some((arg) => arg === "-h" || arg === "--help")) {
  console.log(HELP);
  process.exit(EXIT_CONFORMING);
}
if (beforeMarker.some((arg) => arg === "-V" || arg === "--version")) {
  console.log(`onyx ${require("../package.json").version}`);
  process.exit(EXIT_CONFORMING);
}

const [command, ...rest] = argv;
if (!command) {
  console.error(HELP);
  process.exit(EXIT_TOOL_ERROR);
}
if (command !== "validate" && command !== "summary") fail(`unknown command "${command}"`);

// `--` ends the options, so a file whose name begins with `--` can still be named. Without
// it such a file was unreachable by either CLI: every spelling of it parsed as a flag.
const stop = rest.indexOf("--");
const optionArgs = stop === -1 ? rest : rest.slice(0, stop);
const afterStop = stop === -1 ? [] : rest.slice(stop + 1);

const flags = optionArgs.filter((arg) => arg.startsWith("--"));
const positional = [...optionArgs.filter((arg) => !arg.startsWith("--")), ...afterStop];

// Per command, not globally. `summary --strict` was accepted in silence, which reads as
// "strictness was applied" when nothing of the kind happened.
const ALLOWED = { validate: ["--json", "--strict"], summary: ["--json"] };
for (const flag of flags) {
  if (!ALLOWED[command].includes(flag)) {
    fail(`${command} does not take ${flag}`);
  }
}
if (positional.length === 0) fail(`${command} needs a file, or \`-\` for standard input`);
if (positional.length > 1) fail(`unexpected argument "${positional[1]}"`);

const asJson = flags.includes("--json");
const strict = flags.includes("--strict");

let wasm;
try {
  wasm = require("../lib/onyx-wasm.js").load();
} catch (error) {
  const message = `wasm module unavailable (${error.message}). Run \`just wasm\` from the repository root.`;
  if (asJson) failJson("tool/engine-unavailable", message, strict);
  else fail(message);
}

let text;
try {
  text = readFileSync(positional[0] === "-" ? 0 : positional[0], "utf8");
} catch (error) {
  const message = `cannot read ${positional[0]}: ${error.message}`;
  if (asJson) failJson("tool/unreadable-input", message, strict);
  else fail(message);
}

// The wasm module can trap — an oversized document, or a panic, which `panic = "abort"`
// turns into a dead instance. Without this the exception escapes and Node exits 1, which
// this tool defines as "read and does not conform". A crash is not a verdict about the
// document, and reporting it as one is the worst of the available lies.
let report;
try {
  report = command === "validate" ? wasm.validate(text) : wasm.summary(text);
} catch (error) {
  const message = `the engine could not process this document: ${error.message}`;
  if (asJson) failJson("tool/engine-failed", message, strict);
  else fail(message);
}

function printFindings(findings) {
  for (const finding of findings ?? []) {
    console.error(`${finding.severity}: ${finding.rule} at ${finding.path || "/"}`);
    console.error(`  ${finding.message}`);
  }
}

// `--strict` is this process's flag, not the engine's, so the verdict is narrowed here —
// the same split the Rust CLI makes: `conforming` is about the document, `accepted` is
// about this run.
const warnings = (report.findings ?? []).filter((f) => f.severity === "warning").length;
const accepted = report.conforming && !(strict && warnings > 0);
if (asJson) {
  console.log(JSON.stringify({ ...report, accepted, strict }));
} else if (command === "summary") {
  // Findings go to stderr and the diary still goes to stdout. A document can be worth
  // reading and non-conforming at once, and printing nothing at all — which is what this
  // did — leaves someone with a silent non-zero exit and no idea why.
  printFindings(report.findings);
  console.log(`Onyx ${report.specVersion} from ${report.producer}`);
  for (const day of report.days ?? []) {
    const energy = day.energyKcal === null ? "—" : `${Math.round(day.energyKcal)} kcal`;
    const mass = day.bodyMassKg === null ? "" : `  ${day.bodyMassKg.toFixed(1)} kg`;
    console.log(`  ${day.date}  ${String(day.entryCount).padStart(3)} entries  ${energy}${mass}`);
  }
} else {
  printFindings(report.findings);
  if ((report.findings ?? []).length === 0) {
    console.log(`conforming: Onyx ${report.specVersion} from ${report.producer}`);
  }
}

// Warnings do not make a document non-conforming; --strict is for callers who disagree.
// One computation, used by both the payload and the exit code, so the two cannot disagree.
process.exit(accepted ? EXIT_CONFORMING : EXIT_NON_CONFORMING);
