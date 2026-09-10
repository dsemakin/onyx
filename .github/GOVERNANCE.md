# Governance

## Current model

This project is maintained by a single person, Dan Semakin, who has final say on
specification and engine changes. That is stated plainly rather than dressed up,
because a format's governance is something an implementer is entitled to evaluate
before adopting it.

This is explicitly a starting position, not the intended end state. Section 7.3 of the
specification commits to the opposite:

> Move governance off a vendor domain. If a second implementer adopts it, the `$id`
> should move to a neutral host and changes should need more than one party's approval
> — the GTFS lesson again.

**When a second independent implementer ships**, this project moves to a neutral
organisation and specification changes will require agreement from more than one party.
The single most consequential decision in GTFS's history was renaming it from *Google*
Transit Feed Specification to *General*; the second was letting people other than Google
decide what went into it. Both lessons are taken here.

## Specification changes

The format is versioned with semver, independently of the engine crates.

| | Meaning | Consumer obligation |
|---|---|---|
| MAJOR | Meanings changed | MUST refuse a major it does not know |
| MINOR | Members added | MUST accept any minor of a major it knows, ignoring unknown members |
| PATCH | Editorial only | No change required |

The must-ignore rule is what makes this work: because unknown members are ignored,
adding a field usually requires no change at the consumer at all.

Process:

1. Open an issue using the **Specification change** template. State the problem the
   change solves and which existing documents it would affect.
2. Discussion happens in the issue. Anything that changes the meaning of an existing
   member needs a migration story before it can be accepted.
3. The change lands as a new version directory under `spec/`, never as an edit to a
   published one. `spec/v1/` matches documents already in the wild byte for byte and
   stays that way.
4. Mechanical transformations are added to `migrations/` as declarative manifests, so
   implementations in any language can apply them without linking this engine.
5. New conformance cases land in `corpus/` in the same pull request. A rule nobody can
   test is not a rule.

## Engine changes

The crates are versioned separately from the specification. A patch release of
`onyx-core` never implies anything about the format, and a new spec version does not
force a major bump of the engine.

The CLI's `--json` output and the C ABI are public interfaces. Additive changes only;
anything else bumps the documented `reportVersion` or the ABI version.

## Becoming a second implementer

This is the contribution the project wants most. If you have built a reader or writer
in any language:

1. Run the conformance corpus against it — see `docs/conformance.md`.
2. Open a pull request adding it to the implementations list in the README.

An implementation that passes the corpus with no vendor block present is the thing that
turns this from one app's export format into a format. A consumer that only reads its
own files has implemented a backup.
