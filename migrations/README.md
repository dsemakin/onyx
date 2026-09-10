# Migration manifests

One schema for a nutrition log, and a system for moving documents **up and down between its
versions** so that apps on different versions can still exchange data. That system is a
purpose of this project rather than a convenience bolted onto it: a format that only works
when everybody upgrades in lockstep is not a portable format.

The same machinery is where bringing **other formats into this schema** will live. A
converter is a migration whose source happens not to be a version of Onyx.

Manifests are declarative data, not code, so an implementation in any language can apply
them without linking the Rust engine. A format whose only migration path is one binary is a
format with one implementation.

## Reading across a version costs nothing

Under the must-ignore rule a 1.0 document *is* a valid 1.7 document, and a 1.7 document read
by a 1.0 consumer simply loses the members it does not know. No migration step is involved
in either direction. Migration is for **rewriting** a document as another version, not for
reading one.

## Downgrading usually moves nothing

No object in this schema is closed. A member that a newer minor added is therefore already
legal in an older one — an older reader ignores it, and any engine that preserves unknown
members keeps it exactly where it sat. Relocating it would change the document's shape to
solve a problem that does not exist.

So for a `clarification` or an `additive` manifest, a downgrade restamps `specVersion` and
stops. `1.1 -> 1.0 -> 1.1` is lossless because nothing moved.

## The demoted namespace, for when something must move

Across a MAJOR a member may no longer mean what it used to, and leaving it in place is worse
than parking it. Only then does the engine relocate members into a reserved namespace, keyed
by the JSON Pointer each came from:

    "extensions": {
      "io.github.dsemakin.onyx.demoted": {
        "fromVersion": "2.0.0",
        "members": {
          "/days/0/entries/0/nutrients/fibre": { "value": 10.6, "unit": "g" }
        }
      }
    }

An upgrade **must** put them back and clear the block. The manifest's `kind` decides whether
this happens at all: `clarification` and `additive` never trigger it.

## Manifest format

    {
      "manifestVersion": 1,
      "from": "1.0.0",
      "to": "1.1.0",
      "kind": "additive",
      "addedMembers": ["/coverage", "/days/*/entries/*/nutrients/fibre"]
    }

`addedMembers` are RFC 6901 JSON Pointers where `*` matches every index of an array. For an
additive minor that is the whole manifest: the engine derives both directions from it. A
change of *meaning*, which only happens across a MAJOR, would need explicit operations, and
none exist yet.

## What is here

**No manifests.** One version of the specification exists, so there is nothing to migrate
between, and a manifest for a version nobody can write would describe a fiction. A test in
`crates/onyx-core/src/migrate.rs` asserts the shipped set is empty, so this claim cannot
quietly stop being true.

The machinery is built and exercised: the engine's tests bring their own manifest, which is
the same shape a caller supplies through `migrate_with`. When a second version lands, add
the manifest here, run `node scripts/vendor.mjs`, and list it in `EMBEDDED`.

Migrations never convert units. Units are explicit in the format, so converting them is the
job of a converter reading a foreign format, not of a migration between versions of this one.
