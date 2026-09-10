# SPDX-License-Identifier: MIT OR Apache-2.0
"""A minimal Onyx reader, written to be read.

It exists to prove two things:

1. The specification is complete enough to implement from ``spec/`` and ``corpus/``
   alone, without reading the Rust engine.
2. The conformance corpus is passable by an independent implementation.

If this file ever needs a rule that is not written down in the specification, that is a
bug in the specification rather than in this file.

Standard library only, like the rest of the project.
"""

import json
import datetime
import re

FORMAT = "onyx"
SPEC_MAJOR = 1

# Members are read out of the document dictionary and never removed from it, so anything
# this reader does not understand is still there when the document is written back. In a
# dynamically typed language the must-ignore rule costs nothing at all; the discipline is
# simply never to rebuild a document from the fields you happen to know.


class NotOnyx(Exception):
    """The document is not one this reader can accept, with the rule that says so."""

    def __init__(self, rule, message):
        super().__init__(message)
        self.rule = rule


def parse(text):
    """Read a document and check it is one this build understands.

    This is the identity gate, not the validator. Identity comes from ``format`` — never
    from the file name and never from ``$schema``, which legitimately differs between
    documents.
    """
    try:
        document = json.loads(text)
    except ValueError as error:
        raise NotOnyx("document/unreadable", f"not valid JSON: {error}") from error

    if not isinstance(document, dict):
        raise NotOnyx("document/not-onyx", "a document is a JSON object")

    if document.get("format") != FORMAT:
        raise NotOnyx("document/not-onyx", f"format was {document.get('format')!r}")

    # Semver, and semver gives each version one spelling: no sign and no leading zero. The
    # schema's pattern is \d+ and cannot say that, so this reader and the engine could
    # disagree about "01.0.0" with nothing noticing; the corpus case
    # invalid/spec-version-with-a-leading-zero.json is what keeps them agreeing.
    version = document.get("specVersion")
    if not isinstance(version, str) or not re.fullmatch(
        r"(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)", version
    ):
        raise NotOnyx("document/malformed-version", f"specVersion was {version!r}")

    major = int(version.split(".")[0])
    if major > SPEC_MAJOR:
        # A newer MAJOR may have changed what members mean. Refusing is correct;
        # misreading someone's history is not.
        raise NotOnyx("document/unsupported-major", f"spec major {major} is newer than {SPEC_MAJOR}")

    return document


def serialise(document):
    """Write a document back out.

    Preservation is free here: the dictionary handed back by :func:`parse` still holds
    every member that came in, including those from a later minor version and every
    vendor block.
    """
    return json.dumps(document, indent=2, ensure_ascii=False)


# ── units ────────────────────────────────────────────────────────────────────
# UCUM codes, grouped by the dimension each field expects. Checking by dimension rather
# than against one flat list also catches a valid code in the wrong place, such as energy
# recorded in kilograms.

ENERGY = {"kcal": 1.0, "kJ": 1 / 4.184}
MASS_KG = {
    "kg": 1.0,
    "g": 1e-3,
    "mg": 1e-6,
    "ug": 1e-9,
    "[lb_av]": 0.45359237,
    "[oz_av]": 0.028349523125,
}
LENGTH = {"mm", "cm", "m", "[in_i]", "[ft_i]"}
DURATION = {"a", "mo", "d"}
VOLUME = {"mL", "L", "[foz_us]"}
SCALAR = {"1", "%"}
# A portion may be a mass, a volume, or a count of the servings described alongside it.
PORTION = set(MASS_KG) | VOLUME | SCALAR

# The open vocabularies of v1.0.0. A value outside these is not an error — §3.5 says a
# consumer treats it as absent — but it is worth reporting, and this reader reported none of
# them at all until round ten. The corpus case written to make that message reachable passed
# here without this reader doing anything, which is what a runner that ignores an expectation
# it cannot check buys you.
MEAL_TYPES = {"breakfast", "lunch", "dinner", "snack"}
SOURCES = {"manual", "barcode", "database", "estimated"}
SEXES = {"female", "male", "other"}
GOAL_DIRECTIONS = {"lose", "maintain", "gain"}
GOAL_TYPES = {"targetWeight", "weightDirection"}
MEASUREMENT_TYPES = {"bodyMass"}


def to_kcal(quantity):
    if not isinstance(quantity, dict):
        return None
    factor = ENERGY.get(quantity.get("unit"))
    value = quantity.get("value")
    return value * factor if factor is not None and isinstance(value, (int, float)) else None


def to_kg(quantity):
    if not isinstance(quantity, dict):
        return None
    factor = MASS_KG.get(quantity.get("unit"))
    value = quantity.get("value")
    return value * factor if factor is not None and isinstance(value, (int, float)) else None


def local_date(timestamp):
    """The local calendar date a timestamp was written in.

    No time zone database is needed: an RFC 3339 timestamp shows local time next to its
    offset, so the date it displays is already the local date. Section 3.6 requires folding
    by that offset rather than by the document timeZone, because one document-level zone
    cannot describe a subject who travelled.
    """
    if isinstance(timestamp, str) and re.match(r"^\d{4}-\d{2}-\d{2}T", timestamp):
        return timestamp[:10]
    return None


def instant(timestamp):
    """Seconds from a fixed epoch, with the offset applied.

    Needed to order two timestamps written in different zones. Comparing the strings would
    order by wall clock and put 08:00+09:00 after 08:00+02:00, which is backwards by seven
    hours. Mirrors ``Timestamp::instant`` in the Rust engine — the two must agree, because
    the corpus asserts a single answer for both.
    """
    match = re.match(
        r"^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2}):(\d{2})(?:\.\d+)?([+-])(\d{2}):(\d{2})$",
        timestamp if isinstance(timestamp, str) else "",
    )
    if match is None:
        return None
    year, month, day, hour, minute, second, sign, oh, om = match.groups()
    days = datetime.date(int(year), int(month), int(day)).toordinal()
    local = days * 86400 + int(hour) * 3600 + int(minute) * 60 + int(second)
    east = (int(oh) * 60 + int(om)) * (1 if sign == "+" else -1)
    return local - east * 60


# ── validation ───────────────────────────────────────────────────────────────
# Only the rules the conformance corpus asserts. The Rust engine carries more; a
# conforming consumer is not obliged to, and keeping this short is the point of the file.


def _finding(severity, rule, path, message):
    return {"severity": severity, "rule": rule, "path": path, "message": message}


def _check_unit(quantity, allowed, dimension, path, findings):
    """Both members, then the unit.

    Completeness comes first because a quantity missing ``value`` is not a unit problem,
    and because reading the absent number as 0 turns an unreadable entry into a confident
    zero. ``allowed`` of ``None`` means the dimension depends on an open vocabulary this
    reader does not recognise: sign and completeness still apply, the unit cannot.
    """
    if not isinstance(quantity, dict):
        return

    value, unit = quantity.get("value"), quantity.get("unit")
    has_value = isinstance(value, (int, float)) and not isinstance(value, bool)
    if not has_value or not isinstance(unit, str):
        missing = []
        if not has_value:
            missing.append("`value`")
        if not isinstance(unit, str):
            missing.append("`unit`")
        findings.append(
            _finding(
                "error",
                "quantity/incomplete",
                path,
                f"this quantity has no {' or '.join(missing)}; both are required",
            )
        )
        return

    if allowed is not None and unit not in allowed:
        findings.append(
            _finding(
                "error",
                "unit/dimension",
                f"{path}/unit",
                f"{unit!r} is not a UCUM {dimension} code",
            )
        )


def _check_atwater(nutrients, path, findings):
    """Energy against its macronutrients.

    Nothing can prove from one document that nutrients are per-portion rather than per
    100 g, but the arithmetic catches it: macros stated per 100 g beside a per-portion
    energy produce a sum wildly out of proportion.
    """
    energy = to_kcal(nutrients.get("energy"))
    grams = [to_kg(nutrients.get(name)) for name in ("protein", "carbohydrate", "fat")]
    if energy is None or any(value is None for value in grams) or energy < 50:
        return

    protein, carbohydrate, fat = (value * 1000 for value in grams)
    predicted = 4 * protein + 4 * carbohydrate + 9 * fat
    if predicted <= 0:
        return

    ratio = predicted / energy
    if not 0.75 <= ratio <= 1.25:
        findings.append(
            _finding(
                "warning",
                "nutrients/atwater",
                path,
                f"macros imply {predicted:.0f} kcal but energy says {energy:.0f}",
            )
        )


def _check_timestamp(timestamp, path, findings):
    if isinstance(timestamp, str) and timestamp.endswith(("Z", "z")):
        findings.append(
            _finding(
                "error",
                "time/utc-normalised",
                path,
                "normalised to UTC, so the local day this belonged to is gone",
            )
        )


def _check_vocabulary(value, known, noun, path, findings):
    """Report a value outside an open vocabulary as a note, never as an error."""
    if isinstance(value, str) and value not in known:
        findings.append(
            _finding(
                "info",
                "vocabulary/unknown",
                path,
                f"{value!r} is not a {noun} v1.0.0 defines; a consumer should treat it as absent",
            )
        )


def validate(document):
    """Return findings. Only ``error`` findings make a document non-conforming."""
    findings = []

    _check_timestamp(document.get("exportedAt"), "/exportedAt", findings)

    if "timeZone" not in document:
        findings.append(
            _finding(
                "warning",
                "document/missing-timezone",
                "/timeZone",
                "no IANA zone, so folding measurements onto local days is guesswork",
            )
        )

    subject = document.get("subject")
    if isinstance(subject, dict):
        _check_unit(subject.get("age"), DURATION, "duration", "/subject/age", findings)
        _check_unit(subject.get("height"), LENGTH, "length", "/subject/height", findings)
        _check_vocabulary(subject.get("sex"), SEXES, "sex", "/subject/sex", findings)

    for index, goal in enumerate(document.get("goals") or []):
        if not isinstance(goal, dict):
            continue
        path = f"/goals/{index}"
        _check_vocabulary(goal.get("type"), GOAL_TYPES, "goal type", f"{path}/type", findings)
        _check_vocabulary(
            goal.get("direction"), GOAL_DIRECTIONS, "direction", f"{path}/direction", findings
        )
        if goal.get("type") == "targetWeight":
            _check_unit(goal.get("value"), MASS_KG, "mass", f"{path}/value", findings)
        else:
            # An open vocabulary: the dimension is unknown, so only completeness applies.
            _check_unit(goal.get("value"), None, "", f"{path}/value", findings)

    seen = {}
    for index, day in enumerate(document.get("days") or []):
        if not isinstance(day, dict):
            continue
        base = f"/days/{index}"

        date = day.get("date")
        if date in seen:
            findings.append(
                _finding(
                    "error",
                    "day/duplicate-date",
                    f"{base}/date",
                    f"{date} already appears at /days/{seen[date]}",
                )
            )
        elif isinstance(date, str):
            seen[date] = index

        for key in ("energyTarget", "energyConsumed"):
            _check_unit(day.get(key), ENERGY, "energy", f"{base}/{key}", findings)

        for position, entry in enumerate(day.get("entries") or []):
            if not isinstance(entry, dict):
                continue
            entry_path = f"{base}/entries/{position}"

            _check_timestamp(entry.get("loggedAt"), f"{entry_path}/loggedAt", findings)
            _check_vocabulary(
                entry.get("mealType"), MEAL_TYPES, "mealType", f"{entry_path}/mealType", findings
            )
            _check_vocabulary(
                entry.get("source"), SOURCES, "source", f"{entry_path}/source", findings
            )

            confidence = entry.get("confidence")
            if isinstance(confidence, (int, float)) and not isinstance(confidence, bool):
                if not 0.0 <= confidence <= 1.0:
                    findings.append(
                        _finding(
                            "error",
                            "entry/confidence-range",
                            f"{entry_path}/confidence",
                            f"confidence is {confidence}; §3.5 defines it on 0..1",
                        )
                    )
            _check_unit(entry.get("quantity"), PORTION, "portion", f"{entry_path}/quantity", findings)

            # `%` is in the schema's portion enum and has no meaning in §3.4. Accepted,
            # because the schema says so, and reported, because it names no whole.
            quantity = entry.get("quantity")
            if isinstance(quantity, dict) and quantity.get("unit") == "%":
                findings.append(
                    _finding(
                        "warning",
                        "quantity/undefined-portion-unit",
                        f"{entry_path}/quantity/unit",
                        "`%` names no whole for the percentage to be of",
                    )
                )

            nutrients = entry.get("nutrients")
            if isinstance(nutrients, dict):
                path = f"{entry_path}/nutrients"
                _check_unit(nutrients.get("energy"), ENERGY, "energy", f"{path}/energy", findings)
                for macro in ("protein", "carbohydrate", "fat"):
                    _check_unit(nutrients.get(macro), MASS_KG, "mass", f"{path}/{macro}", findings)
                _check_atwater(nutrients, path, findings)

    for index, measurement in enumerate(document.get("bodyMeasurements") or []):
        if not isinstance(measurement, dict):
            continue
        path = f"/bodyMeasurements/{index}"
        _check_vocabulary(
            measurement.get("type"),
            MEASUREMENT_TYPES,
            "measurement type",
            f"{path}/type",
            findings,
        )
        _check_timestamp(measurement.get("observedAt"), f"{path}/observedAt", findings)
        if measurement.get("type") == "bodyMass":
            _check_unit(measurement.get("value"), MASS_KG, "mass", f"{path}/value", findings)

    return findings


def conforming(findings):
    return not any(finding["severity"] == "error" for finding in findings)


# ── canonical restoration ────────────────────────────────────────────────────


def summarise(document):
    """Rebuild the diary from the portable layer alone.

    This is the specification's real test (§5). It reads ``days`` and
    ``bodyMeasurements`` and nothing else — in particular it cannot see inside a vendor
    block, so what it returns is what *any* conforming consumer must recover from the
    same file. A consumer that only reads its own files has implemented a backup.
    """
    days = []
    by_date = {}
    latest = {}

    for day in document.get("days") or []:
        if not isinstance(day, dict):
            continue
        entries = [entry for entry in (day.get("entries") or []) if isinstance(entry, dict)]

        recorded = to_kcal(day.get("energyConsumed"))
        summed = [to_kcal((entry.get("nutrients") or {}).get("energy")) for entry in entries]
        summed = [value for value in summed if value is not None]

        restored = {
            "date": day.get("date"),
            "entryCount": len(entries),
            # The recorded total wins: §3.3 allows it to differ from the sum, and it is
            # the figure the subject actually saw.
            "energyKcal": recorded if recorded is not None else (sum(summed) if summed else None),
            "bodyMassKg": None,
        }
        by_date[restored["date"]] = restored
        days.append(restored)

    for measurement in document.get("bodyMeasurements") or []:
        if not isinstance(measurement, dict) or measurement.get("type") != "bodyMass":
            continue
        date = local_date(measurement.get("observedAt"))
        kilograms = to_kg(measurement.get("value"))
        if date is None or kilograms is None:
            continue

        when = instant(measurement.get("observedAt"))
        if when is None:
            continue

        matching = [day for day in days if day["date"] == date]
        if matching:
            # The latest reading of the day wins, by the clock rather than by position in
            # the array — and it reaches every record carrying that date, because duplicate
            # dates are an error this function does not itself check for.
            if date not in latest or when >= latest[date]:
                for day in matching:
                    day["bodyMassKg"] = kilograms
                latest[date] = when
        else:
            # A weigh-in with no food logged that day still makes the day real.
            restored = {"date": date, "entryCount": 0, "energyKcal": None, "bodyMassKg": kilograms}
            days.append(restored)
            latest[date] = when

    days.sort(key=lambda day: day["date"] or "")
    return days
