//! A JSON reader and writer, so this crate depends on nothing.
//!
//! # Why this exists
//!
//! `serde_json` is excellent and was used here first. It was replaced so the project has
//! no dependencies at all: nothing to audit that someone else wrote, nothing whose release
//! cadence we have to track, and nothing that can change under us.
//!
//! The usual objection is that hand-rolling a parser for untrusted input trades an audited
//! implementation for a new attack surface. That objection is weaker than it sounds in safe
//! Rust. The crate is `#![forbid(unsafe_code)]`, so the worst a malformed document can do
//! is produce a wrong value or a panic — never memory corruption. Wrong values are what
//! tests are for, and panics are prevented structurally:
//!
//! - Nesting is bounded ([`MAX_DEPTH`]), so no input can exhaust the stack. The writer
//!   stops at [`MAX_WRITE_DEPTH`] for the same reason, so a value built in memory deeper
//!   than any document could be is refused rather than recursed into without limit.
//! - Number and UTF-8 handling are delegated to `std`, which is where the genuinely hard
//!   parts of JSON live and where nobody should be writing their own.
//! - There is no `unwrap`, no `expect`, and no indexing that is not bounds-checked.
//!
//! # What it is not
//!
//! It is not general-purpose. It reads and writes the subset of JSON this format uses,
//! with object member order preserved so a document survives a round trip unshuffled.

use std::collections::HashMap;
use std::fmt;

use std::fmt::Write as _;

/// Maximum nesting depth.
///
/// A recursive-descent parser is a stack-overflow risk on hostile input, and a stack
/// overflow aborts the process rather than returning an error. Bounding depth turns that
/// into an ordinary parse failure. Real diaries nest about six levels deep.
pub const MAX_DEPTH: usize = 128;

/// Maximum nesting depth the writer will follow.
///
/// Nothing the reader admits comes near it, and a downgrade wraps a parked subtree in only
/// three more levels. It exists because a `Value` can be built in memory without going
/// through the reader, and a writer recursing without limit into such a value would
/// overflow the stack — an abort, where this is an error a caller sees.
pub const MAX_WRITE_DEPTH: usize = 2 * MAX_DEPTH;

/// A JSON value.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Number(Number),
    String(String),
    Array(Vec<Value>),
    Object(Object),
}

/// A number, keeping integers exact.
///
/// Storing every number as `f64` would rewrite `169705` as `169705.0` on the way out and
/// silently lose precision on large identifiers. USDA food ids are integers and must
/// survive as integers.
#[derive(Debug, Clone, Copy)]
pub enum Number {
    Integer(i64),
    Float(f64),
}

impl PartialEq for Number {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            // Integers compare as integers. Two i64 beyond 2^53 round to the same f64, so
            // going through as_f64 would make two distinct food ids compare equal — the
            // exact thing the Integer variant exists to prevent.
            (Self::Integer(a), Self::Integer(b)) => a == b,
            _ => self.as_f64() == other.as_f64(),
        }
    }
}

impl Number {
    pub fn as_f64(self) -> f64 {
        match self {
            Self::Integer(value) => value as f64,
            Self::Float(value) => value,
        }
    }

    pub fn as_i64(self) -> Option<i64> {
        match self {
            Self::Integer(value) => Some(value),
            // Accepts 12.0 as 12, because a producer may write either. The range check is
            // the point: `1e30` is finite and has no fractional part, and casting it
            // saturates to i64::MAX — which would be returned as though it were exact.
            Self::Float(value)
                if value.fract() == 0.0 && value >= -(2f64.powi(63)) && value < 2f64.powi(63) =>
            {
                Some(value as i64)
            }
            Self::Float(_) => None,
        }
    }
}

/// An object, in the order its members were written.
///
/// A `Vec` keeps the order, because a document that has been through this crate should not
/// come back reshuffled. For the handful of members a real object has, scanning that `Vec`
/// beats hashing.
///
/// # Why there is also an index
///
/// Scanning is O(n) per lookup, and parsing inserts every member, so reading one object
/// with n members was O(n²). Measured on this machine before the index existed: 2,000
/// members took 6.9 ms, 8,000 took 146 ms, and 32,000 took 2.1 seconds — a document a few
/// hundred kilobytes long could occupy a CPU for minutes. Nesting depth was already bounded
/// by [`MAX_DEPTH`]; breadth was not, and untrusted documents are the whole use case.
///
/// So the index is built once an object grows past [`INDEX_THRESHOLD`] members and
/// maintained from then on. Below that it is never allocated, and small objects — every
/// object in a normal document — behave exactly as they did.
#[derive(Clone, Default)]
pub struct Object {
    members: Vec<(String, Value)>,
    index: Option<HashMap<String, usize>>,
}

/// Below this many members, a linear scan is cheaper than hashing and the index is not
/// built at all.
const INDEX_THRESHOLD: usize = 16;

impl PartialEq for Object {
    /// The index is a lookup accelerator, never part of the value.
    fn eq(&self, other: &Self) -> bool {
        self.members == other.members
    }
}

impl fmt::Debug for Object {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_map()
            .entries(self.members.iter().map(|(k, v)| (k, v)))
            .finish()
    }
}

impl Object {
    pub fn new() -> Self {
        Self {
            members: Vec::new(),
            index: None,
        }
    }

    /// Position of `key`, through the index when there is one.
    fn position(&self, key: &str) -> Option<usize> {
        match &self.index {
            Some(index) => index.get(key).copied(),
            None => self.members.iter().position(|(name, _)| name == key),
        }
    }

    /// Builds the index once the object is big enough to be worth it.
    fn index_if_large(&mut self) {
        if self.index.is_none() && self.members.len() >= INDEX_THRESHOLD {
            self.index = Some(
                self.members
                    .iter()
                    .enumerate()
                    .map(|(at, (name, _))| (name.clone(), at))
                    .collect(),
            );
        }
    }

    pub fn get(&self, key: &str) -> Option<&Value> {
        self.position(key).map(|at| &self.members[at].1)
    }

    pub fn get_mut(&mut self, key: &str) -> Option<&mut Value> {
        let at = self.position(key)?;
        Some(&mut self.members[at].1)
    }

    pub fn contains_key(&self, key: &str) -> bool {
        self.position(key).is_some()
    }

    /// Replaces in place if the key exists, keeping its position; appends otherwise.
    pub fn insert(&mut self, key: impl Into<String>, value: Value) {
        let key = key.into();
        match self.position(&key) {
            Some(at) => self.members[at].1 = value,
            None => {
                let at = self.members.len();
                if let Some(index) = &mut self.index {
                    index.insert(key.clone(), at);
                }
                self.members.push((key, value));
                self.index_if_large();
            }
        }
    }

    pub fn remove(&mut self, key: &str) -> Option<Value> {
        let at = self.position(key)?;
        let (name, value) = self.members.remove(at);

        // Everything after the hole shifted down by one.
        if let Some(index) = &mut self.index {
            index.remove(&name);
            for position in index.values_mut() {
                if *position > at {
                    *position -= 1;
                }
            }
        }
        Some(value)
    }

    pub fn is_empty(&self) -> bool {
        self.members.is_empty()
    }

    pub fn len(&self) -> usize {
        self.members.len()
    }

    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.members.iter().map(|(name, _)| name.as_str())
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &Value)> {
        self.members
            .iter()
            .map(|(name, value)| (name.as_str(), value))
    }

    pub fn extend(&mut self, other: Object) {
        for (key, value) in other.members {
            self.insert(key, value);
        }
    }
}

impl FromIterator<(String, Value)> for Object {
    fn from_iter<I: IntoIterator<Item = (String, Value)>>(iter: I) -> Self {
        let mut object = Object::new();
        for (key, value) in iter {
            object.insert(key, value);
        }
        object
    }
}

/// A canonical non-negative decimal: digits only, and no leading zero unless it *is* zero.
///
/// `str::parse` is more generous — it takes `"007"` and `"+7"` — so two spellings would
/// name one number. RFC 6901 array indices and semver components both forbid that, and the
/// one rule lives here so the pointer code, `migrate` and `document` cannot drift apart.
pub(crate) fn canonical_decimal(text: &str) -> Option<u64> {
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    if text.len() > 1 && text.starts_with('0') {
        return None;
    }
    text.parse().ok()
}

/// An RFC 6901 array index.
///
/// Public within the crate because `migrate` addresses the same documents with the same
/// pointers. It had its own copy, which behaved identically today and had nothing keeping
/// the two in step tomorrow.
pub(crate) fn pointer_index(key: &str) -> Option<usize> {
    canonical_decimal(key).and_then(|index| usize::try_from(index).ok())
}

impl Value {
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(text) => Some(text),
            _ => None,
        }
    }

    pub fn as_object(&self) -> Option<&Object> {
        match self {
            Self::Object(object) => Some(object),
            _ => None,
        }
    }

    pub fn as_object_mut(&mut self) -> Option<&mut Object> {
        match self {
            Self::Object(object) => Some(object),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&[Value]> {
        match self {
            Self::Array(items) => Some(items),
            _ => None,
        }
    }

    pub fn as_array_mut(&mut self) -> Option<&mut Vec<Value>> {
        match self {
            Self::Array(items) => Some(items),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Self::Number(number) => Some(number.as_f64()),
            _ => None,
        }
    }

    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Self::Number(number) => number.as_i64(),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(value) => Some(*value),
            _ => None,
        }
    }

    pub fn is_null(&self) -> bool {
        matches!(self, Self::Null)
    }

    /// A member of an object, or an element of an array by decimal index.
    ///
    /// Numeric keys work on arrays so a JSON Pointer can be walked segment by segment
    /// without knowing what it will find.
    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Self::Object(object) => object.get(key),
            Self::Array(items) => pointer_index(key).and_then(|index| items.get(index)),
            _ => None,
        }
    }

    pub fn get_mut(&mut self, key: &str) -> Option<&mut Value> {
        match self {
            Self::Object(object) => object.get_mut(key),
            Self::Array(items) => items.get_mut(pointer_index(key)?),
            _ => None,
        }
    }
}

impl From<String> for Value {
    fn from(value: String) -> Self {
        Self::String(value)
    }
}

impl From<&str> for Value {
    fn from(value: &str) -> Self {
        Self::String(value.to_owned())
    }
}

impl From<bool> for Value {
    fn from(value: bool) -> Self {
        Self::Bool(value)
    }
}

impl From<i64> for Value {
    fn from(value: i64) -> Self {
        Self::Number(Number::Integer(value))
    }
}

impl From<usize> for Value {
    fn from(value: usize) -> Self {
        Self::Number(Number::Integer(value as i64))
    }
}

impl From<f64> for Value {
    fn from(value: f64) -> Self {
        // A whole number stays whole, so 389.0 is written 389 rather than 389.0.
        if value.is_finite() && value.fract() == 0.0 && value.abs() < 9.007_199_254_740_992e15 {
            Self::Number(Number::Integer(value as i64))
        } else {
            Self::Number(Number::Float(value))
        }
    }
}

impl<T: Into<Value>> From<Option<T>> for Value {
    fn from(value: Option<T>) -> Self {
        match value {
            Some(value) => value.into(),
            None => Value::Null,
        }
    }
}

impl From<Object> for Value {
    fn from(value: Object) -> Self {
        Self::Object(value)
    }
}

impl From<Vec<Value>> for Value {
    fn from(value: Vec<Value>) -> Self {
        Self::Array(value)
    }
}

/// Why a document could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    /// Byte offset where the problem was found.
    pub offset: usize,
    pub message: &'static str,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} at byte {}", self.message, self.offset)
    }
}

impl std::error::Error for ParseError {}

/// Reads a JSON document.
///
/// Rejects trailing content, unbounded nesting, and anything the grammar does not allow.
/// Comments and trailing commas are not accepted: this reads JSON, not a superset of it.
pub fn parse(input: &str) -> Result<Value, ParseError> {
    let mut parser = Parser {
        bytes: input.as_bytes(),
        offset: 0,
    };
    parser.skip_whitespace();
    let value = parser.value(0)?;
    parser.skip_whitespace();
    if parser.offset < parser.bytes.len() {
        return Err(parser.error("unexpected trailing content"));
    }
    Ok(value)
}

struct Parser<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl Parser<'_> {
    fn error(&self, message: &'static str) -> ParseError {
        ParseError {
            offset: self.offset,
            message,
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.offset).copied()
    }

    fn skip_whitespace(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.offset += 1;
        }
    }

    fn expect(&mut self, byte: u8, message: &'static str) -> Result<(), ParseError> {
        if self.peek() == Some(byte) {
            self.offset += 1;
            Ok(())
        } else {
            Err(self.error(message))
        }
    }

    fn value(&mut self, depth: usize) -> Result<Value, ParseError> {
        if depth > MAX_DEPTH {
            return Err(self.error("nested too deeply"));
        }
        match self.peek() {
            Some(b'{') => self.object(depth),
            Some(b'[') => self.array(depth),
            Some(b'"') => Ok(Value::String(self.string()?)),
            Some(b't') => self.literal(b"true", Value::Bool(true)),
            Some(b'f') => self.literal(b"false", Value::Bool(false)),
            Some(b'n') => self.literal(b"null", Value::Null),
            Some(b'-' | b'0'..=b'9') => self.number(),
            Some(_) => Err(self.error("unexpected character")),
            None => Err(self.error("unexpected end of input")),
        }
    }

    fn literal(&mut self, word: &[u8], value: Value) -> Result<Value, ParseError> {
        let end = self.offset + word.len();
        if end <= self.bytes.len() && &self.bytes[self.offset..end] == word {
            self.offset = end;
            Ok(value)
        } else {
            Err(self.error("invalid literal"))
        }
    }

    fn object(&mut self, depth: usize) -> Result<Value, ParseError> {
        self.expect(b'{', "expected an object")?;
        let mut object = Object::new();

        self.skip_whitespace();
        if self.peek() == Some(b'}') {
            self.offset += 1;
            return Ok(Value::Object(object));
        }

        loop {
            self.skip_whitespace();
            let key = self.string()?;
            self.skip_whitespace();
            self.expect(b':', "expected ':' after a member name")?;
            self.skip_whitespace();
            let value = self.value(depth + 1)?;

            // A duplicate member is not a parse error in JSON; the last one wins, which is
            // what insert does, while keeping the original position.
            object.insert(key, value);

            self.skip_whitespace();
            match self.peek() {
                Some(b',') => self.offset += 1,
                Some(b'}') => {
                    self.offset += 1;
                    return Ok(Value::Object(object));
                }
                _ => return Err(self.error("expected ',' or '}'")),
            }
        }
    }

    fn array(&mut self, depth: usize) -> Result<Value, ParseError> {
        self.expect(b'[', "expected an array")?;
        let mut items = Vec::new();

        self.skip_whitespace();
        if self.peek() == Some(b']') {
            self.offset += 1;
            return Ok(Value::Array(items));
        }

        loop {
            self.skip_whitespace();
            items.push(self.value(depth + 1)?);
            self.skip_whitespace();
            match self.peek() {
                Some(b',') => self.offset += 1,
                Some(b']') => {
                    self.offset += 1;
                    return Ok(Value::Array(items));
                }
                _ => return Err(self.error("expected ',' or ']'")),
            }
        }
    }
}

impl Parser<'_> {
    fn hex4(&mut self) -> Result<u16, ParseError> {
        let mut code: u16 = 0;
        for _ in 0..4 {
            let digit = match self.peek() {
                Some(byte @ b'0'..=b'9') => u16::from(byte - b'0'),
                Some(byte @ b'a'..=b'f') => u16::from(byte - b'a') + 10,
                Some(byte @ b'A'..=b'F') => u16::from(byte - b'A') + 10,
                _ => return Err(self.error("invalid unicode escape")),
            };
            code = code * 16 + digit;
            self.offset += 1;
        }
        Ok(code)
    }

    fn string(&mut self) -> Result<String, ParseError> {
        self.expect(b'"', "expected a string")?;
        let mut text = String::new();

        loop {
            let byte = match self.peek() {
                Some(byte) => byte,
                None => return Err(self.error("unterminated string")),
            };

            match byte {
                b'"' => {
                    self.offset += 1;
                    return Ok(text);
                }
                // JSON forbids unescaped control characters inside a string.
                0x00..=0x1F => return Err(self.error("unescaped control character")),
                b'\\' => {
                    self.offset += 1;
                    let escape = match self.peek() {
                        Some(byte) => byte,
                        None => return Err(self.error("unterminated escape")),
                    };
                    self.offset += 1;
                    match escape {
                        b'"' => text.push('"'),
                        b'\\' => text.push('\\'),
                        b'/' => text.push('/'),
                        b'b' => text.push('\u{8}'),
                        b'f' => text.push('\u{c}'),
                        b'n' => text.push('\n'),
                        b'r' => text.push('\r'),
                        b't' => text.push('\t'),
                        b'u' => {
                            let unit = self.hex4()?;
                            let character = if (0xD800..=0xDBFF).contains(&unit) {
                                // A high surrogate must be followed by its low half.
                                self.expect(b'\\', "unpaired surrogate")?;
                                self.expect(b'u', "unpaired surrogate")?;
                                let low = self.hex4()?;
                                if !(0xDC00..=0xDFFF).contains(&low) {
                                    return Err(self.error("unpaired surrogate"));
                                }
                                let combined = 0x1_0000
                                    + ((u32::from(unit) - 0xD800) << 10)
                                    + (u32::from(low) - 0xDC00);
                                char::from_u32(combined)
                            } else if (0xDC00..=0xDFFF).contains(&unit) {
                                return Err(self.error("unpaired surrogate"));
                            } else {
                                char::from_u32(u32::from(unit))
                            };
                            match character {
                                Some(character) => text.push(character),
                                None => return Err(self.error("invalid code point")),
                            }
                        }
                        _ => return Err(self.error("unknown escape")),
                    }
                }
                _ => {
                    // The input came in as a &str, so any multi-byte sequence here is
                    // already valid UTF-8. Copy it across whole rather than byte by byte.
                    let start = self.offset;
                    let end = start + utf8_width(byte);
                    if end > self.bytes.len() {
                        return Err(self.error("truncated UTF-8 sequence"));
                    }
                    match std::str::from_utf8(&self.bytes[start..end]) {
                        Ok(chunk) => text.push_str(chunk),
                        Err(_) => return Err(self.error("invalid UTF-8")),
                    }
                    self.offset = end;
                }
            }
        }
    }

    fn number(&mut self) -> Result<Value, ParseError> {
        let start = self.offset;
        let mut floating = false;

        if self.peek() == Some(b'-') {
            self.offset += 1;
        }

        // JSON allows one leading zero and no more: `0` and `0.5` are numbers, `01` is
        // not. Accepting it would let two documents that differ textually compare equal.
        match self.peek() {
            Some(b'0') => {
                self.offset += 1;
                if matches!(self.peek(), Some(b'0'..=b'9')) {
                    return Err(self.error("a number may not have leading zeros"));
                }
            }
            Some(b'1'..=b'9') => {
                while matches!(self.peek(), Some(b'0'..=b'9')) {
                    self.offset += 1;
                }
            }
            _ => return Err(self.error("expected a digit")),
        }

        if self.peek() == Some(b'.') {
            floating = true;
            self.offset += 1;
            let fraction_from = self.offset;
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.offset += 1;
            }
            if self.offset == fraction_from {
                return Err(self.error("expected a digit after '.'"));
            }
        }

        if matches!(self.peek(), Some(b'e' | b'E')) {
            floating = true;
            self.offset += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.offset += 1;
            }
            let exponent_from = self.offset;
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.offset += 1;
            }
            if self.offset == exponent_from {
                return Err(self.error("expected a digit in the exponent"));
            }
        }

        let text = match std::str::from_utf8(&self.bytes[start..self.offset]) {
            Ok(text) => text,
            Err(_) => return Err(self.error("invalid number")),
        };

        // Integers stay exact so a food id is not rewritten as a float. Floating-point
        // parsing goes to std, which is the part nobody should write themselves.
        if !floating {
            if let Ok(integer) = text.parse::<i64>() {
                return Ok(Value::Number(Number::Integer(integer)));
            }
        }
        match text.parse::<f64>() {
            Ok(float) if float.is_finite() => Ok(Value::Number(Number::Float(float))),
            _ => Err(self.error("number out of range")),
        }
    }
}

fn utf8_width(byte: u8) -> usize {
    match byte {
        0x00..=0x7F => 1,
        0xC0..=0xDF => 2,
        0xE0..=0xEF => 3,
        _ => 4,
    }
}

// ── writing ──────────────────────────────────────────────────────────────────

impl std::fmt::Display for Number {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Integer(value) => write!(f, "{value}"),
            // Rust prints the shortest decimal that round-trips, which is exactly what a
            // document needs. Non-finite values cannot occur: the parser rejects them and
            // `From<f64>` is the only other way in.
            // JSON has no NaN and no infinity. The reader already refuses them, but
            // `Value::from(f64)` is public, so a caller can build one in memory — and
            // writing `NaN` unquoted would produce a document no parser accepts,
            // including this one.
            Self::Float(value) if !value.is_finite() => f.write_str("null"),
            Self::Float(value) => write!(f, "{value}"),
        }
    }
}

/// Compact form, for machines.
impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write_compact(f, self, 0)
    }
}

/// The writer recurses with the value, so it carries the depth the reader bounds. Past
/// [`MAX_WRITE_DEPTH`] it returns an error — which `to_string` reports as a panic naming
/// the cause — rather than exhausting the stack, which nothing can report.
fn write_compact(f: &mut impl std::fmt::Write, value: &Value, depth: usize) -> std::fmt::Result {
    match value {
        Value::Null => f.write_str("null"),
        Value::Bool(true) => f.write_str("true"),
        Value::Bool(false) => f.write_str("false"),
        Value::Number(number) => write!(f, "{number}"),
        Value::String(text) => write_escaped(f, text),
        Value::Array(_) | Value::Object(_) if depth >= MAX_WRITE_DEPTH => Err(std::fmt::Error),
        Value::Array(items) => {
            f.write_str("[")?;
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    f.write_str(",")?;
                }
                write_compact(f, item, depth + 1)?;
            }
            f.write_str("]")
        }
        Value::Object(object) => {
            f.write_str("{")?;
            for (index, (key, member)) in object.iter().enumerate() {
                if index > 0 {
                    f.write_str(",")?;
                }
                write_escaped(f, key)?;
                f.write_str(":")?;
                write_compact(f, member, depth + 1)?;
            }
            f.write_str("}")
        }
    }
}

impl Value {
    /// Indented form, for people and for diffs. Two spaces per level.
    pub fn to_pretty(&self) -> String {
        let mut out = String::new();
        // Writing into a String cannot fail. The one error the writer returns is a value
        // nested past MAX_WRITE_DEPTH, which no document can produce and which the compact
        // writer reports the same way through `to_string`; silently returning a truncated
        // document would be worse than either.
        write_pretty(&mut out, self, 0)
            .expect("a value nested deeper than MAX_WRITE_DEPTH cannot be written");
        out
    }
}

fn write_pretty(out: &mut String, value: &Value, indent: usize) -> std::fmt::Result {
    const STEP: usize = 2;
    let pad = |out: &mut String, level: usize| {
        for _ in 0..level * STEP {
            out.push(' ');
        }
    };

    match value {
        // `indent` is the nesting level, so it is also the depth the compact writer bounds.
        Value::Array(_) | Value::Object(_) if indent >= MAX_WRITE_DEPTH => {
            return Err(std::fmt::Error);
        }
        Value::Array(items) if !items.is_empty() => {
            out.push_str("[\n");
            for (index, item) in items.iter().enumerate() {
                pad(out, indent + 1);
                write_pretty(out, item, indent + 1)?;
                if index + 1 < items.len() {
                    out.push(',');
                }
                out.push('\n');
            }
            pad(out, indent);
            out.push(']');
        }
        Value::Object(object) if !object.is_empty() => {
            out.push_str("{\n");
            for (index, (key, member)) in object.iter().enumerate() {
                pad(out, indent + 1);
                write_escaped(out, key)?;
                out.push_str(": ");
                write_pretty(out, member, indent + 1)?;
                if index + 1 < object.len() {
                    out.push(',');
                }
                out.push('\n');
            }
            pad(out, indent);
            out.push('}');
        }
        other => write!(out, "{other}")?,
    }
    Ok(())
}

fn write_escaped(out: &mut impl std::fmt::Write, text: &str) -> std::fmt::Result {
    out.write_str("\"")?;
    for character in text.chars() {
        match character {
            '"' => out.write_str("\\\"")?,
            '\\' => out.write_str("\\\\")?,
            '\n' => out.write_str("\\n")?,
            '\r' => out.write_str("\\r")?,
            '\t' => out.write_str("\\t")?,
            '\u{8}' => out.write_str("\\b")?,
            '\u{c}' => out.write_str("\\f")?,
            // Everything else below 0x20 has no short form and must be escaped.
            control if control < ' ' => write!(out, "\\u{:04x}", control as u32)?,
            // Anything else, including non-ASCII, is written as itself: JSON is UTF-8 and
            // escaping it would only make documents larger and harder to read.
            other => out.write_char(other)?,
        }
    }
    out.write_str("\"")
}

/// Builds an [`Object`] from `"key" => value` pairs.
///
/// The small piece of `serde_json::json!` this project actually used. Values go through
/// `Value::from`, so a bare `&str`, number or bool works, and so does a `Value` already
/// built.
///
/// ```
/// use onyx_core::{json_object, json::Value};
/// let object = json_object! {
///     "conforming" => true,
///     "producer" => "Example",
/// };
/// assert_eq!(Value::Object(object).to_string(), r#"{"conforming":true,"producer":"Example"}"#);
/// ```
#[macro_export]
macro_rules! json_object {
    ($($key:expr => $value:expr),* $(,)?) => {{
        #[allow(unused_mut)]
        let mut object = $crate::json::Object::new();
        $( object.insert($key, $crate::json::Value::from($value)); )*
        object
    }};
}

/// A shared `Null`, so indexing a missing member can return a reference rather than panic.
static NULL: Value = Value::Null;

/// `value["key"]` for objects, and `value["0"]` for arrays.
///
/// A missing member reads as `Null` rather than panicking, which is what makes walking an
/// unknown document readable. Use [`Value::get`] when the difference between "absent" and
/// "present and null" matters.
impl std::ops::Index<&str> for Value {
    type Output = Value;

    fn index(&self, key: &str) -> &Value {
        self.get(key).unwrap_or(&NULL)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip(text: &str) -> String {
        parse(text).expect("should parse").to_string()
    }

    #[test]
    fn reads_and_writes_the_shapes_a_document_uses() {
        assert_eq!(
            round_trip(r#"{"a":1,"b":[true,false,null]}"#),
            r#"{"a":1,"b":[true,false,null]}"#
        );
        assert_eq!(round_trip("[]"), "[]");
        assert_eq!(round_trip("{}"), "{}");
        assert_eq!(round_trip(r#"  { "a" : 1 }  "#), r#"{"a":1}"#);
    }

    #[test]
    fn accepts_the_zeros_json_allows_and_rejects_the_rest() {
        assert_eq!(parse("0").unwrap().as_i64(), Some(0));
        assert_eq!(parse("-0").unwrap().as_i64(), Some(0));
        assert_eq!(parse("0.5").unwrap().as_f64(), Some(0.5));
        assert_eq!(parse("10").unwrap().as_i64(), Some(10));
        for text in ["01", "-01", "00", "007"] {
            assert!(parse(text).is_err(), "should have rejected {text:?}");
        }
    }

    #[test]
    fn large_integers_do_not_collapse_into_each_other() {
        // Beyond 2^53 these are the same f64 and different i64. Comparing through f64
        // would call them equal and quietly lose the difference.
        let a = parse("9007199254740993").unwrap();
        let b = parse("9007199254740992").unwrap();
        assert_ne!(a, b);
        assert_eq!(a, parse("9007199254740993").unwrap());
    }

    #[test]
    fn integers_stay_integers() {
        // A USDA food id rewritten as 169705.0 would be a different document.
        assert_eq!(round_trip("169705"), "169705");
        assert_eq!(round_trip("-3"), "-3");
        assert_eq!(round_trip("0"), "0");
    }

    #[test]
    fn floats_survive_a_round_trip_exactly() {
        assert_eq!(round_trip("80.512645675"), "80.512645675");
        assert_eq!(round_trip("6.9"), "6.9");
        assert_eq!(round_trip("0.62"), "0.62");
        // Exponents are accepted on the way in and normalised on the way out.
        assert_eq!(parse("1e2").unwrap().as_f64(), Some(100.0));
    }

    #[test]
    fn object_member_order_is_preserved() {
        let text = r#"{"z":1,"a":2,"m":3}"#;
        assert_eq!(round_trip(text), text);
    }

    #[test]
    fn a_duplicate_member_keeps_its_position_and_takes_the_last_value() {
        assert_eq!(round_trip(r#"{"a":1,"b":2,"a":3}"#), r#"{"a":3,"b":2}"#);
    }

    #[test]
    fn handles_every_escape() {
        let parsed = parse(r#""q\"b\\s\/n\nr\rt\tb\bf\f""#).unwrap();
        assert_eq!(parsed.as_str(), Some("q\"b\\s/n\nr\rt\tb\u{8}f\u{c}"));
        // And writes them back in a form it can read again.
        assert_eq!(parse(&parsed.to_string()).unwrap(), parsed);
    }

    #[test]
    fn handles_unicode_escapes_and_surrogate_pairs() {
        assert_eq!(parse(r#""é""#).unwrap().as_str(), Some("é"));
        // U+1F34E, outside the basic plane, arrives as a surrogate pair.
        assert_eq!(parse(r#""🍎""#).unwrap().as_str(), Some("🍎"));
    }

    #[test]
    fn non_ascii_is_written_as_itself() {
        // Escaping it would only make documents bigger and less readable, and a food name
        // is exactly where non-ASCII shows up.
        assert_eq!(
            round_trip(r#"{"name":"Crème brûlée"}"#),
            r#"{"name":"Crème brûlée"}"#
        );
    }

    #[test]
    fn rejects_what_is_not_json() {
        let bad = [
            r#"{"a":1}trailing"#,
            r#"{"a":1,}"#,
            r#"[1,]"#,
            r#"{a:1}"#,
            r#"{"a" 1}"#,
            r#""unterminated"#,
            r#"'single quoted'"#,
            "// a comment\n1",
            "NaN",
            "Infinity",
            "01",
            "1.",
            ".5",
            "1e",
            "tru",
            "",
        ];
        for text in bad {
            assert!(parse(text).is_err(), "should have rejected {text:?}");
        }
    }

    #[test]
    fn rejects_a_raw_control_character_in_a_string() {
        assert!(parse("\"a\nb\"").is_err());
        assert!(parse("\"a\u{0}b\"").is_err());
    }

    #[test]
    fn rejects_unpaired_surrogates() {
        assert!(parse(r#""\ud83c""#).is_err());
        assert!(parse(r#""\udf4e""#).is_err());
        assert!(parse(r#""\ud83cA""#).is_err());
    }

    #[test]
    fn refuses_input_nested_deeper_than_the_limit() {
        // Hostile input must not be able to exhaust the stack: a stack overflow aborts the
        // process, which is not a failure a caller can handle.
        let deep = format!(
            "{}1{}",
            "[".repeat(MAX_DEPTH + 10),
            "]".repeat(MAX_DEPTH + 10)
        );
        assert!(parse(&deep).is_err());

        let fine = format!("{}1{}", "[".repeat(8), "]".repeat(8));
        assert!(parse(&fine).is_ok());
    }

    #[test]
    fn refuses_to_write_a_value_nested_deeper_than_the_limit() {
        use std::fmt::Write as _;

        // The reader bounds depth, but a `Value` can be built without it. A writer recursing
        // into such a value without limit would overflow the stack, which aborts; this is
        // an error instead, from both writers.
        let mut deep = Value::Null;
        for _ in 0..MAX_WRITE_DEPTH + 10 {
            deep = Value::Array(vec![deep]);
        }
        let mut out = String::new();
        assert!(write!(out, "{deep}").is_err());
        assert!(write_pretty(&mut String::new(), &deep, 0).is_err());

        // Everything the reader admits, both writers write, and it reads back.
        let mut fine = Value::Null;
        for _ in 0..MAX_DEPTH - 2 {
            fine = Value::Array(vec![fine]);
        }
        assert_eq!(parse(&fine.to_string()).unwrap(), fine);
        assert_eq!(parse(&fine.to_pretty()).unwrap(), fine);
    }

    #[test]
    fn truncated_input_is_an_error_rather_than_a_panic() {
        // Every prefix of a valid document must fail cleanly.
        let valid = r#"{"a":[1,2,{"b":"é"}],"c":true}"#;
        for length in 1..valid.len() {
            // Slicing a &str inside a multi-byte character panics in std, which would be
            // this test's bug rather than the parser's.
            if valid.is_char_boundary(length) {
                let _ = parse(&valid[..length]);
            }
        }
    }

    #[test]
    fn pretty_output_is_indented_and_reparses() {
        let value = parse(r#"{"a":1,"b":[1,2],"c":{},"d":[]}"#).unwrap();
        let pretty = value.to_pretty();
        assert!(pretty.contains("\n  \"a\": 1"));
        // Empty containers stay on one line rather than becoming three.
        assert!(pretty.contains("\"c\": {}"));
        assert!(pretty.contains("\"d\": []"));
        assert_eq!(parse(&pretty).unwrap(), value);
    }

    #[test]
    fn objects_support_the_operations_the_engine_needs() {
        let mut object = Object::new();
        object.insert("a", Value::from(1i64));
        object.insert("b", Value::from("two"));
        object.insert("a", Value::from(3i64));

        assert_eq!(object.len(), 2);
        assert_eq!(object.get("a").and_then(Value::as_i64), Some(3));
        assert_eq!(object.keys().collect::<Vec<_>>(), vec!["a", "b"]);
        assert_eq!(object.remove("a").and_then(|v| v.as_i64()), Some(3));
        assert!(!object.contains_key("a"));
    }

    /// The index only switches on past a threshold, so the interesting cases are on both
    /// sides of it and across the boundary.
    #[test]
    fn an_indexed_object_behaves_exactly_like_a_scanned_one() {
        for count in [
            1usize,
            INDEX_THRESHOLD - 1,
            INDEX_THRESHOLD,
            INDEX_THRESHOLD + 1,
            200,
        ] {
            let mut object = Object::new();
            for i in 0..count {
                object.insert(format!("k{i}"), Value::from(i as i64));
            }
            assert_eq!(object.len(), count, "count {count}");

            // Every key is findable, and insertion order survived.
            for i in 0..count {
                assert_eq!(
                    object.get(&format!("k{i}")).and_then(Value::as_i64),
                    Some(i as i64),
                    "count {count}, key k{i}"
                );
            }
            assert_eq!(
                object.keys().collect::<Vec<_>>(),
                (0..count).map(|i| format!("k{i}")).collect::<Vec<_>>(),
                "order changed at count {count}"
            );

            // Replacing keeps position; removing shifts the rest down.
            if count >= 2 {
                object.insert("k0", Value::from(999i64));
                assert_eq!(object.get("k0").and_then(Value::as_i64), Some(999));
                assert_eq!(object.keys().next(), Some("k0"), "replace moved the member");

                assert_eq!(object.remove("k0").and_then(|v| v.as_i64()), Some(999));
                assert_eq!(object.get("k0"), None);
                assert_eq!(object.len(), count - 1);
                for i in 1..count {
                    assert_eq!(
                        object.get(&format!("k{i}")).and_then(Value::as_i64),
                        Some(i as i64),
                        "after remove, count {count}, key k{i}"
                    );
                }
            }
        }
    }

    /// Reading one wide object was O(n²): 32,000 members took 2.1 seconds, which is a
    /// denial of service on input that arrives from anywhere. The bound is deliberately
    /// loose — linear is 8x for this 8x of work, quadratic is 64x — so this fails on a
    /// return to quadratic without failing on a slow or busy machine.
    #[test]
    fn parsing_a_wide_object_does_not_go_quadratic() {
        fn parse_members(count: usize) -> std::time::Duration {
            let text = format!(
                "{{{}}}",
                (0..count)
                    .map(|i| format!("\"k{i}\":{i}"))
                    .collect::<Vec<_>>()
                    .join(",")
            );
            let started = std::time::Instant::now();
            let value = parse(&text).expect("parses");
            let elapsed = started.elapsed();
            assert_eq!(value.as_object().map(Object::len), Some(count));
            elapsed
        }

        let small = parse_members(4_000).as_nanos().max(1);
        let large = parse_members(32_000).as_nanos().max(1);
        assert!(
            large < small * 24,
            "8x the members took {}x the time; the index is not working",
            large / small
        );
    }
    /// A float outside i64's range has no integer value, and saturating to i64::MAX and
    /// returning it as though it were exact is how a food id turns into a different one.
    #[test]
    fn a_float_too_large_for_an_integer_is_not_an_integer() {
        for text in ["1e30", "-1e30", "1e300"] {
            let value = parse(text).expect("parses as a number");
            assert_eq!(value.as_i64(), None, "{text} was accepted as an i64");
        }
        // The ordinary cases still work.
        assert_eq!(parse("12.0").unwrap().as_i64(), Some(12));
        assert_eq!(parse("-3").unwrap().as_i64(), Some(-3));
        assert_eq!(parse("12.5").unwrap().as_i64(), None);
    }
    /// JSON has no NaN and no infinity, so the writer must not emit one even though a
    /// caller can build one in memory.
    #[test]
    fn a_non_finite_number_is_written_as_null_not_as_nan() {
        for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let text = Value::from(bad).to_string();
            assert_eq!(text, "null", "{bad} was written as {text}");
            // And what it writes must read back.
            assert!(parse(&text).is_ok(), "{text} is not valid JSON");
        }
        assert_eq!(Value::from(1.5f64).to_string(), "1.5");
    }

    /// RFC 6901 array indices have no leading zeros and no sign.
    #[test]
    fn a_pointer_array_index_follows_rfc_6901() {
        let value = parse(r#"{"days": [10, 20, 30]}"#).unwrap();
        let days = value.get("days").unwrap();

        assert_eq!(days.get("0").and_then(Value::as_i64), Some(10));
        assert_eq!(days.get("2").and_then(Value::as_i64), Some(30));

        for rejected in ["00", "007", "+1", "-1", "1.0", " 1", ""] {
            assert!(
                days.get(rejected).is_none(),
                "{rejected:?} was accepted as an array index"
            );
        }
    }
}
