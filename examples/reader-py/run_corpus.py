# SPDX-License-Identifier: MIT OR Apache-2.0
"""Run the conformance corpus against the reader in this directory.

This is the whole point of the example: an implementation that shares no code with the
Rust engine, reading the same JSON cases, and agreeing about what they mean.

    python examples/reader-py/run_corpus.py

Standard library only. Exits 0 when every case holds.
"""

import json
import os
import sys

import onyx

HERE = os.path.dirname(os.path.abspath(__file__))
CORPUS = os.path.join(HERE, "..", "..", "corpus")

failures = []


def fail(name, message):
    failures.append(f"{name}\n    {message}")


def read(*parts):
    with open(os.path.join(CORPUS, *parts), encoding="utf-8") as handle:
        return handle.read()


def check_valid(text, case, name):
    try:
        document = onyx.parse(text)
    except onyx.NotOnyx as error:
        return fail(name, f"rejected: {error} ({error.rule})")

    findings = onyx.validate(document)
    errors = [finding["rule"] for finding in findings if finding["severity"] == "error"]
    if errors:
        return fail(name, "expected no errors, got: " + ", ".join(errors))

    for rule in (case.get("expect") or {}).get("notes", []):
        if not any(f["rule"] == rule and f["severity"] == "info" for f in findings):
            saw = ", ".join(f"{f['rule']}({f['severity']})" for f in findings) or "none"
            fail(name, f"expected a note {rule!r}; findings were: {saw}")

    for rule in (case.get("expect") or {}).get("warns", []):
        if not any(f["rule"] == rule and f["severity"] == "warning" for f in findings):
            saw = ", ".join(f"{f['rule']}({f['severity']})" for f in findings) or "none"
            fail(name, f"expected a warning {rule!r}; findings were: {saw}")


def check_invalid(text, case, name):
    expected = (case.get("expect") or {}).get("rule")
    if not expected:
        return fail(name, "an invalid case must name expect.rule")

    try:
        document = onyx.parse(text)
    except onyx.NotOnyx as error:
        if error.rule != expected:
            fail(name, f"expected {expected!r}, the gate reported {error.rule!r}")
        return

    errors = [f["rule"] for f in onyx.validate(document) if f["severity"] == "error"]
    if expected not in errors:
        fail(name, f"expected error {expected!r}, got: " + (", ".join(errors) or "none"))


def contained(before, after, path, name):
    """Every member of `before` must still be reachable in `after`."""
    if isinstance(before, dict):
        if not isinstance(after, dict):
            return fail(name, f"{path} stopped being an object")
        for key, value in before.items():
            if key not in after:
                return fail(name, f"{path}/{key} was dropped")
            contained(value, after[key], f"{path}/{key}", name)
    elif isinstance(before, list):
        if not isinstance(after, list) or len(before) != len(after):
            return fail(name, f"{path} changed length")
        for index, item in enumerate(before):
            contained(item, after[index], f"{path}/{index}", name)
    elif before != after:
        fail(name, f"{path} changed value")


def check_roundtrip(text, name):
    try:
        once = onyx.parse(text)
        serialised = onyx.serialise(once)
        twice = onyx.parse(serialised)
    except onyx.NotOnyx as error:
        return fail(name, f"rejected: {error}")

    if once != twice:
        return fail(name, "parsing its own output produced a different document")
    if serialised != onyx.serialise(twice):
        return fail(name, "serialising twice produced different text; not idempotent")

    contained(json.loads(text), json.loads(serialised), "", name)


def close(a, b):
    return abs(a - b) <= 1e-6 * max(abs(a), abs(b), 1)


def check_consumer(text, folder, name):
    try:
        document = onyx.parse(text)
    except onyx.NotOnyx as error:
        return fail(name, f"rejected: {error}")

    expected = json.loads(read(folder, "expected.json"))["days"]
    restored = onyx.summarise(document)

    if len(restored) != len(expected):
        return fail(name, f"expected {len(expected)} day(s), restored {len(restored)}")

    for index, want in enumerate(expected):
        got = restored[index]
        if got["date"] != want["date"]:
            fail(name, f"day {index}: expected {want['date']}, restored {got['date']}")
        if got["entryCount"] != want["entryCount"]:
            fail(name, f"{want['date']}: entryCount {got['entryCount']} != {want['entryCount']}")
        for field in ("energyKcal", "bodyMassKg"):
            a, b = got.get(field), want.get(field)
            same = (a is None and b is None) or (a is not None and b is not None and close(a, b))
            if not same:
                fail(name, f"{want['date']} {field}: expected {b}, restored {a}")


# Every ``expect`` key this runner knows how to check.
#
# A runner that ignores an expectation it does not implement reports a pass it never
# established. That is what happened with ``notes``: it was added to the manifest, this
# runner went on ignoring it, and a fixture written specifically to make a message reachable
# passed here against a reader with no info severity at all. Declaring the set means the next
# expectation added to the manifest fails loudly in every implementation that has not caught
# up, which is exactly what should happen.
UNDERSTOOD = {"warns", "notes", "rule", "schemaValid", "accepted"}


def check_expectations_are_understood(name, case):
    unknown = set((case.get("expect") or {})) - UNDERSTOOD
    for key in sorted(unknown):
        fail(name, f"the manifest expects {key!r}, which this runner cannot check")
    return not unknown


def main():
    manifest = json.loads(read("manifest.json"))
    cases = manifest["cases"]

    for case in cases:
        group = case.get("group")
        if "file" in case:
            name, text = case["file"], read(case["file"])
        else:
            name = os.path.join(case["dir"], "document.json")
            text = read(case["dir"], "document.json")

        if not check_expectations_are_understood(name, case):
            continue

        if group == "valid":
            check_valid(text, case, name)
        elif group == "invalid":
            check_invalid(text, case, name)
        elif group == "roundtrip":
            check_roundtrip(text, name)
        elif group == "consumer":
            check_consumer(text, case["dir"], name)
        else:
            fail(name, f"unknown group {group!r}")

    if failures:
        print(f"{len(failures)} of {len(cases)} corpus cases failed:\n", file=sys.stderr)
        for failure in failures:
            print(f"  {failure}\n", file=sys.stderr)
        return 1

    print(f"{len(cases)} corpus cases passed against the Python reader")
    return 0


if __name__ == "__main__":
    sys.exit(main())
