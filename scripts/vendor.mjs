// SPDX-License-Identifier: MIT OR Apache-2.0
//
// Keeps the copies that cargo forces us to have identical to their originals.
//
// Two of them:
//   migrations/*.json   -> crates/onyx-core/migrations/
//   LICENSES/{MIT,Apache-2.0}.txt -> LICENSE-{MIT,APACHE} in each published crate
//
// Cargo packages only what lives inside a crate directory, and `include_str!` resolves
// relative to the source file, so a crate that reaches up into the repository root builds
// fine here and fails the moment it is published. The manifests therefore have to exist
// inside the crate as well.
//
// The copy is committed rather than generated at build time, because a consumer building
// from crates.io has no repository to generate it from. `--check` fails when the two
// diverge, which is what keeps a vendored copy honest.

import { readdirSync, readFileSync, writeFileSync, mkdirSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const check = process.argv.includes("--check");
const stale = [];
let copies = 0;

function mirror(from, to, label) {
  copies += 1;
  const wanted = readFileSync(from, "utf8");
  let found = null;
  try {
    found = readFileSync(to, "utf8");
  } catch {
    /* not vendored yet */
  }
  if (found === wanted) return;
  if (check) {
    stale.push(label);
  } else {
    mkdirSync(dirname(to), { recursive: true });
    writeFileSync(to, wanted);
    console.log(`vendored ${label}`);
  }
}

// The engine embeds the manifests with include_str!, which cannot reach outside the crate.
const manifests = join(root, "migrations");
const into = join(root, "crates", "onyx-core", "migrations");
const names = readdirSync(manifests).filter((n) => n.endsWith(".json"));
// None today: one version of the specification exists, so there is nothing to migrate
// between. Do not create the directory for an empty set.
if (names.length > 0) {
  mkdirSync(into, { recursive: true });
  for (const name of names) {
    mirror(join(manifests, name), join(into, name), `migrations/${name}`);
  }
}

// Cargo will not reach up to the workspace root either, so a published crate carrying only
// the `license` field and none of the text would leave packagers with nothing to install.
for (const crate of ["onyx-core", "onyx-cli", "onyx-wasm"]) {
  // Named LICENSE-MIT and LICENSE-APACHE inside the crate: that is what packagers and
  // crates.io expect to find, whatever the canonical files are called here.
  for (const [from, to] of [["MIT.txt", "LICENSE-MIT"], ["Apache-2.0.txt", "LICENSE-APACHE"]]) {
    mirror(join(root, "LICENSES", from), join(root, "crates", crate, to), `${crate}/${to}`);
  }
}

if (stale.length > 0) {
  console.error("vendored copies are out of date:\n");
  for (const name of stale) console.error(`  ${name}`);
  console.error("\nRun: node scripts/vendor.mjs");
  process.exit(1);
}

console.log(check ? `${copies} vendored copies match their originals` : "vendoring complete");
