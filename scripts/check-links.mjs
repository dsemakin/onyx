// SPDX-License-Identifier: MIT OR Apache-2.0
// Verifies that every relative link in the repository's Markdown resolves to a real file.
//
// Cheap to run and catches the thing that silently rots when files move: a README that
// points at a document nobody has been able to open for six months.

import { readdirSync, readFileSync, existsSync, statSync } from "node:fs";
import { join, dirname, resolve, relative } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const SKIP = new Set([".git", "target", "node_modules", "staging", "pkg"]);

function markdownFiles(directory) {
  const found = [];
  for (const entry of readdirSync(directory, { withFileTypes: true })) {
    if (SKIP.has(entry.name)) continue;
    const path = join(directory, entry.name);
    if (entry.isDirectory()) found.push(...markdownFiles(path));
    else if (entry.name.endsWith(".md")) found.push(path);
  }
  return found;
}

const broken = [];
let checked = 0;

for (const file of markdownFiles(root)) {
  const text = readFileSync(file, "utf8");
  // [label](target) — ignoring images, anchors and absolute URLs.
  for (const match of text.matchAll(/\[[^\]]*\]\(([^)]+)\)/g)) {
    const target = match[1].split("#")[0].trim();
    if (!target || /^(https?:|mailto:)/.test(target)) continue;
    checked += 1;
    const resolved = resolve(dirname(file), target);
    if (!existsSync(resolved)) {
      broken.push(`${relative(root, file)} -> ${target}`);
    } else if (target.endsWith("/") && !statSync(resolved).isDirectory()) {
      broken.push(`${relative(root, file)} -> ${target} (not a directory)`);
    }
  }
}

if (broken.length > 0) {
  console.error(`${broken.length} broken link(s) of ${checked} checked:\n`);
  for (const entry of broken) console.error(`  ${entry}`);
  process.exit(1);
}
console.log(`${checked} relative links resolve`);
