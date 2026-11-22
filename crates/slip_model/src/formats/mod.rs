//! Binary and JSON forms of the plan, shard and run record
//! (`spec/model.md` §6–§10). Work package WP-006.
//!
//! Each of [`Plan`], [`Shard`] and [`RunRecord`] has `KIND`, `VERSION`,
//! `to_bytes`, `from_bytes`, `to_json`, `from_json` and `validate`.
//! [`dump`] renders any of the three as pretty JSON and [`undump`] turns
//! such a rendering back into the file's bytes, unknown ignorable sections
//! included; [`content_id`] validates a file and returns its trailer.
//!
//! Readers check every rule of the spec: the container (§5), every
//! encoding (§2), every enumeration (§1 rule 6) and the invariants of each
//! format. Rules the spec leaves open are recorded in
//! `spec/landed/model/WP-006.md`.

use std::fmt::{self, Write as _};

use crate::container::{FIRST_IGNORABLE, Reader, TRAILER_LEN, Writer};
use crate::json::{self, Value};
use crate::wire::{Decoder, Encoder, Error, ErrorKind};
use crate::{Digest, Observer, Phase, Plan, RunRecord, Shard};

mod common;
mod plan;
mod run;
mod shard;

use common::{JResult, Result, read_hex, read_record, read_str, read_uv};

/// An error converting the JSON rendering (§10) to a value: the JSON path
/// of the offending value (`sections.observers[3].main`; empty for the
/// top-level value) and what is wrong with it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JsonFormError {
    pub path: String,
    pub message: String,
}

impl fmt::Display for JsonFormError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.path.is_empty() {
            write!(f, "at the top level: {}", self.message)
        } else {
            write!(f, "at {}: {}", self.path, self.message)
        }
    }
}

impl std::error::Error for JsonFormError {}

/// A JSON path under construction, rendered only when an error needs it.
#[derive(Clone, Copy, Debug)]
pub(super) enum At<'p> {
    Root,
    Key(&'p At<'p>, &'p str),
    Index(&'p At<'p>, usize),
}

impl<'p> At<'p> {
    pub(super) fn key<'q>(&'q self, key: &'q str) -> At<'q> {
        At::Key(self, key)
    }

    pub(super) fn index(&'p self, i: usize) -> At<'p> {
        At::Index(self, i)
    }

    fn render(&self, out: &mut String) {
        match self {
            At::Root => {}
            At::Key(parent, key) => {
                parent.render(out);
                if !out.is_empty() {
                    out.push('.');
                }
                out.push_str(key);
            }
            At::Index(parent, i) => {
                parent.render(out);
                let _ = write!(out, "[{i}]");
            }
        }
    }

    pub(super) fn error(&self, message: impl Into<String>) -> JsonFormError {
        let mut path = String::new();
        self.render(&mut path);
        JsonFormError { path, message: message.into() }
    }
}

/// An unknown ignorable section: its id (64 or above) and payload.
pub(super) type Extra<'a> = (u64, &'a [u8]);

/// Unknown ignorable sections read from JSON.
pub(super) type OwnedExtras = Vec<(u64, Vec<u8>)>;

// ---------------------------------------------------------------------------
// Binary: opening a file

/// A container opened for one format: a decoder for each known section id
/// (`None` if absent), the unknown ignorable sections, the offset of the
/// `end` byte, and the content id.
pub(super) struct Opened<'a, const N: usize> {
    pub sections: [Option<Decoder<'a>>; N],
    pub extras: Vec<Extra<'a>>,
    pub end: usize,
    pub content_id: Digest,
}

/// Opens `bytes` (§5 rule 5) and checks that every critical section present
/// is one of `ids` (`UnknownSection`) before any section is interpreted.
pub(super) fn open<const N: usize>(
    bytes: &[u8],
    kind: [u8; 4],
    version: u32,
    ids: [u64; N],
) -> Result<Opened<'_, N>> {
    let reader = Reader::open(bytes, kind, version)?;
    let sections = ids.map(|id| reader.decoder(id));
    let extras = reader
        .ids()
        .filter(|&id| id >= FIRST_IGNORABLE)
        .filter_map(|id| reader.section(id).map(|p| (id, p)))
        .collect();
    let content_id = reader.content_id();
    reader.finish()?;
    Ok(Opened { sections, extras, end: bytes.len() - TRAILER_LEN - 1, content_id })
}

/// A section a format marks required (§5 rule 6): `Malformed` at the `end`
/// byte if absent.
pub(super) fn required<'a>(
    section: Option<Decoder<'a>>,
    end: usize,
    what: &'static str,
) -> Result<Decoder<'a>> {
    section.ok_or(Error::new(ErrorKind::Malformed, end, what))
}

/// A whole file: the known sections in ascending id order, then `extras`.
pub(super) fn seal(
    kind: [u8; 4],
    version: u32,
    sections: &[(u64, &[u8])],
    extras: &[Extra<'_>],
) -> Vec<u8> {
    let mut w = Writer::new(kind, version);
    for &(id, payload) in sections.iter().chain(extras) {
        w.section(id, payload);
    }
    w.finish()
}

// ---------------------------------------------------------------------------
// JSON: the container rendering (§10)

/// A section of a format, as named in the JSON rendering.
pub(super) struct SectionName {
    pub id: u64,
    pub name: &'static str,
    pub required: bool,
}

/// `{ "kind", "version", "sections" }` with `known` (in ascending id order)
/// followed by `extras` rendered as `"#<id>": { "hex": … }`.
pub(super) fn render_container(
    kind: [u8; 4],
    version: u32,
    known: Vec<(&str, Value)>,
    extras: &[Extra<'_>],
) -> Value {
    let mut sections: Vec<(String, Value)> =
        known.into_iter().map(|(k, v)| (k.to_owned(), v)).collect();
    for &(id, payload) in extras {
        sections.push((format!("#{id}"), common::json_hex(payload)));
    }
    common::object(vec![
        ("kind", Value::Str(String::from_utf8_lossy(&kind).into_owned())),
        ("version", Value::Int(i128::from(version))),
        ("sections", Value::Object(sections)),
    ])
}

/// The sections of a JSON rendering: the value of each known section (in
/// the order of `names`; `None` if absent and optional) and the unknown
/// ignorable sections.
pub(super) struct JsonSections<'v, const N: usize> {
    pub known: [Option<&'v Value>; N],
    pub extras: OwnedExtras,
}

/// Reads `{ "kind", "version", "sections" }` for a format whose sections
/// are `names` (ascending id). Members of `sections` must be in ascending
/// id order; `"#<id>"` is accepted for an id of 64 or above only.
pub(super) fn read_container<'v, const N: usize>(
    v: &'v Value,
    kind: [u8; 4],
    version: u32,
    names: &[SectionName; N],
) -> JResult<JsonSections<'v, N>> {
    let root = At::Root;
    let [k, ver, sections] = read_record(v, &root, ["kind", "version", "sections"])?;
    let kind_name = String::from_utf8_lossy(&kind);
    if read_str(k, &root.key("kind"))? != kind_name {
        return Err(root.key("kind").error(format!("expected \"{kind_name}\"")));
    }
    let at = root.key("version");
    let have = read_uv(ver, &at)?;
    if have == 0 || have > u64::from(version) {
        return Err(at.error(format!("unsupported version {have}; this reader supports {version}")));
    }
    let at = root.key("sections");
    let members = sections.as_object().ok_or_else(|| at.error("wrong type: expected an object"))?;
    let mut known: [Option<&'v Value>; N] = [None; N];
    let mut extras = Vec::new();
    let mut last = 0u64;
    for (key, value) in members {
        let here = at.key(key);
        let id = if let Some(i) = names.iter().position(|n| n.name == key) {
            known[i] = Some(value);
            names[i].id
        } else {
            let id = unknown_section_id(key).ok_or_else(|| here.error("unknown member"))?;
            extras.push((id, read_hex(value, &here)?));
            id
        };
        if id <= last {
            return Err(here.error("member out of order"));
        }
        last = id;
    }
    for (i, n) in names.iter().enumerate() {
        if n.required && known[i].is_none() {
            return Err(at.key(n.name).error("missing member"));
        }
    }
    Ok(JsonSections { known, extras })
}

/// The id of `"#<id>"`: canonical decimal, 64 or above (an unknown critical
/// section has no rendering, since no reader can produce one).
fn unknown_section_id(key: &str) -> Option<u64> {
    let digits = key.strip_prefix('#')?;
    if digits.is_empty()
        || !digits.bytes().all(|c| c.is_ascii_digit())
        || (digits.len() > 1 && digits.starts_with('0'))
    {
        return None;
    }
    digits.parse::<u64>().ok().filter(|&id| id >= FIRST_IGNORABLE)
}

fn extras_ref(extras: &OwnedExtras) -> Vec<Extra<'_>> {
    extras.iter().map(|(id, p)| (*id, p.as_slice())).collect()
}

fn pretty(v: &Value) -> String {
    json::write_pretty(v).unwrap_or_else(|e| unreachable!("a format rendering is writable: {e}"))
}

// ---------------------------------------------------------------------------
// The typed interface

macro_rules! format_impl {
    ($ty:ident, $module:ident, $kind:literal) => {
        impl $ty {
            /// The container kind (§5.1).
            pub const KIND: [u8; 4] = *$kind;
            /// The format version this crate reads and writes.
            pub const VERSION: u32 = 1;

            /// The file. The value must satisfy [`Self::validate`]; that is
            /// checked by a debug assertion.
            pub fn to_bytes(&self) -> Vec<u8> {
                debug_assert!(
                    self.validate().is_ok(),
                    "{}::to_bytes of an invalid value: {:?}",
                    stringify!($ty),
                    self.validate()
                );
                $module::encode(self, &[])
            }

            /// Reads a file, checking every rule and invariant of the spec.
            /// Unknown ignorable sections are skipped.
            pub fn from_bytes(bytes: &[u8]) -> std::result::Result<$ty, Error> {
                Ok($module::decode(bytes)?.0)
            }

            /// The JSON rendering (§10).
            pub fn to_json(&self) -> Value {
                $module::render(self, &[])
            }

            /// Reads the JSON rendering (§10), checking the same invariants
            /// as [`Self::from_bytes`]. `"#<id>"` sections are checked and
            /// skipped.
            pub fn from_json(value: &Value) -> std::result::Result<$ty, JsonFormError> {
                Ok($module::read(value)?.0)
            }

            /// The invariants of the format, for a value built in memory:
            /// `Err` names the JSON path of the first violation and the rule.
            pub fn validate(&self) -> std::result::Result<(), String> {
                $module::check(self).map_err(|(path, what)| format!("{path}: {what}"))
            }
        }
    };
}

format_impl!(Plan, plan, b"PLAN");
format_impl!(Shard, shard, b"SHRD");
format_impl!(RunRecord, run, b"RUNR");

/// The kind of a file, if its header names one of the three formats.
fn kind_of(bytes: &[u8]) -> Option<[u8; 4]> {
    let kind: [u8; 4] = bytes.get(8..12)?.try_into().ok()?;
    [Plan::KIND, Shard::KIND, RunRecord::KIND].contains(&kind).then_some(kind)
}

/// Renders a plan, shard or run record as pretty JSON (§10), for
/// `slip dump`. The file is validated as by `from_bytes`; unknown ignorable
/// sections are rendered as `"#<id>"`.
pub fn dump(bytes: &[u8]) -> std::result::Result<String, Error> {
    let value = match kind_of(bytes) {
        Some(k) if k == Shard::KIND => {
            let (s, extras, _) = shard::decode(bytes)?;
            shard::render(&s, &extras)
        }
        Some(k) if k == RunRecord::KIND => {
            let (r, extras, _) = run::decode(bytes)?;
            run::render(&r, &extras)
        }
        // A plan, or a file that is not one of the three: opening it as a
        // plan reports `BadMagic` (or `Truncated`) for the latter.
        _ => {
            let (p, extras, _) = plan::decode(bytes)?;
            plan::render(&p, &extras)
        }
    };
    Ok(pretty(&value))
}

/// The bytes of the file whose JSON rendering (§10) is `value`: the inverse
/// of [`dump`], unknown ignorable sections included. The value is checked as
/// by `from_json`.
pub fn undump(value: &Value) -> std::result::Result<Vec<u8>, JsonFormError> {
    let root = At::Root;
    let [kind, _, _] = read_record(value, &root, ["kind", "version", "sections"])?;
    match read_str(kind, &root.key("kind"))? {
        "PLAN" => {
            let (p, extras) = plan::read(value)?;
            Ok(plan::encode(&p, &extras_ref(&extras)))
        }
        "SHRD" => {
            let (s, extras) = shard::read(value)?;
            Ok(shard::encode(&s, &extras_ref(&extras)))
        }
        "RUNR" => {
            let (r, extras) = run::read(value)?;
            Ok(run::encode(&r, &extras_ref(&extras)))
        }
        _ => Err(root.key("kind").error("expected one of \"PLAN\", \"SHRD\", \"RUNR\"")),
    }
}

/// The content id (§5) of a plan, shard or run record: its trailer digest,
/// returned only once the whole file has been validated as by `from_bytes`.
pub fn content_id(bytes: &[u8]) -> std::result::Result<Digest, Error> {
    Ok(match kind_of(bytes) {
        Some(k) if k == Shard::KIND => shard::decode(bytes)?.2,
        Some(k) if k == RunRecord::KIND => run::decode(bytes)?.2,
        _ => plan::decode(bytes)?.2,
    })
}

// ---------------------------------------------------------------------------
// Record codecs shared with other protocols (spec/exec.md §5.2)

/// Appends the §7 encoding of an `Observer`, as it appears in the plan.
pub fn encode_observer(observer: &Observer, out: &mut Encoder) {
    plan::enc_observer(out, observer);
}

/// Reads one `Observer` (§7), validating the record on its own: every
/// encoding, `id == observer_id(command)`, `command.env` strictly ascending
/// by name, `origins` non-empty and strictly ascending. Checks that need
/// the plan (configuration indices, uniqueness, plan order) are not made.
pub fn decode_observer(input: &mut Decoder<'_>) -> std::result::Result<Observer, Error> {
    plan::dec_observer(input, &mut Vec::new())
}

/// Appends the §8 encoding of a `Phase`.
pub fn encode_phase(phase: Phase, out: &mut Encoder) {
    common::enc_enum(out, phase);
}

/// Reads one `Phase` (§8); an unlisted value is `Malformed`.
pub fn decode_phase(input: &mut Decoder<'_>) -> std::result::Result<Phase, Error> {
    common::dec_enum(input)
}
