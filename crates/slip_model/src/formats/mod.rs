//! Binary and JSON forms of the plan, shard and run record
//! (`spec/model.md` §6–§10), work package WP-006, and of the shard's `digests`
//! and `verdict` sections (`spec/digest.md` §11, `spec/verify.md` §6), work
//! package WP-015, and of declared changes (`spec/verify.md` §2), work
//! package WP-022, and of the transform request (`spec/edits.md` §3) and
//! the shard's sections `edits`, `binding`, `rule_use` and `spellings`
//! (`spec/edits.md` §4, `spec/verify.md` §9.1, §10.4), work package WP-060,
//! and of the edit set, overlay, result and transform run record
//! (`spec/edits.md` §6, §7.1, §8.5, §10.6), work package WP-061.
//!
//! Each of [`Plan`], [`Shard`], [`RunRecord`], [`Declared`],
//! [`Transform`], [`EditSet`], [`Overlay`], [`TransformResult`] and
//! [`TransformRun`] has `KIND`, `VERSION`, `to_bytes`, `from_bytes`,
//! `to_json`, `from_json` and `validate`.
//! [`dump`] renders any of the nine as pretty JSON and [`undump`] turns
//! such a rendering back into the file's bytes, unknown ignorable sections
//! included; [`content_id`] validates a file and returns its trailer.
//!
//! Readers check every rule of the spec: the container (§5), every
//! encoding (§2), every enumeration (§2) and the invariants of each
//! format. The order of checks, offsets and JSON rules are `spec/model.md`
//! §5.4, §7.1, §8.1–§8.3, §10.1 and §10.2.

use std::fmt::{self, Write as _};

use crate::container::{FIRST_IGNORABLE, Reader, TRAILER_LEN, Writer};
use crate::declared::Declared;
use crate::editset::EditSet;
use crate::json::{self, Value};
use crate::overlay::Overlay;
use crate::result::TransformResult;
use crate::transform::Transform;
use crate::transform_run::TransformRun;
use crate::wire::{Decoder, Encoder, Error, ErrorKind};
use crate::{Digest, Observer, Phase, Plan, RunRecord, Shard};

mod agreement;
mod common;
mod declared;
mod digests;
mod edits;
mod editset;
mod existing_use;
mod found;
mod names;
mod negatives;
mod overlay;
mod plan;
mod rebuilt_use;
mod result;
mod run;
mod shard;
mod spellings;
mod transform;
mod transform_run;
mod verdict;

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

/// The longest `Outcome.message`, in bytes (§8).
pub const MESSAGE_MAX: usize = 4096;

/// `message` cut to at most [`MESSAGE_MAX`] bytes at a character boundary
/// (§8): what a writer of a shard stores as `Outcome.message`.
pub fn clip_message(message: &str) -> &str {
    if message.len() <= MESSAGE_MAX {
        return message;
    }
    let mut end = MESSAGE_MAX;
    while !message.is_char_boundary(end) {
        end -= 1;
    }
    &message[..end]
}

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
    /// The file's format version, `1..=` the reader's.
    pub version: u32,
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
    let version = reader.version();
    reader.finish()?;
    Ok(Opened { sections, extras, end: bytes.len() - TRAILER_LEN - 1, content_id, version })
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
    /// The rendering's format version, `1..=` the reader's.
    pub version: u32,
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
    // `have` is in `1..=version`, a `u32`.
    let version = u32::try_from(have).unwrap_or(version);
    Ok(JsonSections { known, extras, version })
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
    ($ty:ident, $module:ident, $kind:literal, $version:literal) => {
        impl $ty {
            /// The container kind (§5.1).
            pub const KIND: [u8; 4] = *$kind;
            /// The format version this crate writes; it reads every
            /// version from 1 to this one.
            pub const VERSION: u32 = $version;

            /// The file. The value must satisfy [`Self::validate`]; that is
            /// only debug-asserted, and a release build writes an invalid
            /// value as it is: a producer calls `validate()` first.
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

            /// [`Self::from_bytes`] and the file's content id (§5, as
            /// [`content_id`] gives it) from one decode.
            pub fn from_bytes_with_id(bytes: &[u8]) -> std::result::Result<($ty, Digest), Error> {
                let (value, _, id) = $module::decode(bytes)?;
                Ok((value, id))
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

format_impl!(Plan, plan, b"PLAN", 1);
format_impl!(Shard, shard, b"SHRD", 2);
format_impl!(RunRecord, run, b"RUNR", 1);
format_impl!(Declared, declared, b"DECL", 4);
format_impl!(Transform, transform, b"TRFM", 3);
format_impl!(EditSet, editset, b"EDTS", 3);
format_impl!(Overlay, overlay, b"OVLY", 1);
format_impl!(TransformResult, result, b"RSLT", 2);
format_impl!(TransformRun, transform_run, b"RUNT", 1);

/// The kinds [`dump`], [`undump`] and [`content_id`] know.
const KINDS: [[u8; 4]; 9] = [
    Plan::KIND,
    Shard::KIND,
    RunRecord::KIND,
    Declared::KIND,
    Transform::KIND,
    EditSet::KIND,
    Overlay::KIND,
    TransformResult::KIND,
    TransformRun::KIND,
];

/// The kind of a file, if its header names one of the nine formats.
fn kind_of(bytes: &[u8]) -> Option<[u8; 4]> {
    let kind: [u8; 4] = bytes.get(8..12)?.try_into().ok()?;
    KINDS.contains(&kind).then_some(kind)
}

/// Renders a file of one of the nine kinds as pretty JSON (§10), for
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
        Some(k) if k == Declared::KIND => {
            let (x, extras, _, version) = declared::decode_versioned(bytes)?;
            declared::render_as(&x, &extras, version)
        }
        Some(k) if k == Transform::KIND => {
            let (t, extras, _) = transform::decode(bytes)?;
            transform::render(&t, &extras)
        }
        Some(k) if k == EditSet::KIND => {
            let (x, extras, _) = editset::decode(bytes)?;
            editset::render(&x, &extras)
        }
        Some(k) if k == Overlay::KIND => {
            let (x, extras, _) = overlay::decode(bytes)?;
            overlay::render(&x, &extras)
        }
        Some(k) if k == TransformResult::KIND => {
            let (x, extras, _) = result::decode(bytes)?;
            result::render(&x, &extras)
        }
        Some(k) if k == TransformRun::KIND => {
            let (x, extras, _) = transform_run::decode(bytes)?;
            transform_run::render(&x, &extras)
        }
        // A plan, or a file that is not one of the nine: opening it as a
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
        "DECL" => {
            let (x, extras, version) = declared::read_versioned(value)?;
            Ok(declared::encode_as(&x, &extras_ref(&extras), version))
        }
        "TRFM" => {
            let (t, extras) = transform::read(value)?;
            Ok(transform::encode(&t, &extras_ref(&extras)))
        }
        "EDTS" => {
            let (x, extras) = editset::read(value)?;
            Ok(editset::encode(&x, &extras_ref(&extras)))
        }
        "OVLY" => {
            let (x, extras) = overlay::read(value)?;
            Ok(overlay::encode(&x, &extras_ref(&extras)))
        }
        "RSLT" => {
            let (x, extras) = result::read(value)?;
            Ok(result::encode(&x, &extras_ref(&extras)))
        }
        "RUNT" => {
            let (x, extras) = transform_run::read(value)?;
            Ok(transform_run::encode(&x, &extras_ref(&extras)))
        }
        _ => Err(root.key("kind").error(
            "expected one of \"PLAN\", \"SHRD\", \"RUNR\", \"DECL\", \"TRFM\", \"EDTS\", \
             \"OVLY\", \"RSLT\", \"RUNT\"",
        )),
    }
}

/// The content id (§5) of a file of one of the nine kinds: its trailer digest,
/// returned only once the whole file has been validated as by `from_bytes`.
pub fn content_id(bytes: &[u8]) -> std::result::Result<Digest, Error> {
    Ok(match kind_of(bytes) {
        Some(k) if k == Shard::KIND => shard::decode(bytes)?.2,
        Some(k) if k == RunRecord::KIND => run::decode(bytes)?.2,
        Some(k) if k == Declared::KIND => declared::decode(bytes)?.2,
        Some(k) if k == Transform::KIND => transform::decode(bytes)?.2,
        Some(k) if k == EditSet::KIND => editset::decode(bytes)?.2,
        Some(k) if k == Overlay::KIND => overlay::decode(bytes)?.2,
        Some(k) if k == TransformResult::KIND => result::decode(bytes)?.2,
        Some(k) if k == TransformRun::KIND => transform_run::decode(bytes)?.2,
        _ => plan::decode(bytes)?.2,
    })
}

/// `(plan, round, started_unix_ns)` of a transform run record
/// (`spec/edits.md` §10.6), for the store's path of it (`spec/exec.md`
/// §2.2): the whole record is validated as by `TransformRun::from_bytes`
/// first, so a record that does not read is refused whatever its prefix.
pub fn transform_run_names(bytes: &[u8]) -> std::result::Result<(Digest, u64, u64), Error> {
    transform_run::names(bytes)
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
