//! Mapping between the document types and JSON.
//!
//! This is what `#[derive(Serialize, Deserialize)]` used to do. Keeping it in one file
//! rather than beside each type is deliberate: the mapping is the part that has to stay
//! in step with the specification, and drift is easier to spot when it is all in one place.
//!
//! # Two rules run through everything here
//!
//! **Unknown members are preserved.** Reading takes members out of a working copy of the
//! object; whatever is left over becomes `extra` and is written back out untouched. That
//! is the must-ignore rule, implemented once.
//!
//! **A member of the wrong shape is left alone rather than discarded.** If `quantity` is a
//! string where an object belongs, it is not consumed, so it stays in `extra` and survives
//! the round trip while the typed field reads as absent. Refusing the whole document would
//! punish a reader for a producer's mistake; dropping the member would lose data silently.
//! The validator is what reports it.

use crate::diary::{Day, FoodEntry, FoodIdentifiers, Nutrients};
use crate::document::{Document, Producer, Quantity};
use crate::json::{Object, Value};
use crate::profile::{Goal, Measurement, PreferredUnits, Subject};
use crate::vocabulary::{GoalDirection, MealType, Sex, Source};

// ── reading helpers ──────────────────────────────────────────────────────────

/// Removes a string member, leaving it in place if it is not a string.
fn take_string(object: &mut Object, key: &str) -> Option<String> {
    if !matches!(object.get(key), Some(Value::String(_))) {
        return None;
    }
    match object.remove(key) {
        Some(Value::String(text)) => Some(text),
        _ => None,
    }
}

fn take_i64(object: &mut Object, key: &str) -> Option<i64> {
    object.get(key).and_then(Value::as_i64)?;
    object.remove(key).and_then(|value| value.as_i64())
}

fn take_f64(object: &mut Object, key: &str) -> Option<f64> {
    object.get(key).and_then(Value::as_f64)?;
    object.remove(key).and_then(|value| value.as_f64())
}

/// Removes an object member and reads it with `read`, leaving it in place if it is not an
/// object.
fn take_object<T>(object: &mut Object, key: &str, read: impl Fn(Object) -> T) -> Option<T> {
    if !matches!(object.get(key), Some(Value::Object(_))) {
        return None;
    }
    match object.remove(key) {
        Some(Value::Object(inner)) => Some(read(inner)),
        _ => None,
    }
}

/// Removes an array member and reads it, **only if every element is an object**.
///
/// If any element is not, the whole array is left where it is, exactly as `take_object`
/// leaves a member that is not an object. It then survives in `extra` untouched and is
/// written back byte for byte.
///
/// The previous version filtered non-objects out with `filter_map`, so `[{...}, 42, {...}]`
/// parsed to two entries and was *written back with the 42 gone*. That is silent data loss,
/// and the roundtrip corpus could not catch it: those cases assert that what survives is
/// unchanged, not that nothing left.
fn take_array<T>(object: &mut Object, key: &str, read: impl Fn(Object) -> T) -> Option<Vec<T>> {
    let Some(Value::Array(items)) = object.get(key) else {
        return None;
    };
    if !items.iter().all(|item| matches!(item, Value::Object(_))) {
        return None;
    }

    match object.remove(key) {
        Some(Value::Array(items)) => Some(
            items
                .into_iter()
                .map(|item| match item {
                    Value::Object(inner) => read(inner),
                    // Unreachable: every element was checked above.
                    _ => unreachable!("take_array checked every element is an object"),
                })
                .collect(),
        ),
        _ => None,
    }
}

// ── writing helpers ──────────────────────────────────────────────────────────

/// Writes a member only when it is present, so an absent section is never materialised as
/// an empty one.
fn put(object: &mut Object, key: &str, value: Option<Value>) {
    if let Some(value) = value {
        object.insert(key, value);
    }
}

fn put_str(object: &mut Object, key: &str, value: &Option<String>) {
    put(object, key, value.clone().map(Value::String));
}

/// Writes a member the format requires — unless this build has nothing to write for it.
///
/// These are `String` rather than `Option<String>`, and that is deliberate rather than an
/// oversight repeated from [`Quantity`]. The distinction is whether the empty value is a
/// real one. A quantity's `value` of `0` is a genuine measurement — a zero-calorie food
/// exists — so absent and zero had to become different states, and they did. An empty
/// string is not a date, not a timestamp, not a measurement type and not a producer name:
/// no position here defines it, so absent and empty are the same fact and one type can
/// carry both.
///
/// What is not acceptable either way is writing the empty one back. A document with no
/// `date` that comes out carrying `"date": ""` has been edited, not processed.
fn put_required(object: &mut Object, key: &str, value: &str) {
    if !value.is_empty() {
        object.insert(key, Value::from(value.to_owned()));
    }
}

fn put_quantity(object: &mut Object, key: &str, value: &Option<Quantity>) {
    put(object, key, value.as_ref().map(Quantity::to_json));
}

fn finish(mut object: Object, extra: &Object) -> Value {
    object.extend(extra.clone());
    Value::Object(object)
}

// ── quantities, producers, vocabularies ──────────────────────────────────────

impl Quantity {
    pub(crate) fn from_object(mut object: Object) -> Self {
        // No `unwrap_or`. A missing or malformed member stays missing: inventing 0.0 here
        // is what turned an unreadable energy into a recorded zero.
        Self {
            value: take_f64(&mut object, "value"),
            unit: take_string(&mut object, "unit"),
            extra: object,
        }
    }

    pub(crate) fn to_json(&self) -> Value {
        let mut object = Object::new();
        put(&mut object, "value", self.value.map(Value::from));
        put_str(&mut object, "unit", &self.unit);
        finish(object, &self.extra)
    }
}

impl Producer {
    pub(crate) fn from_object(mut object: Object) -> Self {
        Self {
            name: take_string(&mut object, "name"),
            version: take_string(&mut object, "version"),
            platform: take_string(&mut object, "platform"),
            extra: object,
        }
    }

    pub(crate) fn to_json(&self) -> Value {
        let mut object = Object::new();
        put_str(&mut object, "name", &self.name);
        put_str(&mut object, "version", &self.version);
        put_str(&mut object, "platform", &self.platform);
        finish(object, &self.extra)
    }
}

// ── the diary ────────────────────────────────────────────────────────────────

impl Nutrients {
    pub(crate) fn from_object(mut object: Object) -> Self {
        Self {
            energy: take_object(&mut object, "energy", Quantity::from_object),
            protein: take_object(&mut object, "protein", Quantity::from_object),
            carbohydrate: take_object(&mut object, "carbohydrate", Quantity::from_object),
            fat: take_object(&mut object, "fat", Quantity::from_object),
            extra: object,
        }
    }

    pub(crate) fn to_json(&self) -> Value {
        let mut object = Object::new();
        put_quantity(&mut object, "energy", &self.energy);
        put_quantity(&mut object, "protein", &self.protein);
        put_quantity(&mut object, "carbohydrate", &self.carbohydrate);
        put_quantity(&mut object, "fat", &self.fat);
        finish(object, &self.extra)
    }
}

impl FoodIdentifiers {
    pub(crate) fn from_object(mut object: Object) -> Self {
        Self {
            gtin: take_string(&mut object, "gtin"),
            fdc_id: take_i64(&mut object, "fdcId"),
            off_id: take_string(&mut object, "offId"),
            extra: object,
        }
    }

    pub(crate) fn to_json(&self) -> Value {
        let mut object = Object::new();
        put_str(&mut object, "gtin", &self.gtin);
        put(&mut object, "fdcId", self.fdc_id.map(Value::from));
        put_str(&mut object, "offId", &self.off_id);
        finish(object, &self.extra)
    }
}

impl FoodEntry {
    pub(crate) fn from_object(mut object: Object) -> Self {
        Self {
            logged_at: take_string(&mut object, "loggedAt"),
            meal_type: take_string(&mut object, "mealType").map(MealType::from),
            name: take_string(&mut object, "name"),
            identifiers: take_object(&mut object, "identifiers", FoodIdentifiers::from_object),
            quantity: take_object(&mut object, "quantity", Quantity::from_object),
            serving_description: take_string(&mut object, "servingDescription"),
            nutrients: take_object(&mut object, "nutrients", Nutrients::from_object),
            source: take_string(&mut object, "source").map(Source::from),
            confidence: take_f64(&mut object, "confidence"),
            note: take_string(&mut object, "note"),
            extra: object,
        }
    }

    pub(crate) fn to_json(&self) -> Value {
        let mut object = Object::new();
        put_str(&mut object, "loggedAt", &self.logged_at);
        put(
            &mut object,
            "mealType",
            self.meal_type.as_ref().map(|v| Value::from(v.as_str())),
        );
        put_str(&mut object, "name", &self.name);
        put(
            &mut object,
            "identifiers",
            self.identifiers.as_ref().map(FoodIdentifiers::to_json),
        );
        put_quantity(&mut object, "quantity", &self.quantity);
        put_str(&mut object, "servingDescription", &self.serving_description);
        put(
            &mut object,
            "nutrients",
            self.nutrients.as_ref().map(Nutrients::to_json),
        );
        put(
            &mut object,
            "source",
            self.source.as_ref().map(|v| Value::from(v.as_str())),
        );
        put(&mut object, "confidence", self.confidence.map(Value::from));
        put_str(&mut object, "note", &self.note);
        finish(object, &self.extra)
    }
}

impl Day {
    pub(crate) fn from_object(mut object: Object) -> Self {
        Self {
            date: take_string(&mut object, "date"),
            energy_target: take_object(&mut object, "energyTarget", Quantity::from_object),
            energy_consumed: take_object(&mut object, "energyConsumed", Quantity::from_object),
            entries: take_array(&mut object, "entries", FoodEntry::from_object),
            note: take_string(&mut object, "note"),
            extra: object,
        }
    }

    pub(crate) fn to_json(&self) -> Value {
        let mut object = Object::new();
        put_str(&mut object, "date", &self.date);
        put_quantity(&mut object, "energyTarget", &self.energy_target);
        put_quantity(&mut object, "energyConsumed", &self.energy_consumed);
        put(
            &mut object,
            "entries",
            self.entries
                .as_ref()
                .map(|entries| Value::Array(entries.iter().map(FoodEntry::to_json).collect())),
        );
        put_str(&mut object, "note", &self.note);
        finish(object, &self.extra)
    }
}

// ── subject, goals, measurements ─────────────────────────────────────────────

impl PreferredUnits {
    pub(crate) fn from_object(mut object: Object) -> Self {
        Self {
            mass: take_string(&mut object, "mass"),
            energy: take_string(&mut object, "energy"),
            extra: object,
        }
    }

    pub(crate) fn to_json(&self) -> Value {
        let mut object = Object::new();
        put_str(&mut object, "mass", &self.mass);
        put_str(&mut object, "energy", &self.energy);
        finish(object, &self.extra)
    }
}

impl Subject {
    pub(crate) fn from_object(mut object: Object) -> Self {
        Self {
            sex: take_string(&mut object, "sex").map(Sex::from),
            age: take_object(&mut object, "age", Quantity::from_object),
            height: take_object(&mut object, "height", Quantity::from_object),
            preferred_units: take_object(
                &mut object,
                "preferredUnits",
                PreferredUnits::from_object,
            ),
            extra: object,
        }
    }

    pub(crate) fn to_json(&self) -> Value {
        let mut object = Object::new();
        put(
            &mut object,
            "sex",
            self.sex.as_ref().map(|v| Value::from(v.as_str())),
        );
        put_quantity(&mut object, "age", &self.age);
        put_quantity(&mut object, "height", &self.height);
        put(
            &mut object,
            "preferredUnits",
            self.preferred_units.as_ref().map(PreferredUnits::to_json),
        );
        finish(object, &self.extra)
    }
}

impl Goal {
    pub(crate) fn from_object(mut object: Object) -> Self {
        Self {
            kind: take_string(&mut object, "type"),
            value: take_object(&mut object, "value", Quantity::from_object),
            direction: take_string(&mut object, "direction").map(GoalDirection::from),
            extra: object,
        }
    }

    pub(crate) fn to_json(&self) -> Value {
        let mut object = Object::new();
        put_str(&mut object, "type", &self.kind);
        put_quantity(&mut object, "value", &self.value);
        put(
            &mut object,
            "direction",
            self.direction.as_ref().map(|v| Value::from(v.as_str())),
        );
        finish(object, &self.extra)
    }
}

impl Measurement {
    pub(crate) fn from_object(mut object: Object) -> Self {
        Self {
            observed_at: take_string(&mut object, "observedAt"),
            kind: take_string(&mut object, "type"),
            value: take_object(&mut object, "value", Quantity::from_object),
            extra: object,
        }
    }

    pub(crate) fn to_json(&self) -> Value {
        let mut object = Object::new();
        put_str(&mut object, "observedAt", &self.observed_at);
        put_str(&mut object, "type", &self.kind);
        put_quantity(&mut object, "value", &self.value);
        finish(object, &self.extra)
    }
}

// ── the document ─────────────────────────────────────────────────────────────

impl Document {
    /// Reads a document from an already-parsed value.
    ///
    /// Shape only: whether this *is* an ONYX document, and whether this build understands
    /// its major version, is decided by [`crate::parse`].
    pub(crate) fn from_object(mut object: Object) -> Self {
        Self {
            schema: take_string(&mut object, "$schema"),
            format: take_string(&mut object, "format").unwrap_or_default(),
            spec_version: take_string(&mut object, "specVersion").unwrap_or_default(),
            exported_at: take_string(&mut object, "exportedAt"),
            time_zone: take_string(&mut object, "timeZone"),
            producer: take_object(&mut object, "producer", Producer::from_object),
            subject: take_object(&mut object, "subject", Subject::from_object),
            goals: take_array(&mut object, "goals", Goal::from_object),
            days: take_array(&mut object, "days", Day::from_object),
            body_measurements: take_array(
                &mut object,
                "bodyMeasurements",
                Measurement::from_object,
            ),
            extensions: take_object(&mut object, "extensions", |inner| inner),
            extra: object,
        }
    }

    /// Writes the document, known members first in the order the specification documents
    /// them, then anything preserved from a version this build does not know.
    pub(crate) fn to_json(&self) -> Value {
        let mut object = Object::new();
        put_str(&mut object, "$schema", &self.schema);
        put_required(&mut object, "format", &self.format);
        put_required(&mut object, "specVersion", &self.spec_version);
        put_str(&mut object, "exportedAt", &self.exported_at);
        put_str(&mut object, "timeZone", &self.time_zone);
        put(
            &mut object,
            "producer",
            self.producer.as_ref().map(Producer::to_json),
        );
        put(
            &mut object,
            "subject",
            self.subject.as_ref().map(Subject::to_json),
        );
        put(
            &mut object,
            "goals",
            self.goals
                .as_ref()
                .map(|goals| Value::Array(goals.iter().map(Goal::to_json).collect())),
        );
        put(
            &mut object,
            "days",
            self.days
                .as_ref()
                .map(|days| Value::Array(days.iter().map(Day::to_json).collect())),
        );
        put(
            &mut object,
            "bodyMeasurements",
            self.body_measurements
                .as_ref()
                .map(|items| Value::Array(items.iter().map(Measurement::to_json).collect())),
        );
        put(
            &mut object,
            "extensions",
            self.extensions.clone().map(Value::Object),
        );
        finish(object, &self.extra)
    }
}

#[cfg(test)]
mod tests {
    use crate::json;

    /// A document populating **every member this build types**, in every struct.
    ///
    /// The round-trip test in `lib.rs` uses a realistic fixture, which means it only
    /// guards members that fixture happens to contain — `preferredUnits`,
    /// `servingDescription`, `energyTarget`, `note`, `offId` and `platform` were all
    /// invisible to it. This one exists to be exhaustive rather than realistic.
    ///
    /// **Add a member here whenever you add a typed field.** The two tests below then
    /// cover both halves of the mapping: that reading claims it, and that writing puts it
    /// back.
    const COMPLETE: &str = r#"{
      "$schema": "https://cdn.jsdelivr.net/gh/dsemakin/onyx@v1.0.0/spec/v1/log.schema.json",
      "format": "onyx",
      "specVersion": "1.0.0",
      "exportedAt": "2026-08-14T12:00:00+03:00",
      "timeZone": "Europe/Berlin",
      "producer": { "name": "Complete", "version": "9.9.9", "platform": "android" },
      "subject": {
        "sex": "female",
        "age": { "value": 34, "unit": "a" },
        "height": { "value": 165, "unit": "cm" },
        "preferredUnits": { "mass": "kg", "energy": "kcal" }
      },
      "goals": [
        { "type": "targetWeight", "value": { "value": 62, "unit": "kg" } },
        { "type": "weightDirection", "direction": "loss" }
      ],
      "days": [
        {
          "date": "2026-08-10",
          "energyTarget": { "value": 2100, "unit": "kcal" },
          "energyConsumed": { "value": 389, "unit": "kcal" },
          "note": "a note on the day",
          "entries": [
            {
              "loggedAt": "2026-08-10T08:30:00+03:00",
              "mealType": "breakfast",
              "name": "Oats",
              "identifiers": { "gtin": "05011234567890", "fdcId": 169705, "offId": "3017620422003" },
              "quantity": { "value": 100, "unit": "g" },
              "servingDescription": "1 cup",
              "nutrients": {
                "energy": { "value": 389, "unit": "kcal" },
                "protein": { "value": 16.9, "unit": "g" },
                "carbohydrate": { "value": 66, "unit": "g" },
                "fat": { "value": 6.9, "unit": "g" }
              },
              "source": "estimated",
              "confidence": 0.9,
              "note": "a note on the entry"
            }
          ]
        }
      ],
      "bodyMeasurements": [
        { "observedAt": "2026-08-10T07:00:00+03:00", "type": "bodyMass",
          "value": { "value": 80.4, "unit": "kg" } }
      ],
      "extensions": { "com.example.tracker": { "blockVersion": 4 } }
    }"#;

    /// Every member the fixture carries must be claimed by a typed field.
    ///
    /// This is the *reading* half of the mapping. If a `from_object` stops consuming a
    /// member, it falls through into `extra` and this fails — which is how a half-finished
    /// rename gets caught before it reaches anyone.
    #[test]
    fn every_typed_member_is_claimed_on_the_way_in() {
        let document = crate::parse(COMPLETE).expect("the complete fixture must parse");

        assert!(
            document.extra.is_empty(),
            "unclaimed at the top level: {:?}",
            document.extra
        );
        assert!(
            document
                .producer
                .as_ref()
                .is_none_or(|p| p.extra.is_empty()),
            "unclaimed in producer"
        );

        let subject = document.subject.as_ref().expect("subject");
        assert!(subject.extra.is_empty(), "unclaimed in subject");
        assert!(
            subject.preferred_units.as_ref().unwrap().extra.is_empty(),
            "unclaimed in preferredUnits"
        );

        for goal in document.goals.as_ref().expect("goals") {
            assert!(goal.extra.is_empty(), "unclaimed in a goal");
        }

        let day = &document.days()[0];
        assert!(day.extra.is_empty(), "unclaimed in day");

        let entry = &day.entries.as_ref().expect("entries")[0];
        assert!(entry.extra.is_empty(), "unclaimed in entry");
        assert!(
            entry.identifiers.as_ref().unwrap().extra.is_empty(),
            "unclaimed in identifiers"
        );
        assert!(
            entry.nutrients.as_ref().unwrap().extra.is_empty(),
            "unclaimed in nutrients"
        );
        assert!(
            entry.quantity.as_ref().unwrap().extra.is_empty(),
            "unclaimed in quantity"
        );

        let measurement = &document.body_measurements()[0];
        assert!(measurement.extra.is_empty(), "unclaimed in measurement");
        assert!(
            measurement
                .value
                .as_ref()
                .is_none_or(|v| v.extra.is_empty()),
            "unclaimed in a measurement value"
        );
    }

    /// Every member the fixture carries must come back out.
    ///
    /// This is the *writing* half. A `to_json` that forgets a field its `from_object`
    /// consumes is silent data loss — the member is taken out of `extra` on the way in and
    /// never written on the way out — and it is the failure this whole file most needs
    /// guarding against.
    #[test]
    fn every_typed_member_survives_the_way_out() {
        let document = crate::parse(COMPLETE).expect("the complete fixture must parse");
        let written = crate::to_string_pretty(&document);

        let before = json::parse(COMPLETE).expect("fixture is valid JSON");
        let after = json::parse(&written).expect("our own output is valid JSON");
        contained(&before, &after, "");
    }

    /// Asserts every member of `before` is still reachable in `after`, at any depth.
    fn contained(before: &json::Value, after: &json::Value, path: &str) {
        match before {
            json::Value::Object(members) => {
                let after = after
                    .as_object()
                    .unwrap_or_else(|| panic!("{path} stopped being an object"));
                for (key, value) in members.iter() {
                    let found = after
                        .get(key)
                        .unwrap_or_else(|| panic!("{path}/{key} was dropped on the way out"));
                    contained(value, found, &format!("{path}/{key}"));
                }
            }
            json::Value::Array(items) => {
                let after = after
                    .as_array()
                    .unwrap_or_else(|| panic!("{path} stopped being an array"));
                assert_eq!(items.len(), after.len(), "{path} changed length");
                for (index, item) in items.iter().enumerate() {
                    contained(item, &after[index], &format!("{path}/{index}"));
                }
            }
            scalar => assert_eq!(scalar, after, "{path} changed value"),
        }
    }
}
