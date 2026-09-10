//! Moving a document between specification versions.
//!
//! Most of what people expect from a migration engine is already free. Under the
//! must-ignore rule a 1.0 document *is* a valid 1.7 document, and a 1.7 document read by a
//! 1.0 consumer simply loses the members it does not know. Upgrading across a minor is a
//! no-op, and reading across one needs nothing at all.
//!
//! What is not free is rewriting a document *down* to an older version without losing what
//! the newer one carried. That is the job this module exists for, and it is the only
//! reason a minor version needs migration machinery.
//!
//! # The demoted namespace
//!
//! Members that have no home in the target version are parked under a reserved extension
//! namespace, keyed by the concrete JSON Pointer each came from, rather than dropped.
//! Upgrading again puts them back. That is what makes a downgrade followed by an upgrade
//! lossless, and it has to be part of the specification rather than one engine's
//! behaviour — otherwise two engines disagree about where the data went.
//!
//! # Migrations are data
//!
//! The manifests live in `migrations/` as JSON and are embedded at compile time, so an
//! implementation in any other language can apply the same transformations by reading the
//! same files, without linking this engine. A format whose only migration path is one
//! binary is a format with one implementation.

use crate::document::{Document, semver};
use crate::error::{Error, Result};
use crate::json::{Object, Value};

/// Reserved namespace holding members a downgrade could not keep.
///
/// An upgrade **must** re-promote and clear these. A consumer that does not understand
/// them must, as ever, leave them alone.
pub const DEMOTED_NAMESPACE: &str = "io.github.dsemakin.onyx.demoted";

/// One version-to-version transformation, as declared in `migrations/`.
#[derive(Debug, Clone)]
struct Manifest {
    from: String,
    to: String,
    /// What kind of change this is: `clarification`, `additive`, or — for a future MAJOR —
    /// something that alters what existing members mean.
    kind: String,
    /// Members the newer version added, as JSON Pointers where `*` matches every index of
    /// an array.
    added_members: Vec<String>,
}

impl Manifest {
    /// Whether a downgrade has to relocate members rather than leave them where they are.
    ///
    /// Almost never. No object in this format's schema is closed, so a member a newer
    /// minor added is *already legal* in an older one — an older reader simply ignores it,
    /// and the engine preserves it in place. Moving such a member into a parking block
    /// would change the document's shape to solve a problem that does not exist.
    ///
    /// It becomes necessary only across a MAJOR, where a member may no longer mean what it
    /// used to and leaving it in place would be worse than parking it.
    fn relocates_on_downgrade(&self) -> bool {
        !matches!(self.kind.as_str(), "clarification" | "additive")
    }
}

impl Manifest {
    /// Read by hand rather than derived, because this crate has no dependencies.
    ///
    /// This used to panic, on the reasoning that manifests are ours and embedded at compile
    /// time. That stopped being true when [`migrate_with`] was made public: its manifests
    /// come from the caller, and a library that aborts the host process over a malformed
    /// argument is not one you can safely hand a file to.
    fn read(source: &str) -> Result<Self> {
        let value = crate::json::parse(source).map_err(Error::Json)?;
        let object = value
            .as_object()
            .ok_or_else(|| Error::MalformedManifest("it is not a JSON object".to_owned()))?;

        let text = |key: &str| -> Result<String> {
            object
                .get(key)
                .and_then(Value::as_str)
                .map(str::to_owned)
                .ok_or_else(|| Error::MalformedManifest(format!("it has no string `{key}`")))
        };

        Ok(Self {
            from: text("from")?,
            to: text("to")?,
            kind: text("kind")?,
            added_members: object
                .get("addedMembers")
                .and_then(Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .filter_map(|item| item.as_str().map(str::to_owned))
                        .collect()
                })
                .unwrap_or_default(),
        })
    }
}

/// The manifests this build ships, embedded at compile time so migration needs no
/// filesystem while the manifests stay readable data another implementation can use.
///
/// Empty, because one version of the specification exists and there is nothing to migrate
/// between. Add a manifest to `migrations/`, vendor it, and list it here when a second
/// version lands. Shipping a manifest for a version that does not exist would describe a
/// document nobody can write.
const EMBEDDED: &[&str] = &[];

/// Rewrites a document as the target specification version.
///
/// Upgrading across a minor changes nothing but `specVersion`, because minors only add
/// optional members. Downgrading parks anything the target has no home for under
/// [`DEMOTED_NAMESPACE`], so the reverse trip restores it exactly.
pub fn migrate(document: &Document, target: &str) -> Result<Document> {
    migrate_with(document, target, EMBEDDED)
}

/// Rewrites a document using a caller-supplied set of manifests, each the JSON text of one.
///
/// [`migrate`] is this with the manifests this build ships. Taking them as an argument is
/// what lets a caller migrate across versions this build predates, and it is the hook a
/// converter for a foreign format will use.
pub fn migrate_with(document: &Document, target: &str, sources: &[&str]) -> Result<Document> {
    let target_version =
        semver(target).ok_or_else(|| Error::MalformedVersion(target.to_owned()))?;
    let mut current = document.spec_version.clone();
    let mut current_version =
        semver(&current).ok_or_else(|| Error::MalformedVersion(current.clone()))?;

    let manifests: Vec<Manifest> = sources
        .iter()
        .map(|source| Manifest::read(source))
        .collect::<Result<_>>()?;
    let mut value = document.to_json();

    // A malformed manifest chain could otherwise loop. The bound is far above any real
    // number of versions and exists only so a bad manifest fails loudly.
    for _ in 0..64 {
        if current_version == target_version {
            if let Some(object) = value.as_object_mut() {
                object.insert("specVersion", Value::String(current.clone()));
            }
            let object = match value {
                Value::Object(object) => object,
                _ => return Err(Error::MalformedVersion(current)),
            };
            return Ok(Document::from_object(object));
        }

        let step = if target_version > current_version {
            let manifest = manifests
                .iter()
                .find(|manifest| manifest.from == current)
                .ok_or_else(|| Error::NoMigrationPath {
                    from: current.clone(),
                    to: target.to_owned(),
                })?;
            if !restoring_is_possible(&value) {
                return Err(Error::CannotPromote {
                    from: current.clone(),
                    to: manifest.to.clone(),
                    reason: "the demoted block under `extensions` is not an object".to_owned(),
                });
            }
            let stuck = promote(&mut value, manifest);
            if !stuck.is_empty() {
                return Err(Error::CannotPromote {
                    from: current.clone(),
                    to: manifest.to.clone(),
                    reason: format!(
                        "{} parked member(s) could not be restored and remain parked: {}",
                        stuck.len(),
                        stuck.join(", ")
                    ),
                });
            }
            manifest.to.clone()
        } else {
            let manifest = manifests
                .iter()
                .find(|manifest| manifest.to == current)
                .ok_or_else(|| Error::NoMigrationPath {
                    from: current.clone(),
                    to: target.to_owned(),
                })?;
            if manifest.relocates_on_downgrade() {
                demote(&mut value, manifest)?;
            }
            manifest.from.clone()
        };

        current = step;
        current_version =
            semver(&current).ok_or_else(|| Error::MalformedVersion(current.clone()))?;
    }

    Err(Error::NoMigrationPath {
        from: document.spec_version.clone(),
        to: target.to_owned(),
    })
}

fn refusal(manifest: &Manifest, reason: &str) -> Error {
    Error::CannotDemote {
        from: manifest.to.clone(),
        to: manifest.from.clone(),
        reason: reason.to_owned(),
    }
}

/// Whether the pointer's final segment names a position in an array rather than a member
/// of an object.
fn addresses_an_array_element(value: &Value, pointer: &str) -> bool {
    let segments = segments(pointer);
    let Some((_, parents)) = segments.split_last() else {
        return false;
    };
    let mut cursor = value;
    for segment in parents {
        cursor = match child(cursor, segment) {
            Some(child) => child,
            None => return false,
        };
    }
    matches!(cursor, Value::Array(_))
}

/// True when the demoted block can be reached or created.
///
/// Checked *before* anything is removed. `demote` used to remove members first and discover
/// only afterwards that `extensions` was not an object — and its early return then dropped
/// the parked copy, which by that point was the only one.
fn parking_is_possible(value: &Value) -> bool {
    fn object_or_absent(parent: &Object, key: &str) -> Option<bool> {
        match parent.get(key) {
            None => Some(true),
            Some(Value::Object(_)) => None,
            Some(_) => Some(false),
        }
    }

    let Some(document) = value.as_object() else {
        return false;
    };
    if let Some(answer) = object_or_absent(document, "extensions") {
        return answer;
    }
    let Some(extensions) = document.get("extensions").and_then(Value::as_object) else {
        return true;
    };
    if let Some(answer) = object_or_absent(extensions, DEMOTED_NAMESPACE) {
        return answer;
    }
    let Some(block) = extensions.get(DEMOTED_NAMESPACE).and_then(Value::as_object) else {
        return true;
    };
    object_or_absent(block, "members").unwrap_or(true)
}

/// The demoted block, when the document has one and it is an object.
fn demoted_block(value: &Value) -> Option<&Object> {
    value.get("extensions")?.get(DEMOTED_NAMESPACE)?.as_object()
}

/// Parks every member the older version has no home for, rather than dropping it.
///
/// Returns false when the document's `extensions` are not shaped to hold the parked
/// members, in which case nothing is removed and the caller reports it. Losing the members
/// instead is the one outcome this must never have.
fn demote(value: &mut Value, manifest: &Manifest) -> Result<()> {
    if !parking_is_possible(value) {
        return Err(refusal(
            manifest,
            "members must be parked under `extensions`, which is not an object here",
        ));
    }

    // An `addedMembers` pattern whose last segment addresses an array element cannot be
    // parked: `remove_at` only removes object members, and removing an array element would
    // renumber every pointer after it. Doing nothing quietly is the wrong answer — the
    // downgrade would silently leave members the older version does not define.
    for pattern in &manifest.added_members {
        for pointer in expand(value, pattern) {
            if addresses_an_array_element(value, &pointer) {
                return Err(refusal(
                    manifest,
                    &format!(
                        "`{pointer}` addresses an array element; only object members can be parked"
                    ),
                ));
            }
        }
    }

    // The block's `fromVersion` is about to be compared with this manifest's, and the
    // comparison has no answer for something that is not a version. Quietly replacing it
    // understated where the parked members came from — the one thing the member is for —
    // so it is refused like every other malformed shape of the block, before anything moves.
    if let Some(existing) = demoted_block(value).and_then(|block| block.get("fromVersion")) {
        let readable = existing.as_str().is_some_and(|text| semver(text).is_some());
        if !readable {
            return Err(refusal(
                manifest,
                &format!("the demoted block's `fromVersion` is not semver-shaped: {existing}"),
            ));
        }
    }

    let mut parked = Object::new();
    for pattern in &manifest.added_members {
        for pointer in expand(value, pattern) {
            if let Some(removed) = remove_at(value, &pointer) {
                parked.insert(pointer, removed);
            }
        }
    }

    if parked.is_empty() {
        return Ok(());
    }

    let document = value.as_object_mut().expect("parking_is_possible checked");
    if !document.contains_key("extensions") {
        document.insert("extensions", Value::Object(Object::new()));
    }
    let extensions = document
        .get_mut("extensions")
        .and_then(Value::as_object_mut)
        .expect("parking_is_possible checked");
    if !extensions.contains_key(DEMOTED_NAMESPACE) {
        extensions.insert(DEMOTED_NAMESPACE, Value::Object(Object::new()));
    }
    let block = extensions
        .get_mut(DEMOTED_NAMESPACE)
        .and_then(Value::as_object_mut)
        .expect("parking_is_possible checked");

    // The version things were demoted *from*, so an upgrade knows what it is restoring.
    //
    // The highest wins. A chained downgrade — 1.2.0 to 1.1.0 to 1.0.0 — calls this once per
    // step, and blindly overwriting left the block claiming everything came from 1.1.0
    // while it also held members that only ever existed in 1.2.0. The block is one place;
    // it has to name the top of the range it covers. An existing value that is not a
    // version was refused above, so the fallback only remains for a malformed manifest.
    let highest = match block.get("fromVersion").and_then(Value::as_str) {
        Some(existing) => match (semver(existing), semver(&manifest.to)) {
            (Some(there), Some(here)) if there >= here => existing.to_owned(),
            _ => manifest.to.clone(),
        },
        None => manifest.to.clone(),
    };
    block.insert("fromVersion", Value::String(highest));
    if !block.contains_key("members") {
        block.insert("members", Value::Object(Object::new()));
    }
    let members = block
        .get_mut("members")
        .and_then(Value::as_object_mut)
        .expect("parking_is_possible checked");
    members.extend(parked);
    Ok(())
}

/// Puts back anything an earlier downgrade parked for this version.
///
/// Upgrading is otherwise a no-op: every member a minor adds is optional, so a document
/// written against the older version is already valid against the newer one.
/// Restores what a downgrade parked.
///
/// Returns the pointers it could not put back. They stay parked — no data is lost — but a
/// caller is told, because `demote` refuses loudly for the same shape and an upgrade that
/// quietly half-worked is the asymmetry that hides it.
/// Whether the demoted block, if there is one at all, is shaped to be read from.
///
/// Absent is fine — there is simply nothing parked. Present and malformed is not: it is the
/// mirror of the case `demote` refuses, and returning "nothing to restore" for it meant an
/// upgrade reported success while a document that plainly held parked members came back
/// without them.
fn restoring_is_possible(value: &Value) -> bool {
    let Some(document) = value.as_object() else {
        return false;
    };
    let Some(extensions) = document.get("extensions") else {
        return true;
    };
    let Some(extensions) = extensions.as_object() else {
        return false;
    };
    let Some(block) = extensions.get(DEMOTED_NAMESPACE) else {
        return true;
    };
    let Some(block) = block.as_object() else {
        return false;
    };
    match block.get("members") {
        None => true,
        Some(members) => members.as_object().is_some(),
    }
}

fn promote(value: &mut Value, manifest: &Manifest) -> Vec<String> {
    let restorable: Vec<(String, Value)> = match value
        .get("extensions")
        .and_then(|extensions| extensions.get(DEMOTED_NAMESPACE))
        .and_then(|block| block.get("members"))
        .and_then(Value::as_object)
    {
        Some(members) => members
            .iter()
            .filter(|(pointer, _)| {
                manifest
                    .added_members
                    .iter()
                    .any(|pattern| pattern_matches(pattern, pointer))
            })
            .map(|(pointer, value)| (pointer.to_owned(), value.clone()))
            .collect(),
        None => return Vec::new(),
    };

    if restorable.is_empty() {
        return Vec::new();
    }

    // Only what actually landed is forgotten. Discarding this result and clearing the block
    // regardless meant a failed insert deleted the parked copy too — the member was gone
    // from both places at once.
    let mut restored_pointers = Vec::new();
    let mut stuck = Vec::new();
    for (pointer, restored) in &restorable {
        match insert_at(value, pointer, restored.clone()) {
            Ok(()) => restored_pointers.push(pointer.clone()),
            Err(why) => stuck.push(format!("{pointer} ({why})")),
        }
    }

    let mut block_is_empty = false;
    if let Some(members) = value
        .get_mut("extensions")
        .and_then(|extensions| extensions.get_mut(DEMOTED_NAMESPACE))
        .and_then(|block| block.get_mut("members"))
        .and_then(Value::as_object_mut)
    {
        for pointer in &restored_pointers {
            members.remove(pointer);
        }
        block_is_empty = members.is_empty();
    }

    // Leave no empty scaffolding behind: a document that has been down and back should
    // look like one that never moved.
    if block_is_empty {
        if let Some(extensions) = value.get_mut("extensions").and_then(Value::as_object_mut) {
            extensions.remove(DEMOTED_NAMESPACE);
            if extensions.is_empty() {
                if let Some(document) = value.as_object_mut() {
                    document.remove("extensions");
                }
            }
        }
    }

    stuck
}

// ── JSON Pointer helpers ─────────────────────────────────────────────────────
//
// RFC 6901 pointers, with one addition: `*` in a pattern matches every index of an array.
// Member names in this format contain neither `/` nor `~`, so the escape sequences the
// RFC defines are not implemented; a future version that allows them would need them.

fn segments(pointer: &str) -> Vec<&str> {
    pointer.split('/').skip(1).collect()
}

/// Whether a concrete pointer is one of the places a pattern describes.
fn pattern_matches(pattern: &str, pointer: &str) -> bool {
    let pattern = segments(pattern);
    let pointer = segments(pointer);
    pattern.len() == pointer.len()
        && pattern
            .iter()
            .zip(pointer.iter())
            .all(|(wanted, found)| *wanted == "*" || wanted == found)
}

/// Every concrete pointer in this document that the pattern describes **and that exists**.
fn expand(value: &Value, pattern: &str) -> Vec<String> {
    let mut found = Vec::new();
    descend(value, &segments(pattern), String::new(), &mut found);
    found
}

fn descend(value: &Value, remaining: &[&str], prefix: String, found: &mut Vec<String>) {
    let Some((head, tail)) = remaining.split_first() else {
        found.push(prefix);
        return;
    };

    if *head == "*" {
        if let Some(items) = value.as_array() {
            for (index, item) in items.iter().enumerate() {
                descend(item, tail, format!("{prefix}/{index}"), found);
            }
        }
        return;
    }

    if let Some(child) = child(value, head) {
        descend(child, tail, format!("{prefix}/{head}"), found);
    }
}

fn child<'a>(value: &'a Value, segment: &str) -> Option<&'a Value> {
    match value {
        Value::Object(members) => members.get(segment),
        Value::Array(items) => {
            crate::json::pointer_index(segment).and_then(|index| items.get(index))
        }
        _ => None,
    }
}

fn child_mut<'a>(value: &'a mut Value, segment: &str) -> Option<&'a mut Value> {
    match value {
        Value::Object(members) => members.get_mut(segment),
        Value::Array(items) => items.get_mut(crate::json::pointer_index(segment)?),
        _ => None,
    }
}

fn remove_at(value: &mut Value, pointer: &str) -> Option<Value> {
    let segments = segments(pointer);
    let (last, parents) = segments.split_last()?;

    let mut cursor = value;
    for segment in parents {
        cursor = child_mut(cursor, segment)?;
    }
    cursor.as_object_mut()?.remove(last)
}

/// Restores a member, refusing to displace one that is already there.
///
/// A downgrade removes leaves and never the objects holding them, so the parent normally
/// still exists — and the member normally does not. But a document can be edited while it
/// sits at the older version, and if something wrote a live value at the same pointer,
/// overwriting it with the parked copy would destroy data the caller can see in favour of
/// data it may have forgotten. The parked copy stays parked, and the caller is told why.
fn insert_at(
    value: &mut Value,
    pointer: &str,
    restored: Value,
) -> std::result::Result<(), &'static str> {
    let segments = segments(pointer);
    let Some((last, parents)) = segments.split_last() else {
        return Err("the pointer names nothing");
    };

    let mut cursor = value;
    for segment in parents {
        cursor = child_mut(cursor, segment).ok_or("its parent could not be reached")?;
    }

    match cursor {
        Value::Object(members) if members.contains_key(last) => {
            Err("a member already exists there")
        }
        Value::Object(members) => {
            members.insert((*last).to_owned(), restored);
            Ok(())
        }
        _ => Err("its parent is not an object"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The manifest under test, defined here rather than shipped.
    ///
    /// `migrations/` is empty because one version of the specification exists. The machinery
    /// still has to be exercised, so the tests bring their own — which is exactly the shape a
    /// caller supplies through [`migrate_with`].
    const ADDITIVE: &str = r#"{
      "manifestVersion": 1,
      "from": "1.0.0",
      "to": "1.1.0",
      "kind": "additive",
      "addedMembers": [
        "/coverage",
        "/subject/birthDate",
        "/days/*/entries/*/nutrients/fibre",
        "/days/*/entries/*/nutrients/sodium"
      ]
    }"#;

    /// A document using members a hypothetical 1.1.0 adds: a coverage window, a birth
    /// date, and two micronutrients the four-nutrient v1.0 model has no field for.
    const NEWER: &str = r#"{
      "format": "onyx",
      "specVersion": "1.1.0",
      "exportedAt": "2026-08-14T12:00:00+03:00",
      "timeZone": "Europe/Berlin",
      "producer": { "name": "Later Minor" },
      "coverage": { "from": "2026-08-01", "to": "2026-08-14" },
      "subject": { "sex": "female", "birthDate": "1992-03-04" },
      "days": [
        {
          "date": "2026-08-10",
          "entries": [
            {
              "loggedAt": "2026-08-10T08:30:00+03:00",
              "name": "Oats",
              "nutrients": {
                "energy": { "value": 389, "unit": "kcal" },
                "fibre": { "value": 10.6, "unit": "g" },
                "sodium": { "value": 6, "unit": "mg" }
              }
            }
          ]
        }
      ]
    }"#;

    #[test]
    fn upgrading_a_minor_adds_nothing() {
        let document = crate::parse(
            r#"{
              "format": "onyx",
              "specVersion": "1.0.0",
              "exportedAt": "2026-08-14T12:00:00+03:00",
              "producer": { "name": "Test" }
            }"#,
        )
        .unwrap();

        // Every member 1.1.0 adds is optional, so an older document is already valid
        // against it. There is nothing to do but say so.
        let migrated = migrate_with(&document, "1.1.0", &[ADDITIVE]).unwrap();
        assert_eq!(migrated.spec_version, "1.1.0");
        assert!(migrated.extensions.is_none());
    }

    #[test]
    fn downgrading_a_minor_leaves_members_where_they_are() {
        let document = crate::parse(NEWER).unwrap();
        let older = migrate_with(&document, "1.0.0", &[ADDITIVE]).unwrap();

        assert_eq!(older.spec_version, "1.0.0");

        // No object in this format's schema is closed, so a member a newer minor added is
        // already legal in an older one. Relocating it into a parking block would change
        // the document's shape to solve a problem that does not exist — the engine
        // preserves it in place and an older reader simply ignores it.
        assert!(older.member("coverage").is_some());
        let nutrients = older.days()[0].entries.as_ref().unwrap()[0]
            .nutrients
            .as_ref()
            .unwrap();
        assert!(nutrients.extra.contains_key("fibre"));
        assert!(nutrients.extra.contains_key("sodium"));
        assert!(
            nutrients.energy.is_some(),
            "energy is a 1.0 member and must stay"
        );

        // And nothing was parked, because nothing needed to be.
        assert!(older.extension(DEMOTED_NAMESPACE).is_none());
    }

    #[test]
    fn down_then_up_is_lossless() {
        // The whole reason this machinery exists. If this does not hold, a downgrade is
        // data loss wearing a migration's clothes.
        let original = crate::parse(NEWER).unwrap();
        let older = migrate_with(&original, "1.0.0", &[ADDITIVE]).unwrap();
        let restored = migrate_with(&older, "1.1.0", &[ADDITIVE]).unwrap();

        assert_eq!(restored, original);
        // And no scaffolding left behind: it should look like it never moved.
        assert!(restored.extension(DEMOTED_NAMESPACE).is_none());
        assert!(restored.extensions.is_none());
    }

    #[test]
    fn a_vendor_block_survives_the_trip_untouched() {
        let text = NEWER.replace(
            r#""producer": { "name": "Later Minor" },"#,
            r#""producer": { "name": "Later Minor" },
               "extensions": { "com.example.tracker": { "blockVersion": 4 } },"#,
        );
        let original = crate::parse(&text).unwrap();

        let older = migrate_with(&original, "1.0.0", &[ADDITIVE]).unwrap();
        assert!(older.extension("com.example.tracker").is_some());

        let restored = migrate_with(&older, "1.1.0", &[ADDITIVE]).unwrap();
        assert_eq!(restored, original);
        assert!(restored.extension("com.example.tracker").is_some());
    }

    #[test]
    fn refuses_a_version_it_has_no_manifest_for() {
        let document = crate::parse(NEWER).unwrap();
        assert!(matches!(
            migrate_with(&document, "9.0.0", &[ADDITIVE]),
            Err(Error::NoMigrationPath { .. })
        ));
        assert!(matches!(
            migrate_with(&document, "not-a-version", &[ADDITIVE]),
            Err(Error::MalformedVersion(_))
        ));
    }

    #[test]
    fn this_build_ships_no_manifests_because_one_version_exists() {
        // If a manifest is ever added to migrations/ and listed in EMBEDDED, this fails and
        // whoever added it updates the claim here and in migrations/README.md.
        assert!(EMBEDDED.is_empty());

        // And migrate() therefore refuses anything but the version already in hand.
        let document = crate::parse(NEWER).unwrap();
        assert!(matches!(
            migrate(&document, "1.0.0"),
            Err(Error::NoMigrationPath { .. })
        ));
    }

    #[test]
    fn patterns_match_the_places_they_describe() {
        assert!(pattern_matches(
            "/days/*/entries/*/nutrients/fibre",
            "/days/0/entries/2/nutrients/fibre"
        ));
        assert!(!pattern_matches(
            "/days/*/entries/*/nutrients/fibre",
            "/days/0/entries/2/nutrients/energy"
        ));
        assert!(!pattern_matches("/coverage", "/coverage/from"));
    }

    /// `migrate_with` is public, so its manifests are untrusted input. A library that
    /// aborts the host process over a bad argument cannot be handed a file safely.
    #[test]
    fn a_rubbish_manifest_is_an_error_and_not_a_panic() {
        let document = crate::parse(NEWER).unwrap();

        for rubbish in [
            "not json at all",
            "[]",
            "{}",
            r#"{"from": "1.0.0"}"#,
            r#"{"from": 1, "to": "1.1.0", "kind": "additive"}"#,
        ] {
            let outcome = migrate_with(&document, "1.1.0", &[rubbish]);
            assert!(outcome.is_err(), "{rubbish:?} was accepted as a manifest");
        }
    }

    /// A two-step downgrade parks members from both versions in one block, so the block
    /// must name the higher of them.
    #[test]
    fn a_chained_downgrade_records_the_highest_version_it_came_from() {
        // `additive` and `clarification` never relocate — an unknown member survives a
        // downgrade on the must-ignore rule alone. Parking only happens for the other kinds,
        // so those are what this test needs.
        const ZERO_TO_ONE: &str = r#"{
          "from": "1.0.0",
          "to": "1.1.0",
          "kind": "breaking",
          "addedMembers": ["/coverage"]
        }"#;
        const ONE_TO_TWO: &str = r#"{
          "from": "1.1.0",
          "to": "1.2.0",
          "kind": "breaking",
          "addedMembers": ["/hydration"]
        }"#;

        let document = crate::parse(
            r#"{
              "format": "onyx",
              "specVersion": "1.2.0",
              "exportedAt": "2026-08-14T12:00:00+03:00",
              "producer": { "name": "Test" },
              "coverage": "everything",
              "hydration": { "value": 2, "unit": "L" }
            }"#,
        )
        .unwrap();

        let older = migrate_with(&document, "1.0.0", &[ZERO_TO_ONE, ONE_TO_TWO]).unwrap();
        let json = older.to_json();
        let block = json
            .get("extensions")
            .and_then(|e| e.get(DEMOTED_NAMESPACE))
            .expect("a demoted block");

        assert_eq!(older.spec_version, "1.0.0");
        assert_eq!(
            block.get("fromVersion").and_then(Value::as_str),
            Some("1.2.0"),
            "the block holds 1.2.0 members, so naming 1.1.0 would understate it"
        );

        // And the whole way back up still restores both.
        let restored = migrate_with(&older, "1.2.0", &[ZERO_TO_ONE, ONE_TO_TWO]).unwrap();
        let back = restored.to_json();
        assert!(back.get("hydration").is_some(), "1.2.0 member lost");
        assert!(back.get("coverage").is_some(), "1.1.0 member lost");
    }
    const RELOCATING: &str = r#"{
      "from": "1.0.0",
      "to": "1.1.0",
      "kind": "breaking",
      "addedMembers": ["/coverage"]
    }"#;

    fn with_extensions(extensions: &str) -> String {
        format!(
            r#"{{
              "format": "onyx",
              "specVersion": "1.1.0",
              "exportedAt": "2026-08-14T12:00:00+03:00",
              "producer": {{ "name": "Test" }},
              "coverage": "everything",
              "extensions": {extensions}
            }}"#
        )
    }

    /// The parked copy is the only copy once the member has been removed. If there is
    /// nowhere to put it, nothing may be removed.
    #[test]
    fn a_downgrade_that_cannot_park_refuses_instead_of_losing_the_member() {
        for hostile in ["\"not an object\"", "[]", "42"] {
            let document = crate::parse(&with_extensions(hostile)).unwrap();
            let outcome = migrate_with(&document, "1.0.0", &[RELOCATING]);

            assert!(
                matches!(outcome, Err(Error::CannotDemote { .. })),
                "extensions {hostile} should refuse, got {outcome:?}"
            );
            // And the document it was handed still has the member.
            assert!(
                document.to_json().get("coverage").is_some(),
                "extensions {hostile}: the member was taken anyway"
            );
        }
    }

    /// A demoted block whose `members` is not an object is the same trap one level deeper.
    #[test]
    fn a_malformed_demoted_block_also_refuses() {
        let extensions = format!(r#"{{ "{DEMOTED_NAMESPACE}": {{ "members": 7 }} }}"#);
        let document = crate::parse(&with_extensions(&extensions)).unwrap();
        assert!(matches!(
            migrate_with(&document, "1.0.0", &[RELOCATING]),
            Err(Error::CannotDemote { .. })
        ));
    }

    /// An upgrade that cannot put a member back must keep it parked. Clearing the block
    /// regardless deleted the only remaining copy.
    #[test]
    fn a_restore_that_fails_leaves_the_member_parked() {
        // `/days/0/coverage` cannot be restored: there is no `days` array to put it in.
        let parked = format!(
            r#"{{ "{DEMOTED_NAMESPACE}": {{
                 "fromVersion": "1.1.0",
                 "members": {{ "/days/0/coverage": "everything" }}
               }} }}"#
        );
        let text = format!(
            r#"{{
              "format": "onyx",
              "specVersion": "1.0.0",
              "exportedAt": "2026-08-14T12:00:00+03:00",
              "producer": {{ "name": "Test" }},
              "extensions": {parked}
            }}"#
        );
        const DEEP: &str = r#"{
          "from": "1.0.0",
          "to": "1.1.0",
          "kind": "breaking",
          "addedMembers": ["/days/*/coverage"]
        }"#;

        let document = crate::parse(&text).unwrap();
        let outcome = migrate_with(&document, "1.1.0", &[DEEP]);

        // Loud, not silent: `demote` refuses for the same shape, and an upgrade that
        // quietly half-worked is what hides that asymmetry.
        match &outcome {
            Err(Error::CannotPromote { reason, .. }) => {
                assert!(
                    reason.contains("/days/0/coverage"),
                    "the refusal should name what stayed parked: {reason}"
                );
            }
            other => panic!("expected CannotPromote, got {other:?}"),
        }

        // And the member is still parked in the document that was handed in — the failure
        // must not have consumed it.
        let json = document.to_json();
        let still_parked = json
            .get("extensions")
            .and_then(|e| e.get(DEMOTED_NAMESPACE))
            .and_then(|b| b.get("members"))
            .and_then(|m| m.get("/days/0/coverage"));
        assert!(
            still_parked.is_some(),
            "the member could not be restored and was dropped from the block as well"
        );
    }
    /// A manifest that parks whole array elements cannot be honoured — removing one
    /// renumbers every pointer after it. Refusing says so; the previous behaviour was to
    /// park nothing and report success, leaving the document at the older version still
    /// carrying members that version does not define.
    #[test]
    fn a_manifest_that_parks_an_array_element_is_refused_not_ignored() {
        const PARKS_A_DAY: &str = r#"{
          "from": "1.0.0",
          "to": "1.1.0",
          "kind": "breaking",
          "addedMembers": ["/days/*"]
        }"#;

        let document = crate::parse(
            r#"{
              "format": "onyx",
              "specVersion": "1.1.0",
              "exportedAt": "2026-08-14T12:00:00+03:00",
              "producer": { "name": "Test" },
              "days": [{ "date": "2026-08-10" }]
            }"#,
        )
        .unwrap();

        let outcome = migrate_with(&document, "1.0.0", &[PARKS_A_DAY]);
        match outcome {
            Err(Error::CannotDemote { reason, .. }) => {
                assert!(
                    reason.contains("array element"),
                    "the refusal should say why: {reason}"
                );
            }
            other => panic!("expected a refusal, got {other:?}"),
        }
    }
    /// The mirror of `a_downgrade_that_cannot_park_refuses`. An upgrade handed a demoted
    /// block it cannot read used to report success, which reads as "there was nothing
    /// parked" for a document that visibly has some.
    #[test]
    fn an_upgrade_that_cannot_read_the_demoted_block_refuses() {
        const ADDITIVE_BREAKING: &str = r#"{
          "from": "1.0.0",
          "to": "1.1.0",
          "kind": "breaking",
          "addedMembers": ["/coverage"]
        }"#;

        for hostile in [
            format!(r#"{{ "{DEMOTED_NAMESPACE}": 7 }}"#),
            format!(r#"{{ "{DEMOTED_NAMESPACE}": {{ "members": "not an object" }} }}"#),
        ] {
            let text = format!(
                r#"{{
                  "format": "onyx",
                  "specVersion": "1.0.0",
                  "exportedAt": "2026-08-14T12:00:00+03:00",
                  "producer": {{ "name": "Test" }},
                  "extensions": {hostile}
                }}"#
            );
            let document = crate::parse(&text).unwrap();
            let outcome = migrate_with(&document, "1.1.0", &[ADDITIVE_BREAKING]);
            assert!(
                matches!(outcome, Err(Error::CannotPromote { .. })),
                "extensions {hostile} should refuse, got {outcome:?}"
            );
        }
    }

    /// A member written while the document sat at the older version is live data. Restoring
    /// the parked copy over it would trade something the caller can see for something it
    /// may have forgotten; the parked copy stays parked and the refusal says why.
    #[test]
    fn a_restore_refuses_to_overwrite_a_member_written_meanwhile() {
        let original = crate::parse(NEWER).unwrap();
        let older = migrate_with(&original, "1.0.0", &[RELOCATING]).unwrap();
        assert!(
            older.member("coverage").is_none(),
            "the downgrade should have parked it"
        );

        // Something at 1.0.0 writes its own `coverage`.
        let mut edited = older.to_json();
        edited
            .as_object_mut()
            .unwrap()
            .insert("coverage", Value::String("written at 1.0.0".to_owned()));
        let edited = crate::parse(&edited.to_string()).unwrap();

        match migrate_with(&edited, "1.1.0", &[RELOCATING]) {
            Err(Error::CannotPromote { reason, .. }) => assert!(
                reason.contains("/coverage (a member already exists there)"),
                "the refusal should name the member and the cause: {reason}"
            ),
            other => panic!("expected CannotPromote, got {other:?}"),
        }

        // Neither copy was touched: the live one is where it was written, and the parked
        // one is still parked.
        let json = edited.to_json();
        assert_eq!(
            json.get("coverage").and_then(Value::as_str),
            Some("written at 1.0.0")
        );
        assert!(
            json.get("extensions")
                .and_then(|e| e.get(DEMOTED_NAMESPACE))
                .and_then(|b| b.get("members"))
                .and_then(|m| m.get("/coverage"))
                .is_some()
        );
    }

    /// `fromVersion` is compared against the manifest's, and the comparison has no answer for
    /// something that is not a version. Overwriting it understated where the parked members
    /// came from; refusing is what every other malformed shape of the block gets.
    #[test]
    fn a_demoted_block_with_an_unreadable_from_version_is_refused() {
        for hostile in [r#""2.x-beta""#, "7", r#""01.0.0""#] {
            let extensions = format!(
                r#"{{ "{DEMOTED_NAMESPACE}": {{ "fromVersion": {hostile}, "members": {{}} }} }}"#
            );
            let document = crate::parse(&with_extensions(&extensions)).unwrap();
            match migrate_with(&document, "1.0.0", &[RELOCATING]) {
                Err(Error::CannotDemote { reason, .. }) => assert!(
                    reason.contains("fromVersion"),
                    "fromVersion {hostile}: the refusal should say why: {reason}"
                ),
                other => panic!("fromVersion {hostile}: expected a refusal, got {other:?}"),
            }
            // And nothing was taken.
            assert!(document.to_json().get("coverage").is_some());
        }
    }
}
