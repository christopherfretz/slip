//! A small JSON reader and writer (`spec/model.md` §4). Work package WP-002.
//!
//! The reader accepts exactly RFC 8259 JSON in UTF-8 (with an optional
//! byte-order mark) and rejects everything else with the byte offset at
//! which the problem was found. The writer is deterministic and has two
//! forms, [`write_compact`] and [`write_pretty`]; reading its output gives
//! back an equal value.
//!
//! The details of the reader and writer are `spec/model.md` §4.1–§4.3.

use std::collections::HashSet;
use std::fmt;

/// The deepest nesting of arrays and objects the reader accepts and the
/// writer produces (§4: "nesting deeper than 128" is rejected). A top-level
/// array or object is at depth 1.
pub const MAX_DEPTH: usize = 128;

/// The smallest value an [`Value::Int`] may hold: −2^63.
pub const INT_MIN: i128 = -(1 << 63);
/// One past the largest value an [`Value::Int`] may hold: 2^64.
pub const INT_END: i128 = 1 << 64;

/// A JSON value (§4 "Value model").
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    /// An integer in −2^63..2^64. The reader never produces a value outside
    /// that range; the writer rejects one.
    Int(i128),
    /// Any number that is not an `Int`.
    Float(f64),
    Str(String),
    Array(Vec<Value>),
    /// Members in input order. The reader never produces duplicate keys.
    Object(Vec<(String, Value)>),
}

impl Value {
    /// The value of the first member named `key`, if `self` is an object.
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.as_object()?.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&[Value]> {
        match self {
            Value::Array(a) => Some(a),
            _ => None,
        }
    }

    pub fn as_object(&self) -> Option<&[(String, Value)]> {
        match self {
            Value::Object(o) => Some(o),
            _ => None,
        }
    }

    /// The integer, if `self` is an `Int` that fits `u64`. A `Float` is
    /// never converted, even when it is integral.
    pub fn as_u64(&self) -> Option<u64> {
        match self {
            Value::Int(i) => u64::try_from(*i).ok(),
            _ => None,
        }
    }

    /// The integer, if `self` is an `Int` that fits `i64`. A `Float` is
    /// never converted, even when it is integral.
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Value::Int(i) => i64::try_from(*i).ok(),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }
}

/// A reader error: where it was detected and why.
///
/// `offset` is a byte offset into the input given to [`parse`] (a byte-order
/// mark counts). `line` and `column` are 1-based and derived from `offset`:
/// `line` is one plus the number of `\n` bytes before it, `column` is one
/// plus the number of bytes between the last such `\n` (or the start) and it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    pub offset: usize,
    pub line: usize,
    pub column: usize,
    pub message: String,
}

impl Error {
    fn at(bytes: &[u8], offset: usize, message: impl Into<String>) -> Error {
        let before = &bytes[..offset.min(bytes.len())];
        // n line breaks split the prefix into n + 1 pieces.
        let line = before.split(|&b| b == b'\n').count();
        let line_start = before.iter().rposition(|&b| b == b'\n').map_or(0, |p| p + 1);
        Error { offset, line, column: offset - line_start + 1, message: message.into() }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{} (byte {}): {}", self.line, self.column, self.offset, self.message)
    }
}

impl std::error::Error for Error {}

/// A writer error: the value has no JSON form under §4.
#[derive(Debug, Clone, PartialEq)]
pub enum WriteError {
    /// A `Float` that is NaN or infinite.
    NonFiniteFloat(f64),
    /// An `Int` outside −2^63..2^64.
    IntOutOfRange(i128),
    /// Arrays and objects nested deeper than [`MAX_DEPTH`]: the reader would
    /// reject the output.
    TooDeep,
}

impl fmt::Display for WriteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WriteError::NonFiniteFloat(x) => write!(f, "non-finite number {x} cannot be written"),
            WriteError::IntOutOfRange(i) => {
                write!(f, "integer {i} is outside -2^63..2^64 and cannot be written")
            }
            WriteError::TooDeep => write!(f, "nesting deeper than {MAX_DEPTH} cannot be written"),
        }
    }
}

impl std::error::Error for WriteError {}

// ---------------------------------------------------------------------------
// Reader
// ---------------------------------------------------------------------------

/// Parses one JSON text (§4 "Reader").
pub fn parse(bytes: &[u8]) -> Result<Value, Error> {
    let text = match std::str::from_utf8(bytes) {
        Ok(t) => t,
        Err(e) => return Err(Error::at(bytes, e.valid_up_to(), "invalid UTF-8")),
    };
    let start = if bytes.starts_with(b"\xEF\xBB\xBF") { 3 } else { 0 };
    let mut p = Parser { bytes, text, pos: start };
    p.skip_ws();
    let value = p.value(0)?;
    p.skip_ws();
    match p.peek() {
        None => {}
        Some(b'/') => return Err(p.err("comments are not allowed")),
        Some(_) => return Err(p.err("unexpected content after the top-level value")),
    }
    Ok(value)
}

struct Parser<'a> {
    bytes: &'a [u8],
    /// The same input; valid UTF-8, so slices between ASCII delimiters are
    /// on character boundaries.
    text: &'a str,
    pos: usize,
}

impl Parser<'_> {
    fn err(&self, message: impl Into<String>) -> Error {
        Error::at(self.bytes, self.pos, message)
    }

    fn err_at(&self, offset: usize, message: impl Into<String>) -> Error {
        Error::at(self.bytes, offset, message)
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn skip_ws(&mut self) {
        while let Some(b' ' | b'\t' | b'\n' | b'\r') = self.peek() {
            self.pos += 1;
        }
    }

    /// An error for the byte at `pos`, or for the end of input.
    fn unexpected(&self, expected: &str) -> Error {
        match self.peek() {
            None => self.err(format!("unexpected end of input, expected {expected}")),
            Some(b'/') => self.err("comments are not allowed"),
            Some(b'\'') => self.err("single-quoted strings are not allowed"),
            Some(b) if b.is_ascii_graphic() => {
                self.err(format!("unexpected '{}', expected {expected}", b as char))
            }
            Some(_) => {
                let c = self.text[self.pos..].chars().next().unwrap_or('\u{fffd}');
                self.err(format!("unexpected character U+{:04X}, expected {expected}", c as u32))
            }
        }
    }

    fn value(&mut self, depth: usize) -> Result<Value, Error> {
        match self.peek() {
            Some(b'{') => self.object(depth + 1),
            Some(b'[') => self.array(depth + 1),
            Some(b'"') => Ok(Value::Str(self.string()?)),
            Some(b'-' | b'0'..=b'9') => self.number(),
            Some(b't') => self.literal("true", Value::Bool(true)),
            Some(b'f') => self.literal("false", Value::Bool(false)),
            Some(b'n') => self.literal("null", Value::Null),
            _ => Err(self.unexpected("a value")),
        }
    }

    fn literal(&mut self, word: &str, value: Value) -> Result<Value, Error> {
        if self.bytes[self.pos..].starts_with(word.as_bytes()) {
            self.pos += word.len();
            Ok(value)
        } else {
            Err(self.unexpected("a value"))
        }
    }

    fn enter(&self, depth: usize) -> Result<(), Error> {
        if depth > MAX_DEPTH {
            Err(self.err(format!("nesting deeper than {MAX_DEPTH}")))
        } else {
            Ok(())
        }
    }

    fn array(&mut self, depth: usize) -> Result<Value, Error> {
        self.enter(depth)?;
        self.pos += 1; // '['
        let mut items = Vec::new();
        self.skip_ws();
        if self.peek() == Some(b']') {
            self.pos += 1;
            return Ok(Value::Array(items));
        }
        loop {
            self.skip_ws();
            if self.peek() == Some(b']') {
                return Err(self.err("trailing comma in array"));
            }
            items.push(self.value(depth)?);
            self.skip_ws();
            match self.peek() {
                Some(b',') => self.pos += 1,
                Some(b']') => {
                    self.pos += 1;
                    return Ok(Value::Array(items));
                }
                _ => return Err(self.unexpected("',' or ']'")),
            }
        }
    }

    fn object(&mut self, depth: usize) -> Result<Value, Error> {
        /// Below this many members, duplicates are found by a linear scan.
        const SCAN_LIMIT: usize = 16;
        self.enter(depth)?;
        self.pos += 1; // '{'
        let mut members: Vec<(String, Value)> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();
        self.skip_ws();
        if self.peek() == Some(b'}') {
            self.pos += 1;
            return Ok(Value::Object(members));
        }
        loop {
            self.skip_ws();
            match self.peek() {
                Some(b'"') => {}
                Some(b'}') => return Err(self.err("trailing comma in object")),
                _ => return Err(self.unexpected("a string key")),
            }
            let key_at = self.pos;
            let key = self.string()?;
            let duplicate = if members.len() < SCAN_LIMIT {
                members.iter().any(|(k, _)| *k == key)
            } else {
                if seen.is_empty() {
                    seen.extend(members.iter().map(|(k, _)| k.clone()));
                }
                !seen.insert(key.clone())
            };
            if duplicate {
                return Err(self.err_at(key_at, format!("duplicate key {key:?}")));
            }
            self.skip_ws();
            if self.peek() != Some(b':') {
                return Err(self.unexpected("':'"));
            }
            self.pos += 1;
            self.skip_ws();
            let value = self.value(depth)?;
            members.push((key, value));
            self.skip_ws();
            match self.peek() {
                Some(b',') => self.pos += 1,
                Some(b'}') => {
                    self.pos += 1;
                    return Ok(Value::Object(members));
                }
                _ => return Err(self.unexpected("',' or '}'")),
            }
        }
    }

    /// Reads a string starting at the opening quote.
    fn string(&mut self) -> Result<String, Error> {
        self.pos += 1; // '"'
        let mut out = String::new();
        let mut run = self.pos;
        loop {
            match self.peek() {
                None => return Err(self.err("unterminated string")),
                Some(b'"') => {
                    out.push_str(&self.text[run..self.pos]);
                    self.pos += 1;
                    return Ok(out);
                }
                Some(b'\\') => {
                    out.push_str(&self.text[run..self.pos]);
                    self.escape(&mut out)?;
                    run = self.pos;
                }
                Some(b) if b < 0x20 => {
                    return Err(self
                        .err(format!("control character U+{b:04X} must be escaped in a string")));
                }
                Some(_) => self.pos += 1,
            }
        }
    }

    /// Reads one escape starting at the backslash and appends its character.
    fn escape(&mut self, out: &mut String) -> Result<(), Error> {
        let at = self.pos;
        let c = match self.bytes.get(at + 1) {
            Some(b'"') => '"',
            Some(b'\\') => '\\',
            Some(b'/') => '/',
            Some(b'b') => '\u{8}',
            Some(b'f') => '\u{c}',
            Some(b'n') => '\n',
            Some(b'r') => '\r',
            Some(b't') => '\t',
            Some(b'u') => {
                let unit = self.hex4(at)?;
                self.pos = at + 6;
                let c = match unit {
                    0xD800..=0xDBFF => {
                        let next = self.pos;
                        if self.bytes.get(next) != Some(&b'\\')
                            || self.bytes.get(next + 1) != Some(&b'u')
                        {
                            return Err(self.err_at(at, "unpaired UTF-16 surrogate escape"));
                        }
                        let low = self.hex4(next)?;
                        if !(0xDC00..=0xDFFF).contains(&low) {
                            return Err(self.err_at(at, "unpaired UTF-16 surrogate escape"));
                        }
                        self.pos = next + 6;
                        let scalar = 0x10000 + ((unit - 0xD800) << 10) + (low - 0xDC00);
                        char::from_u32(scalar)
                    }
                    0xDC00..=0xDFFF => {
                        return Err(self.err_at(at, "unpaired UTF-16 surrogate escape"));
                    }
                    _ => char::from_u32(unit),
                };
                let Some(c) = c else {
                    unreachable!("a non-surrogate or combined code point is a char")
                };
                out.push(c);
                return Ok(());
            }
            None => return Err(self.err_at(at, "unterminated string")),
            Some(_) => return Err(self.err_at(at, "invalid escape in string")),
        };
        out.push(c);
        self.pos = at + 2;
        Ok(())
    }

    /// The four hexadecimal digits of the `\u` escape whose backslash is at
    /// `at`.
    fn hex4(&self, at: usize) -> Result<u32, Error> {
        let mut unit = 0u32;
        for i in 0..4 {
            let digit = self.bytes.get(at + 2 + i).and_then(|&b| (b as char).to_digit(16));
            match digit {
                Some(d) => unit = unit * 16 + d,
                None => {
                    return Err(self.err_at(at, "\\u escape needs four hexadecimal digits"));
                }
            }
        }
        Ok(unit)
    }

    fn digits(&mut self) -> usize {
        let start = self.pos;
        while let Some(b'0'..=b'9') = self.peek() {
            self.pos += 1;
        }
        self.pos - start
    }

    fn number(&mut self) -> Result<Value, Error> {
        let start = self.pos;
        if self.peek() == Some(b'-') {
            self.pos += 1;
        }
        let int_start = self.pos;
        match self.peek() {
            Some(b'0') => {
                self.pos += 1;
                if let Some(b'0'..=b'9') = self.peek() {
                    return Err(self.err_at(int_start, "leading zeros are not allowed"));
                }
            }
            Some(b'1'..=b'9') => {
                self.digits();
            }
            _ => return Err(self.unexpected("a digit")),
        }
        let mut integral = true;
        if self.peek() == Some(b'.') {
            integral = false;
            self.pos += 1;
            if self.digits() == 0 {
                return Err(self.unexpected("a digit after '.'"));
            }
        }
        if let Some(b'e' | b'E') = self.peek() {
            integral = false;
            self.pos += 1;
            if let Some(b'+' | b'-') = self.peek() {
                self.pos += 1;
            }
            if self.digits() == 0 {
                return Err(self.unexpected("a digit in the exponent"));
            }
        }
        let literal = &self.text[start..self.pos];
        // The parse fails only on i128 overflow, which is outside Int anyway.
        if integral
            && let Ok(i) = literal.parse::<i128>()
            && (INT_MIN..INT_END).contains(&i)
        {
            return Ok(Value::Int(i));
        }
        match literal.parse::<f64>() {
            Ok(x) if x.is_finite() => Ok(Value::Float(x)),
            _ => Err(self.err_at(start, "number is too large to represent")),
        }
    }
}

// ---------------------------------------------------------------------------
// Writer
// ---------------------------------------------------------------------------

/// Writes `value` with no whitespace (§4 "compact").
pub fn write_compact(value: &Value) -> Result<String, WriteError> {
    let mut out = String::new();
    write_value(&mut out, value, None, 0)?;
    Ok(out)
}

/// Writes `value` with two-space indentation and a final `\n` (§4 "pretty").
pub fn write_pretty(value: &Value) -> Result<String, WriteError> {
    let mut out = String::new();
    write_value(&mut out, value, Some(0), 0)?;
    out.push('\n');
    Ok(out)
}

/// `indent` is `None` for the compact form, otherwise the current
/// indentation level of the pretty form. `depth` counts enclosing
/// containers.
fn write_value(
    out: &mut String,
    value: &Value,
    indent: Option<usize>,
    depth: usize,
) -> Result<(), WriteError> {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(true) => out.push_str("true"),
        Value::Bool(false) => out.push_str("false"),
        Value::Int(i) => {
            if !(INT_MIN..INT_END).contains(i) {
                return Err(WriteError::IntOutOfRange(*i));
            }
            out.push_str(&i.to_string());
        }
        Value::Float(x) => write_float(out, *x)?,
        Value::Str(s) => write_string(out, s),
        Value::Array(items) => {
            if depth >= MAX_DEPTH {
                return Err(WriteError::TooDeep);
            }
            if items.is_empty() {
                out.push_str("[]");
                return Ok(());
            }
            out.push('[');
            for (n, item) in items.iter().enumerate() {
                if n > 0 {
                    out.push(',');
                }
                newline(out, indent.map(|l| l + 1));
                write_value(out, item, indent.map(|l| l + 1), depth + 1)?;
            }
            newline(out, indent);
            out.push(']');
        }
        Value::Object(members) => {
            if depth >= MAX_DEPTH {
                return Err(WriteError::TooDeep);
            }
            if members.is_empty() {
                out.push_str("{}");
                return Ok(());
            }
            out.push('{');
            for (n, (key, item)) in members.iter().enumerate() {
                if n > 0 {
                    out.push(',');
                }
                newline(out, indent.map(|l| l + 1));
                write_string(out, key);
                out.push(':');
                if indent.is_some() {
                    out.push(' ');
                }
                write_value(out, item, indent.map(|l| l + 1), depth + 1)?;
            }
            newline(out, indent);
            out.push('}');
        }
    }
    Ok(())
}

/// In the pretty form, a line break followed by `level` indentations.
fn newline(out: &mut String, level: Option<usize>) {
    if let Some(level) = level {
        out.push('\n');
        for _ in 0..level {
            out.push_str("  ");
        }
    }
}

/// Rust's shortest round-trip formatting (`{:?}`), which always has a `.`
/// or an exponent, so the reader classifies the output as a `Float` again.
fn write_float(out: &mut String, x: f64) -> Result<(), WriteError> {
    if !x.is_finite() {
        return Err(WriteError::NonFiniteFloat(x));
    }
    let s = format!("{x:?}");
    debug_assert!(s.contains(['.', 'e']), "f64 Debug output {s} must not read back as an Int");
    out.push_str(&s);
    Ok(())
}

fn write_string(out: &mut String, s: &str) {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    out.push('"');
    let mut run = 0;
    for (i, b) in s.bytes().enumerate() {
        let escape = match b {
            b'"' => "\\\"",
            b'\\' => "\\\\",
            b'\n' => "\\n",
            b'\r' => "\\r",
            b'\t' => "\\t",
            0x08 => "\\b",
            0x0C => "\\f",
            0x00..=0x1F => "",
            _ => continue,
        };
        out.push_str(&s[run..i]);
        if escape.is_empty() {
            out.push_str("\\u00");
            out.push(HEX[usize::from(b >> 4)] as char);
            out.push(HEX[usize::from(b & 0xF)] as char);
        } else {
            out.push_str(escape);
        }
        run = i + 1;
    }
    out.push_str(&s[run..]);
    out.push('"');
}
