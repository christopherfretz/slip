//! Section 35 (`ownership`) of a verify shard and the [`OwnershipList`] blob
//! format (`spec/model.md` §8.9; work package WP-150 stage 2).
//!
//! Section 35 holds `list<digest>`, the content ids of the observer's
//! [`OwnershipList`] blobs in strictly ascending order; phase `verify`,
//! status `ok` (empty `[]` is valid and required in a run with rules when the
//! observer has no entries, so absence never reads as "no entries").
//!
//! Each blob is raw wire-encoded (no `SLIP` container: `blobs/<content
//! digest>` already hashes the full byte stream) starting with `version: uv`
//! (currently 1):
//!
//! ```text
//! OwnershipList := version: uv             1
//!                  file:    bytes          subject path
//!                  through: bytes          empty, or read-only includer
//!                  made:    list<Made>     strictly ascending by (key, post)
//!                  fixed:   list<Fixed>    strictly ascending by (key, as)
//! Made          := key: digest, post: opt<digest>, rule: uv, line: uv, column: uv
//! Fixed         := key: digest, as: FixedAs, line: uv, column: uv, name: str
//! FixedAs       := t = 0 | m = 1
//! ```

use crate::json::Value;
use crate::ownership::{FixedAs, FixedEntry, MadeEntry, OwnershipList};
use crate::wire::{Decoder, Encoder, Error, ErrorKind};
use crate::{Digest, Phase, Status};

use super::common::{
    Enumeration, JResult, Result, dec_enum, dec_list, dec_opt, enc_enum, json_bytes, json_digest,
    json_enum, json_opt, json_uv, malformed, object, read_bytes, read_digest, read_enum, read_list,
    read_opt, read_record, read_str, read_uv,
};
use super::{At, JsonFormError, MESSAGE_MAX};

/// Section 35 (`spec/model.md` §8).
pub(super) const ID: u64 = 35;

impl Enumeration for FixedAs {
    const TABLE: &'static [(Self, u64, &'static str)] =
        &[(FixedAs::T, 0, "t"), (FixedAs::M, 1, "m")];
    const UNLISTED: &'static str = "unlisted FixedAs value";
    const EXPECTED: &'static str = "expected one of \"t\", \"m\"";
}

// ---------------------------------------------------------------------------
// Section 35 of a verify shard (`list<digest>`)

/// Where a section 35 invariant is violated.
#[derive(Clone, Copy, Debug)]
pub(super) enum Loc {
    Section,
    List(usize),
}

impl Loc {
    pub(super) fn path(self) -> String {
        const S: &str = "sections.ownership";
        match self {
            Loc::Section => S.to_owned(),
            Loc::List(i) => format!("{S}[{i}]"),
        }
    }
}

/// Section 35's invariants (`spec/model.md` §8, §8.9): phase `verify`,
/// status `ok`, then list ids strictly ascending.
pub(super) fn violation(
    lists: &[Digest],
    phase: Phase,
    status: Status,
) -> Option<(Loc, &'static str)> {
    if phase != Phase::Verify {
        return Some((Loc::Section, "an ownership section in a shard whose phase is not verify"));
    }
    if status != Status::Ok {
        return Some((Loc::Section, "an ownership section in a shard whose status is not ok"));
    }
    if let Some(i) = (1..lists.len()).find(|&i| lists[i - 1] >= lists[i]) {
        return Some((Loc::List(i), "ownership list ids are not strictly ascending"));
    }
    None
}

pub(super) fn encode(e: &mut Encoder, lists: &[Digest]) {
    e.count(lists.len());
    for id in lists {
        e.digest(id);
    }
}

/// File offsets of what a section 35 invariant can name.
#[derive(Debug, Default)]
pub(super) struct Offsets {
    section: usize,
    lists: Vec<usize>,
}

impl Offsets {
    pub(super) fn of(&self, l: Loc) -> usize {
        match l {
            Loc::Section => self.section,
            Loc::List(i) => self.lists[i],
        }
    }
}

pub(super) fn decode(d: &mut Decoder<'_>) -> Result<(Vec<Digest>, Offsets)> {
    let mut o = Offsets { section: d.offset(), lists: Vec::new() };
    let lists = dec_list(d, Some(&mut o.lists), Decoder::digest)?;
    Ok((lists, o))
}

pub(super) fn render(lists: &[Digest]) -> Value {
    Value::Array(lists.iter().map(json_digest).collect())
}

pub(super) fn read(v: &Value, at: &At<'_>) -> JResult<Vec<Digest>> {
    read_list(v, at, read_digest)
}

// ---------------------------------------------------------------------------
// OwnershipList blob format (`spec/model.md` §8.9)

const LINE_COL: &str = "line is 0 and column is not, or column is 0 and line is not";
const LONG: &str = "longer than 4096 bytes";

#[derive(Clone, Copy, Debug)]
enum ListLoc {
    Empty,
    Through,
    Made(usize),
    MadeLine(usize),
    Fixed(usize),
    FixedLine(usize),
    FixedName(usize),
}

impl ListLoc {
    fn path(self) -> String {
        match self {
            ListLoc::Empty => String::new(),
            ListLoc::Through => "through".to_owned(),
            ListLoc::Made(i) => format!("made[{i}]"),
            ListLoc::MadeLine(i) => format!("made[{i}].line"),
            ListLoc::Fixed(i) => format!("fixed[{i}]"),
            ListLoc::FixedLine(i) => format!("fixed[{i}].line"),
            ListLoc::FixedName(i) => format!("fixed[{i}].name"),
        }
    }
}

#[derive(Debug, Default)]
struct ListOffsets {
    through: usize,
    made_list: usize,
    made: Vec<usize>,
    made_line: Vec<usize>,
    fixed: Vec<usize>,
    fixed_line: Vec<usize>,
    fixed_name: Vec<usize>,
}

impl ListOffsets {
    fn of(&self, l: ListLoc) -> usize {
        match l {
            ListLoc::Empty => self.made_list,
            ListLoc::Through => self.through,
            ListLoc::Made(i) => self.made[i],
            ListLoc::MadeLine(i) => self.made_line[i],
            ListLoc::Fixed(i) => self.fixed[i],
            ListLoc::FixedLine(i) => self.fixed_line[i],
            ListLoc::FixedName(i) => self.fixed_name[i],
        }
    }
}

/// The reader rows of `OwnershipList` (`spec/model.md` §8.9), in order, each
/// checked over its whole list before the next.
fn list_violation(list: &OwnershipList) -> Option<(ListLoc, &'static str)> {
    // 1. Not empty.
    if list.made.is_empty() && list.fixed.is_empty() {
        return Some((ListLoc::Empty, "an empty ownership list"));
    }
    // 2. `through` requires a non-empty `file`.
    if !list.through.is_empty() && list.file.is_empty() {
        return Some((ListLoc::Through, "through without a file"));
    }
    // 3. `through` non-empty forbids `made` entries.
    if !list.through.is_empty() && !list.made.is_empty() {
        return Some((ListLoc::Made(0), "a made entry in a list entered through a read-only file"));
    }
    // 4. `made` strictly ascending by `(key, post)`.
    let m = &list.made;
    if let Some(i) = (1..m.len()).find(|&i| (m[i - 1].key, m[i - 1].post) >= (m[i].key, m[i].post))
    {
        return Some((ListLoc::Made(i), "made entries are not strictly ascending by (key, post)"));
    }
    // 5. `made` line == 0 iff column == 0.
    if let Some(i) = m.iter().position(|x| (x.line == 0) != (x.column == 0)) {
        return Some((ListLoc::MadeLine(i), LINE_COL));
    }
    // 6. `fixed` strictly ascending by `(key, as)`.
    let f = &list.fixed;
    if let Some(i) = (1..f.len()).find(|&i| (f[i - 1].key, f[i - 1].as_) >= (f[i].key, f[i].as_)) {
        return Some((ListLoc::Fixed(i), "fixed entries are not strictly ascending by (key, as)"));
    }
    // 7. `fixed` line == 0 iff column == 0.
    if let Some(i) = f.iter().position(|x| (x.line == 0) != (x.column == 0)) {
        return Some((ListLoc::FixedLine(i), LINE_COL));
    }
    // 8. `fixed` name at most MESSAGE_MAX (4096) bytes.
    if let Some(i) = f.iter().position(|x| x.name.len() > MESSAGE_MAX) {
        return Some((ListLoc::FixedName(i), LONG));
    }
    None
}

impl OwnershipList {
    /// The blob format version this crate writes and reads (`spec/model.md` §8.9).
    pub const VERSION: u64 = 1;

    /// Encodes the ownership list blob. Debug-asserts [`Self::validate`].
    pub fn to_bytes(&self) -> Vec<u8> {
        debug_assert!(
            self.validate().is_ok(),
            "OwnershipList::to_bytes of an invalid value: {:?}",
            self.validate()
        );
        let mut e = Encoder::new();
        e.uv(Self::VERSION).bytes(&self.file).bytes(&self.through).count(self.made.len());
        for m in &self.made {
            e.digest(&m.key);
            match &m.post {
                None => e.u8(0),
                Some(p) => e.u8(1).digest(p),
            };
            e.uv(m.rule).uv(m.line).uv(m.column);
        }
        e.count(self.fixed.len());
        for f in &self.fixed {
            e.digest(&f.key);
            enc_enum(&mut e, f.as_);
            e.uv(f.line).uv(f.column).str(&f.name);
        }
        e.into_bytes()
    }

    /// Decodes an ownership list blob, checking the version rule, every wire
    /// encoding and every reader row of `spec/model.md` §8.9.
    pub fn from_bytes(bytes: &[u8]) -> Result<OwnershipList> {
        let mut d = Decoder::new(bytes);
        let version = d.uv()?;
        if version == 0 || version > Self::VERSION {
            return Err(Error::new(
                ErrorKind::UnsupportedVersion,
                0,
                "unsupported ownership list version",
            ));
        }
        let file = d.bytes()?.to_vec();
        let mut o = ListOffsets { through: d.offset(), ..ListOffsets::default() };
        let through = d.bytes()?.to_vec();
        o.made_list = d.offset();
        let made = dec_list(&mut d, Some(&mut o.made), |d| {
            let key = d.digest()?;
            let post = dec_opt(d, Decoder::digest)?;
            let rule = d.uv()?;
            o.made_line.push(d.offset());
            let line = d.uv()?;
            let column = d.uv()?;
            Ok(MadeEntry { key, post, rule, line, column })
        })?;
        let fixed = dec_list(&mut d, Some(&mut o.fixed), |d| {
            let key = d.digest()?;
            let as_ = dec_enum(d)?;
            o.fixed_line.push(d.offset());
            let line = d.uv()?;
            let column = d.uv()?;
            o.fixed_name.push(d.offset());
            let name = d.str()?.to_owned();
            Ok(FixedEntry { key, as_, line, column, name })
        })?;
        d.finish()?;
        let list = OwnershipList { file, through, made, fixed };
        if let Some((l, what)) = list_violation(&list) {
            return Err(malformed(o.of(l), what));
        }
        Ok(list)
    }

    /// The JSON rendering (`spec/model.md` §8.9, §10).
    pub fn to_json(&self) -> Value {
        let made = self
            .made
            .iter()
            .map(|m| {
                object(vec![
                    ("key", json_digest(&m.key)),
                    ("post", json_opt(m.post.as_ref(), json_digest)),
                    ("rule", json_uv(m.rule)),
                    ("line", json_uv(m.line)),
                    ("column", json_uv(m.column)),
                ])
            })
            .collect();
        let fixed = self
            .fixed
            .iter()
            .map(|f| {
                object(vec![
                    ("key", json_digest(&f.key)),
                    ("as", json_enum(f.as_)),
                    ("line", json_uv(f.line)),
                    ("column", json_uv(f.column)),
                    ("name", Value::Str(f.name.clone())),
                ])
            })
            .collect();
        object(vec![
            ("version", json_uv(Self::VERSION)),
            ("file", json_bytes(&self.file)),
            ("through", json_bytes(&self.through)),
            ("made", Value::Array(made)),
            ("fixed", Value::Array(fixed)),
        ])
    }

    /// Reads the JSON rendering (`spec/model.md` §8.9, §10), checking the
    /// same version rule and invariants as [`Self::from_bytes`].
    pub fn from_json(v: &Value) -> JResult<OwnershipList> {
        let root = At::Root;
        let [version, file, through, made, fixed] =
            read_record(v, &root, ["version", "file", "through", "made", "fixed"])?;
        let ver_at = root.key("version");
        let have = read_uv(version, &ver_at)?;
        if have == 0 || have > Self::VERSION {
            return Err(ver_at.error(format!(
                "unsupported version {have}; this reader supports {}",
                Self::VERSION
            )));
        }
        let file = read_bytes(file, &root.key("file"))?;
        let through = read_bytes(through, &root.key("through"))?;
        let made = read_list(made, &root.key("made"), |v, at| {
            let [key, post, rule, line, column] =
                read_record(v, at, ["key", "post", "rule", "line", "column"])?;
            Ok(MadeEntry {
                key: read_digest(key, &at.key("key"))?,
                post: read_opt(post, &at.key("post"), read_digest)?,
                rule: read_uv(rule, &at.key("rule"))?,
                line: read_uv(line, &at.key("line"))?,
                column: read_uv(column, &at.key("column"))?,
            })
        })?;
        let fixed = read_list(fixed, &root.key("fixed"), |v, at| {
            let [key, as_, line, column, name] =
                read_record(v, at, ["key", "as", "line", "column", "name"])?;
            Ok(FixedEntry {
                key: read_digest(key, &at.key("key"))?,
                as_: read_enum(as_, &at.key("as"))?,
                line: read_uv(line, &at.key("line"))?,
                column: read_uv(column, &at.key("column"))?,
                name: read_str(name, &at.key("name"))?.to_owned(),
            })
        })?;
        let list = OwnershipList { file, through, made, fixed };
        if let Some((l, what)) = list_violation(&list) {
            return Err(JsonFormError { path: l.path(), message: what.to_owned() });
        }
        Ok(list)
    }

    /// Checks the invariants of `spec/model.md` §8.9 for a value built in
    /// memory.
    pub fn validate(&self) -> std::result::Result<(), String> {
        list_violation(self).map_or(Ok(()), |(l, what)| {
            let path = l.path();
            if path.is_empty() { Err(what.to_owned()) } else { Err(format!("{path}: {what}")) }
        })
    }
}
