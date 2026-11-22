//! Pieces shared by the three formats: enumerations, lists, `FileRef` and
//! `Command` in both forms, and the JSON conventions of `spec/model.md` §10.

use crate::json::Value;
use crate::wire::{Decoder, Encoder, Error, ErrorKind};
use crate::{
    Command, Digest, End, EnvVar, FileRef, Language, Phase, Reason, Status, hash::tagged_digest,
};

use super::{At, JsonFormError};

pub(super) type Result<T> = std::result::Result<T, Error>;
pub(super) type JResult<T> = std::result::Result<T, JsonFormError>;

pub(super) fn malformed(offset: usize, what: &'static str) -> Error {
    Error::new(ErrorKind::Malformed, offset, what)
}

// ---------------------------------------------------------------------------
// Enumerations (§2 "enumeration", §1 rule 6)

/// A §2 enumeration: its values, codes and names.
pub(super) trait Enumeration: Copy + PartialEq + 'static {
    /// `(value, code, name)` for every listed value.
    const TABLE: &'static [(Self, u64, &'static str)];
    /// The `what` of the `Malformed` error for an unlisted code.
    const UNLISTED: &'static str;
    /// The JSON error message for an unknown name.
    const EXPECTED: &'static str;

    fn code(self) -> u64 {
        Self::TABLE
            .iter()
            .find(|t| t.0 == self)
            .map_or_else(|| unreachable!("enumeration value missing from its table"), |t| t.1)
    }

    fn name(self) -> &'static str {
        Self::TABLE
            .iter()
            .find(|t| t.0 == self)
            .map_or_else(|| unreachable!("enumeration value missing from its table"), |t| t.2)
    }
}

impl Enumeration for Language {
    const TABLE: &'static [(Self, u64, &'static str)] = &[
        (Language::C, 1, "c"),
        (Language::Cxx, 2, "cxx"),
        (Language::ObjC, 3, "objc"),
        (Language::ObjCxx, 4, "objcxx"),
    ];
    const UNLISTED: &'static str = "unlisted Language value";
    const EXPECTED: &'static str = "expected one of \"c\", \"cxx\", \"objc\", \"objcxx\"";
}

impl Enumeration for Reason {
    const TABLE: &'static [(Self, u64, &'static str)] = &[
        (Reason::NotACompile, 1, "not_a_compile"),
        (Reason::UnsupportedLanguage, 2, "unsupported_language"),
        (Reason::UnsupportedFlag, 3, "unsupported_flag"),
        (Reason::MissingFile, 4, "missing_file"),
        (Reason::MalformedEntry, 5, "malformed_entry"),
        (Reason::UnsupportedCompiler, 6, "unsupported_compiler"),
    ];
    const UNLISTED: &'static str = "unlisted Reason value";
    const EXPECTED: &'static str = "expected one of \"not_a_compile\", \"unsupported_language\", \
         \"unsupported_flag\", \"missing_file\", \"malformed_entry\", \"unsupported_compiler\"";
}

impl Enumeration for Phase {
    const TABLE: &'static [(Self, u64, &'static str)] =
        &[(Phase::Map, 1, "map"), (Phase::Verify, 2, "verify")];
    const UNLISTED: &'static str = "unlisted Phase value";
    const EXPECTED: &'static str = "expected one of \"map\", \"verify\"";
}

impl Enumeration for Status {
    const TABLE: &'static [(Self, u64, &'static str)] = &[
        (Status::Ok, 0, "ok"),
        (Status::Diagnosed, 1, "diagnosed"),
        (Status::Rejected, 2, "rejected"),
        (Status::Failed, 3, "failed"),
    ];
    const UNLISTED: &'static str = "unlisted Status value";
    const EXPECTED: &'static str =
        "expected one of \"ok\", \"diagnosed\", \"rejected\", \"failed\"";
}

impl Enumeration for End {
    const TABLE: &'static [(Self, u64, &'static str)] = &[
        (End::Completed, 0, "completed"),
        (End::Crashed, 1, "crashed"),
        (End::KilledMemory, 2, "killed_memory"),
        (End::KilledTime, 3, "killed_time"),
        (End::Cancelled, 4, "cancelled"),
    ];
    const UNLISTED: &'static str = "unlisted End value";
    const EXPECTED: &'static str = "expected one of \"completed\", \"crashed\", \
         \"killed_memory\", \"killed_time\", \"cancelled\"";
}

pub(super) fn enc_enum<E: Enumeration>(e: &mut Encoder, v: E) {
    e.uv(v.code());
}

/// An enumeration: `Malformed` at its first byte if the code is unlisted.
pub(super) fn dec_enum<E: Enumeration>(d: &mut Decoder<'_>) -> Result<E> {
    let at = d.offset();
    let code = d.uv()?;
    E::TABLE.iter().find(|t| t.1 == code).map(|t| t.0).ok_or_else(|| malformed(at, E::UNLISTED))
}

pub(super) fn json_enum<E: Enumeration>(v: E) -> Value {
    Value::Str(v.name().to_owned())
}

pub(super) fn read_enum<E: Enumeration>(v: &Value, at: &At<'_>) -> JResult<E> {
    let name = read_str(v, at)?;
    E::TABLE.iter().find(|t| t.2 == name).map(|t| t.0).ok_or_else(|| at.error(E::EXPECTED))
}

// ---------------------------------------------------------------------------
// Lists

/// Capacity reserved up front for a list, whatever its count says: the
/// count is bounded by the bytes remaining (`Decoder::count`), but an
/// element in memory is larger than its smallest encoding.
const PREALLOC_MAX: usize = 1024;

/// `list<T>`, recording the offset of each element's first byte in
/// `offsets` when given.
pub(super) fn dec_list<'a, T>(
    d: &mut Decoder<'a>,
    mut offsets: Option<&mut Vec<usize>>,
    mut element: impl FnMut(&mut Decoder<'a>) -> Result<T>,
) -> Result<Vec<T>> {
    let n = d.count()?;
    let mut out = Vec::with_capacity(n.min(PREALLOC_MAX));
    for _ in 0..n {
        if let Some(offsets) = offsets.as_deref_mut() {
            offsets.push(d.offset());
        }
        out.push(element(d)?);
    }
    Ok(out)
}

pub(super) fn read_list<'v, T>(
    v: &'v Value,
    at: &At<'_>,
    mut element: impl FnMut(&'v Value, &At<'_>) -> JResult<T>,
) -> JResult<Vec<T>> {
    let items = v.as_array().ok_or_else(|| at.error("wrong type: expected an array"))?;
    items.iter().enumerate().map(|(i, item)| element(item, &at.index(i))).collect()
}

// ---------------------------------------------------------------------------
// opt<T> (§2)

/// `opt<T>`: `Malformed` at the tag byte unless it is 0 or 1.
pub(super) fn dec_opt<'a, T>(
    d: &mut Decoder<'a>,
    value: impl FnOnce(&mut Decoder<'a>) -> Result<T>,
) -> Result<Option<T>> {
    let at = d.offset();
    match d.u8()? {
        0 => Ok(None),
        1 => Ok(Some(value(d)?)),
        _ => Err(malformed(at, "opt tag is not 0 or 1")),
    }
}

/// The JSON form of `opt<T>` (§10): `null` when absent.
pub(super) fn json_opt<T>(v: Option<&T>, value: impl FnOnce(&T) -> Value) -> Value {
    v.map_or(Value::Null, value)
}

/// Reads `opt<T>`: `null` is absent, anything else must be a `T`.
pub(super) fn read_opt<'v, T>(
    v: &'v Value,
    at: &At<'_>,
    value: impl FnOnce(&'v Value, &At<'_>) -> JResult<T>,
) -> JResult<Option<T>> {
    match v {
        Value::Null => Ok(None),
        v => value(v, at).map(Some),
    }
}

// ---------------------------------------------------------------------------
// FileRef (§6) and Command (§7)

pub(super) fn enc_file_ref(e: &mut Encoder, f: &FileRef) {
    e.bytes(&f.path).uv(f.size).digest(&f.content);
}

pub(super) fn dec_file_ref(d: &mut Decoder<'_>) -> Result<FileRef> {
    Ok(FileRef { path: d.bytes()?.to_vec(), size: d.uv()?, content: d.digest()? })
}

pub(super) fn json_file_ref(f: &FileRef) -> Value {
    object(vec![
        ("path", json_bytes(&f.path)),
        ("size", json_uv(f.size)),
        ("content", json_digest(&f.content)),
    ])
}

pub(super) fn read_file_ref(v: &Value, at: &At<'_>) -> JResult<FileRef> {
    let [path, size, content] = read_record(v, at, ["path", "size", "content"])?;
    Ok(FileRef {
        path: read_bytes(path, &at.key("path"))?,
        size: read_uv(size, &at.key("size"))?,
        content: read_digest(content, &at.key("content"))?,
    })
}

pub(super) fn enc_command(e: &mut Encoder, c: &Command) {
    e.bytes(&c.directory).count(c.argv.len());
    for a in &c.argv {
        e.bytes(a);
    }
    e.count(c.env.len());
    for v in &c.env {
        e.bytes(&v.name).bytes(&v.value);
    }
}

/// A `Command`, recording the offset of each `env` element in `env_offsets`.
pub(super) fn dec_command(
    d: &mut Decoder<'_>,
    env_offsets: Option<&mut Vec<usize>>,
) -> Result<Command> {
    let directory = d.bytes()?.to_vec();
    let argv = dec_list(d, None, |d| Ok(d.bytes()?.to_vec()))?;
    let env = dec_list(d, env_offsets, |d| {
        Ok(EnvVar { name: d.bytes()?.to_vec(), value: d.bytes()?.to_vec() })
    })?;
    Ok(Command { directory, argv, env })
}

pub(super) fn json_command(c: &Command) -> Value {
    object(vec![
        ("directory", json_bytes(&c.directory)),
        ("argv", Value::Array(c.argv.iter().map(|a| json_bytes(a)).collect())),
        (
            "env",
            Value::Array(
                c.env
                    .iter()
                    .map(|v| {
                        object(vec![("name", json_bytes(&v.name)), ("value", json_bytes(&v.value))])
                    })
                    .collect(),
            ),
        ),
    ])
}

pub(super) fn read_command(v: &Value, at: &At<'_>) -> JResult<Command> {
    let [directory, argv, env] = read_record(v, at, ["directory", "argv", "env"])?;
    Ok(Command {
        directory: read_bytes(directory, &at.key("directory"))?,
        argv: read_list(argv, &at.key("argv"), read_bytes)?,
        env: read_list(env, &at.key("env"), |v, at| {
            let [name, value] = read_record(v, at, ["name", "value"])?;
            Ok(EnvVar {
                name: read_bytes(name, &at.key("name"))?,
                value: read_bytes(value, &at.key("value"))?,
            })
        })?,
    })
}

/// The tag of `ObserverId` (§3 table, §7).
pub(super) const OBSERVER_TAG: &str = "slip.observer.v1";

impl Command {
    /// `ObserverId = H("slip.observer.v1", Command)` (`spec/model.md` §7):
    /// the tagged digest of this command's §2 encoding.
    pub fn observer_id(&self) -> Digest {
        let mut e = Encoder::new();
        enc_command(&mut e, self);
        tagged_digest(OBSERVER_TAG, e.as_bytes())
    }
}

// ---------------------------------------------------------------------------
// JSON scalars (§10)

/// 2^53: integers below it are JSON numbers, others decimal strings.
const JSON_EXACT: u64 = 1 << 53;

pub(super) fn object(members: Vec<(&str, Value)>) -> Value {
    Value::Object(members.into_iter().map(|(k, v)| (k.to_owned(), v)).collect())
}

pub(super) fn json_uv(v: u64) -> Value {
    if v < JSON_EXACT { Value::Int(i128::from(v)) } else { Value::Str(v.to_string()) }
}

pub(super) fn json_digest(d: &Digest) -> Value {
    Value::Str(d.to_string())
}

pub(super) fn json_hex(bytes: &[u8]) -> Value {
    use std::fmt::Write as _;
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(s, "{b:02x}");
    }
    object(vec![("hex", Value::Str(s))])
}

pub(super) fn json_bytes(bytes: &[u8]) -> Value {
    match std::str::from_utf8(bytes) {
        Ok(s) => Value::Str(s.to_owned()),
        Err(_) => json_hex(bytes),
    }
}

pub(super) fn read_str<'v>(v: &'v Value, at: &At<'_>) -> JResult<&'v str> {
    v.as_str().ok_or_else(|| at.error("wrong type: expected a string"))
}

/// A `uv`: a number below 2^53, or the decimal string of one at or above it.
/// Any other spelling of the value is rejected, so the JSON form of a value
/// is unique.
pub(super) fn read_uv(v: &Value, at: &At<'_>) -> JResult<u64> {
    match v {
        Value::Int(i) => match u64::try_from(*i) {
            Ok(n) if n < JSON_EXACT => Ok(n),
            Ok(_) => Err(at.error("a value of 2^53 or more must be a decimal string")),
            Err(_) => Err(at.error("wrong type: expected an unsigned integer")),
        },
        Value::Str(s) => {
            let canonical = !s.is_empty()
                && s.bytes().all(|c| c.is_ascii_digit())
                && !(s.len() > 1 && s.starts_with('0'));
            match s.parse::<u64>() {
                Ok(n) if canonical && n >= JSON_EXACT => Ok(n),
                Ok(_) if canonical => {
                    Err(at.error("a value below 2^53 must be a number, not a string"))
                }
                _ => Err(at.error("not a decimal unsigned 64-bit integer")),
            }
        }
        _ => Err(at.error("wrong type: expected an unsigned integer")),
    }
}

pub(super) fn read_digest(v: &Value, at: &At<'_>) -> JResult<Digest> {
    Digest::from_hex(read_str(v, at)?)
        .ok_or_else(|| at.error("not a digest: expected 64 lower-case hexadecimal digits"))
}

/// The `{ "hex": "…" }` form: lower-case digits, an even number of them.
pub(super) fn read_hex(v: &Value, at: &At<'_>) -> JResult<Vec<u8>> {
    let [hex] = read_record(v, at, ["hex"])?;
    let at = at.key("hex");
    let text = read_str(hex, &at)?.as_bytes();
    if text.len() % 2 != 0 {
        return Err(at.error("odd number of hexadecimal digits"));
    }
    let digit = |c: u8| match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        _ => None,
    };
    text.chunks_exact(2)
        .map(|p| Some((digit(p[0])? << 4) | digit(p[1])?))
        .collect::<Option<Vec<u8>>>()
        .ok_or_else(|| at.error("not lower-case hexadecimal"))
}

/// `bytes` or `path`: a string, or `{ "hex": "…" }` holding bytes that are
/// not valid UTF-8 (valid UTF-8 in hex form is not the rendering, and is
/// rejected).
pub(super) fn read_bytes(v: &Value, at: &At<'_>) -> JResult<Vec<u8>> {
    match v {
        Value::Str(s) => Ok(s.as_bytes().to_vec()),
        Value::Object(_) => {
            let bytes = read_hex(v, at)?;
            if std::str::from_utf8(&bytes).is_ok() {
                return Err(at.key("hex").error("valid UTF-8 must be written as a string"));
            }
            Ok(bytes)
        }
        _ => Err(at.error("wrong type: expected a string or {\"hex\": …}")),
    }
}

/// The members of a record, which must be exactly `fields`, in that order.
pub(super) fn read_record<'v, const N: usize>(
    v: &'v Value,
    at: &At<'_>,
    fields: [&'static str; N],
) -> JResult<[&'v Value; N]> {
    let members = v.as_object().ok_or_else(|| at.error("wrong type: expected an object"))?;
    let mut found: [Option<&'v Value>; N] = [None; N];
    let mut next = 0;
    for (key, value) in members {
        match fields.iter().position(|f| f == key) {
            None => return Err(at.key(key).error("unknown member")),
            Some(i) if i < next => return Err(at.key(key).error("member out of order")),
            Some(i) => {
                found[i] = Some(value);
                next = i + 1;
            }
        }
    }
    let mut out: [&'v Value; N] = [&Value::Null; N];
    for (i, f) in found.iter().enumerate() {
        out[i] = f.ok_or_else(|| at.key(fields[i]).error("missing member"))?;
    }
    Ok(out)
}
