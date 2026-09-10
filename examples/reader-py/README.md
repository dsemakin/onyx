# Minimal reader (Python)

A small Onyx reader, written to be read rather than deployed.

```sh
python examples/reader-py/run_corpus.py
```

```
12 corpus cases passed against the Python reader
```

## Why it exists

Two claims this project makes are otherwise only assertions:

1. **The specification is implementable from `spec/` and `corpus/` alone.** This file was
   written from those and from nothing else — it shares no code with the Rust engine and
   does not import it.
2. **The conformance corpus is passable by an independent implementation.** It runs all
   four groups: acceptance, rejection with the right rule, round-trip preservation, and
   restoring the diary from the portable layer.

If this example ever needs a rule that is not written down in the specification, that is a
bug in the specification rather than in this file.

## What it shows

**Preservation costs nothing in a dynamic language.** `parse` hands back the dictionary
`json.loads` produced and never rebuilds it from known fields, so every member from a later
minor version and every vendor block is still there when `serialise` writes it out. The
discipline is a single rule: read out of the document, never reconstruct it.

**No time zone database is needed.** An RFC 3339 timestamp shows local time beside its
offset, so the date it displays *is* the local date. `local_date` is three lines. That is
exactly why §2.6 insists on an offset rather than accepting a UTC instant.

**Conformance is a small target.** About 180 lines of standard library covers the identity
gate, the rules the corpus asserts, and the canonical restoration. A consumer is not
obliged to implement everything the Rust engine does.

## What it deliberately does not do

Only the validation rules the corpus asserts — `unit/dimension`, `day/duplicate-date`,
`time/utc-normalised`, `document/missing-timezone` and `nutrients/atwater`. The engine
carries more. Keeping this short is the point; a second implementer should be able to read
the whole thing in one sitting and know what conformance costs.

MIT OR Apache-2.0, like the rest of the code. Copy it into your own project; that is what
it is for.
