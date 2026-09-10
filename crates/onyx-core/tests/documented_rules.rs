//! Holds `docs/conformance.md`'s rules table to what the engine actually does.
//!
//! That table is not decoration. `docs/conformance.md` tells a second implementer they are
//! held to the *behaviour* rather than to this engine's wording, and the table is where the
//! behaviour is written down — so a row that disagrees with the code is worse than no row
//! at all. It sends someone away to build the wrong thing, and the corpus then fails them
//! for it without ever explaining why.
//!
//! It has already happened once: `time/utc-normalised` was documented as a warning long
//! after the engine started raising it as an error. Nothing noticed, because nothing was
//! looking. This is what looks.
//!
//! The engine's source is read as text rather than reflected over, because severities are
//! written at each call site and there is no catalogue to consult. That makes this a
//! scanner, with the fragility scanners have — so it asserts it found a plausible number of
//! rules before comparing anything, and fails loudly if it ever scans up empty.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

type Rules = BTreeMap<String, BTreeSet<String>>;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read(relative: &str) -> String {
    std::fs::read_to_string(root().join(relative))
        .unwrap_or_else(|error| panic!("{relative}: {error}"))
}

/// Everything before `#[cfg(test)]`, so a rule named only inside a unit test is not mistaken
/// for one the engine can actually raise.
fn production_source(relative: &str) -> String {
    let text = read(relative);
    match text.find("#[cfg(test)]") {
        Some(end) => text[..end].to_owned(),
        None => text,
    }
}

/// Reads the string literal that starts at or after `from`, if one begins within `window`
/// bytes. Rules always contain a `/`, which is what separates them from other literals.
fn literal_after(text: &str, from: usize, window: usize) -> Option<String> {
    let bytes = text.as_bytes();
    let limit = (from + window).min(bytes.len());
    let open = (from..limit).find(|&i| bytes[i] == b'"')?;
    let close = (open + 1..bytes.len()).find(|&i| bytes[i] == b'"')?;
    let literal = &text[open + 1..close];
    literal.contains('/').then(|| literal.to_owned())
}

/// Every `Severity::X, "some/rule"` the engine can raise, plus the parse-gate rules, which
/// are fatal by construction: `parse` returned `Err`, so there is no document to grade.
fn rules_in_code() -> Rules {
    let mut found: Rules = BTreeMap::new();

    let validate = production_source("crates/onyx-core/src/validate.rs");
    let mut at = 0;
    while let Some(offset) = validate[at..].find("Severity::") {
        let start = at + offset + "Severity::".len();
        let name: String = validate[start..]
            .chars()
            .take_while(char::is_ascii_alphabetic)
            .collect();

        if let Some(rule) = literal_after(&validate, start, 120) {
            found.entry(rule).or_default().insert(name.to_lowercase());
        }
        at = start;
    }

    let error = production_source("crates/onyx-core/src/error.rs");
    let gate = error
        .find("fn rule(")
        .map(|start| &error[start..])
        .expect("error.rs still defines rule()");
    let gate = &gate[..gate.find("\n    }").unwrap_or(gate.len())];

    let mut at = 0;
    while let Some(offset) = gate[at..].find('"') {
        let open = at + offset;
        let Some(close) = gate[open + 1..].find('"').map(|i| open + 1 + i) else {
            break;
        };
        let literal = &gate[open + 1..close];
        if literal.contains('/') {
            found
                .entry(literal.to_owned())
                .or_default()
                .insert("error".to_owned());
        }
        at = close + 1;
    }

    found
}

/// The `| \`rule\` | severity | ... |` rows of the rules table. A cell may name more than one
/// severity, as `time/day-mismatch` legitimately does.
fn rules_in_docs() -> Rules {
    let mut found: Rules = BTreeMap::new();

    for line in read("docs/conformance.md").lines() {
        let line = line.trim();
        if !line.starts_with("| `") {
            continue;
        }
        let mut cells = line.split('|').map(str::trim).skip(1);
        let (Some(rule), Some(severity)) = (cells.next(), cells.next()) else {
            continue;
        };
        let rule = rule.trim_matches('`');
        if !rule.contains('/') {
            continue;
        }
        found.insert(
            rule.to_owned(),
            severity
                .split('/')
                .map(|part| part.trim().to_lowercase())
                .collect(),
        );
    }

    found
}

#[test]
fn the_rules_table_matches_the_engine() {
    let code = rules_in_code();
    let docs = rules_in_docs();

    // A scanner that quietly matches nothing would agree with any table at all.
    assert!(
        code.len() >= 12,
        "scanned only {} rules out of the engine; the scanner has drifted from the source",
        code.len()
    );
    assert!(
        docs.len() >= 12,
        "parsed only {} rules out of the table; the document's shape has changed",
        docs.len()
    );

    let mut problems = Vec::new();

    for (rule, severities) in &code {
        match docs.get(rule) {
            None => problems.push(format!(
                "{rule} is raised by the engine but appears in no row of the table"
            )),
            Some(documented) if documented != severities => problems.push(format!(
                "{rule}: the table says {}, the engine raises {}",
                documented.iter().cloned().collect::<Vec<_>>().join("/"),
                severities.iter().cloned().collect::<Vec<_>>().join("/"),
            )),
            Some(_) => {}
        }
    }

    for rule in docs.keys() {
        if !code.contains_key(rule) {
            problems.push(format!(
                "{rule} is documented but the engine never raises it"
            ));
        }
    }

    assert!(
        problems.is_empty(),
        "docs/conformance.md and the engine disagree about {} rule(s):\n\n  {}\n\n\
         Whichever is wrong, they cannot both stand: the table is what a second \
         implementation is held to.\n",
        problems.len(),
        problems.join("\n  "),
    );
}

/// The corpus names rules in `expect.rule`, and a case naming one the engine cannot raise
/// would fail for a reason nobody could act on.
#[test]
fn every_rule_the_corpus_expects_is_a_rule_the_engine_raises() {
    let manifest = read("corpus/manifest.json");
    let code = rules_in_code();

    let mut expected = BTreeSet::new();
    let mut at = 0;
    while let Some(offset) = manifest[at..].find("\"rule\"") {
        let start = at + offset + "\"rule\"".len();
        if let Some(rule) = literal_after(&manifest, start, 40) {
            expected.insert(rule);
        }
        at = start;
    }

    assert!(
        !expected.is_empty(),
        "no expect.rule found in the manifest; this test is not reading it"
    );

    let unknown: Vec<&String> = expected.iter().filter(|r| !code.contains_key(*r)).collect();
    assert!(
        unknown.is_empty(),
        "the corpus expects rules the engine cannot raise: {unknown:?}"
    );
}

/// Every text file in the repository, minus build output and the specification itself.
fn collect_files(dir: &Path, found: &mut Vec<PathBuf>) {
    const SKIP: &[&str] = &[
        "target",
        "node_modules",
        ".git",
        "spec",
        "pkg",
        "__pycache__",
    ];
    const KEEP: &[&str] = &["rs", "md", "json", "py", "mjs", "yml", "toml"];

    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();

        if path.is_dir() {
            if !SKIP.contains(&name.as_ref()) {
                collect_files(&path, found);
            }
        } else if path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| KEEP.contains(&extension))
        {
            found.push(path);
        }
    }
}

/// Every specification section cited anywhere must exist in `spec/v1/SPEC.md`.
///
/// The sections were renumbered once — `quantity` became §3.4 and everything after it
/// shifted — and the citations were not. `confidence` went on pointing at §3.4 long after
/// §3.4 stopped being the section that defines it. This cannot tell a citation that is
/// merely *wrong* from a right one, but it catches every citation left dangling by a
/// renumber, which is the way they actually rot.
#[test]
fn every_cited_specification_section_exists() {
    let spec = read("spec/v1/SPEC.md");

    // Two things are citable. Most sections are headings — `### 3.5 \`source\`` is §3.5.
    // Section 2 is different: its subsections are the numbered principles of an ordered
    // list, so `§2.6` is the sixth item under `## 2. Design principles` and appears in no
    // heading at all.
    let mut headings = BTreeSet::new();
    let mut section = String::new();

    for line in spec.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix('#') {
            let title = rest.trim_start_matches('#').trim();
            let number: String = title
                .chars()
                .take_while(|c| c.is_ascii_digit() || *c == '.')
                .collect();
            let number = number.trim_end_matches('.').to_owned();
            if !number.is_empty() {
                headings.insert(number.clone());
                // Only a top-level `## N.` opens a list-numbered section.
                section = if number.contains('.') {
                    String::new()
                } else {
                    number
                };
            }
            continue;
        }

        if section.is_empty() {
            continue;
        }
        let item: String = trimmed.chars().take_while(char::is_ascii_digit).collect();
        if !item.is_empty() && trimmed[item.len()..].starts_with(". ") {
            headings.insert(format!("{section}.{item}"));
        }
    }
    assert!(
        headings.len() >= 10,
        "only {} headings parsed out of SPEC.md; the scanner has drifted",
        headings.len()
    );

    // Walked, not listed. This was a hardcoded set of six files, which meant a citation
    // anywhere else — the wasm crate, the Python reader, architecture.md — was never looked
    // at. A guard that covers some of the places a mistake can happen mostly teaches you to
    // trust it.
    let mut cited: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut files = Vec::new();
    collect_files(&root(), &mut files);
    assert!(
        files.len() >= 20,
        "only {} files walked; the walk is not reaching the repository",
        files.len()
    );

    for path in &files {
        let Ok(text) = std::fs::read_to_string(path) else {
            continue;
        };
        let shown = path
            .strip_prefix(root())
            .unwrap_or(path)
            .display()
            .to_string()
            .replace(char::from(92), "/");

        // Both spellings. `§3.4` is the common one, and prose writes "Section 3.4" —
        // six live instances of which were outside this guard entirely, so the form a
        // human reaches for when writing a sentence was the form nothing checked.
        for marker in ["§", "Section "] {
            let mut at = 0;
            while let Some(offset) = text[at..].find(marker) {
                let start = at + offset + marker.len();
                let number: String = text[start..]
                    .chars()
                    .take_while(|c| c.is_ascii_digit() || *c == '.')
                    .collect();
                let number = number.trim_end_matches('.');
                if !number.is_empty() {
                    cited
                        .entry(number.to_owned())
                        .or_default()
                        .insert(shown.clone());
                }
                at = start;
            }
        }
    }

    assert!(
        !cited.is_empty(),
        "no § citations found at all; this test is not reading the files"
    );

    let dangling: Vec<String> = cited
        .iter()
        .filter(|(number, _)| !headings.contains(*number))
        .map(|(number, wheres)| {
            format!(
                "§{number} is cited in {} but is not a section of SPEC.md",
                wheres.iter().cloned().collect::<Vec<_>>().join(", ")
            )
        })
        .collect();

    assert!(
        dangling.is_empty(),
        "{} dangling citation(s):

  {}
",
        dangling.len(),
        dangling.join(
            "
  "
        ),
    );
}
