//! The transform request, kind `TRFM` (`spec/edits.md` §3, §2.1). Work
//! package WP-060; the order of checks, offsets, paths and the JSON form
//! are `spec/landed/edits/WP-060.md` §1.

use crate::Digest;
use crate::json::Value;
use crate::transform::{
    Ownership, Rename, Script, TargetKind, Transform, TransformKind, is_identifier, is_keyword,
};
use crate::wire::{Decoder, Encoder};

use super::common::{
    Enumeration, JResult, Result, dec_enum, dec_list, dec_opt, enc_enum, json_bytes, json_digest,
    json_enum, json_opt, json_uv, malformed, object, read_bool, read_bytes, read_digest, read_enum,
    read_list, read_opt, read_record, read_uv,
};
use super::{
    At, Extra, JsonFormError, OwnedExtras, SectionName, open, read_container, render_container,
    seal,
};

/// The section id (`spec/edits.md` §3).
const TRANSFORM: u64 = 1;

impl Enumeration for TransformKind {
    const TABLE: &'static [(Self, u64, &'static str)] =
        &[(TransformKind::Rename, 1, "rename"), (TransformKind::Script, 2, "script")];
    const UNLISTED: &'static str = "unlisted TransformKind value";
    const EXPECTED: &'static str = "expected one of \"rename\", \"script\"";
}

impl Enumeration for TargetKind {
    const TABLE: &'static [(Self, u64, &'static str)] = &[
        (TargetKind::Any, 0, "any"),
        (TargetKind::Function, 1, "function"),
        (TargetKind::Variable, 2, "variable"),
    ];
    const UNLISTED: &'static str = "unlisted TargetKind value";
    const EXPECTED: &'static str = "expected one of \"any\", \"function\", \"variable\"";
}

// ---------------------------------------------------------------------------
// Invariants

#[derive(Clone, Copy, Debug)]
enum Loc {
    Owned,
    OwnedAt(usize),
    ReadonlyAt(usize),
    Indivisible,
    Rename,
    Target,
    TargetAt(usize),
    NewName,
    Script,
}

impl Loc {
    fn path(self) -> String {
        const S: &str = "sections.transform";
        match self {
            Loc::Owned => format!("{S}.ownership.owned"),
            Loc::OwnedAt(i) => format!("{S}.ownership.owned[{i}]"),
            Loc::ReadonlyAt(i) => format!("{S}.ownership.readonly[{i}]"),
            Loc::Indivisible => format!("{S}.indivisible"),
            Loc::Rename => format!("{S}.rename"),
            Loc::Target => format!("{S}.rename.target"),
            Loc::TargetAt(i) => format!("{S}.rename.target[{i}]"),
            Loc::NewName => format!("{S}.rename.new_name"),
            Loc::Script => format!("{S}.script"),
        }
    }
}

fn first_not_ascending(list: &[Vec<u8>]) -> Option<usize> {
    (1..list.len()).find(|&i| list[i - 1] >= list[i])
}

/// The first violation, in field order (`spec/landed/edits/WP-060.md` §1).
fn violation(t: &Transform) -> Option<(Loc, &'static str)> {
    let o = &t.ownership;
    if o.owned.is_empty() {
        return Some((Loc::Owned, "owned is empty"));
    }
    if let Some(i) = first_not_ascending(&o.owned) {
        return Some((Loc::OwnedAt(i), "owned is not strictly ascending"));
    }
    if let Some(i) = first_not_ascending(&o.readonly) {
        return Some((Loc::ReadonlyAt(i), "readonly is not strictly ascending"));
    }
    if let Some(i) = o.readonly.iter().position(|r| o.owned.binary_search(r).is_ok()) {
        return Some((Loc::ReadonlyAt(i), "a readonly entry is also in owned"));
    }
    let rename = t.kind == TransformKind::Rename;
    if rename && !t.indivisible {
        return Some((Loc::Indivisible, "a rename is not indivisible"));
    }
    match (&t.rename, rename) {
        (None, true) => return Some((Loc::Rename, "a rename request has no rename")),
        (Some(_), false) => return Some((Loc::Rename, "a script request has a rename")),
        _ => {}
    }
    if let Some(r) = &t.rename {
        if r.target.is_empty() {
            return Some((Loc::Target, "the target has no component"));
        }
        for (i, c) in r.target.iter().enumerate() {
            if let Some(what) = name_violation(c) {
                return Some((Loc::TargetAt(i), what));
            }
        }
        if let Some(what) = name_violation(&r.new_name) {
            return Some((Loc::NewName, what));
        }
        if r.target.last() == Some(&r.new_name) {
            return Some((Loc::NewName, "the new name is the target's last component"));
        }
    }
    match (&t.script, t.kind == TransformKind::Script) {
        (None, true) => Some((Loc::Script, "a script request has no script")),
        (Some(_), false) => Some((Loc::Script, "a rename request has a script")),
        _ => None,
    }
}

fn name_violation(name: &[u8]) -> Option<&'static str> {
    if !is_identifier(name) {
        Some("not an identifier [A-Za-z_][A-Za-z0-9_]*")
    } else if is_keyword(name) {
        Some("a keyword of C++")
    } else {
        None
    }
}

pub(super) fn check(t: &Transform) -> std::result::Result<(), (String, &'static str)> {
    violation(t).map_or(Ok(()), |(l, what)| Err((l.path(), what)))
}

// ---------------------------------------------------------------------------
// Binary

fn enc_list(e: &mut Encoder, list: &[Vec<u8>]) {
    e.count(list.len());
    for b in list {
        e.bytes(b);
    }
}

pub(super) fn encode(t: &Transform, extras: &[Extra<'_>]) -> Vec<u8> {
    let mut e = Encoder::new();
    enc_enum(&mut e, t.kind);
    enc_list(&mut e, &t.ownership.owned);
    enc_list(&mut e, &t.ownership.readonly);
    e.bool(t.indivisible);
    match &t.rename {
        None => {
            e.u8(0);
        }
        Some(r) => {
            e.u8(1);
            enc_list(&mut e, &r.target);
            enc_enum(&mut e, r.kind);
            e.bytes(&r.new_name);
        }
    }
    match &t.script {
        None => {
            e.u8(0);
        }
        Some(s) => {
            e.u8(1).digest(&s.id).bytes(&s.file);
            enc_list(&mut e, &s.options);
            e.uv(s.steps).uv(s.bytes).uv(s.depth);
        }
    }
    seal(Transform::KIND, Transform::VERSION, &[(TRANSFORM, e.as_bytes())], extras)
}

#[derive(Default)]
struct Offsets {
    owned: usize,
    owned_at: Vec<usize>,
    readonly_at: Vec<usize>,
    indivisible: usize,
    rename: usize,
    target: usize,
    target_at: Vec<usize>,
    new_name: usize,
    script: usize,
}

impl Offsets {
    fn of(&self, l: Loc) -> usize {
        match l {
            Loc::Owned => self.owned,
            Loc::OwnedAt(i) => self.owned_at[i],
            Loc::ReadonlyAt(i) => self.readonly_at[i],
            Loc::Indivisible => self.indivisible,
            Loc::Rename => self.rename,
            Loc::Target => self.target,
            Loc::TargetAt(i) => self.target_at[i],
            Loc::NewName => self.new_name,
            Loc::Script => self.script,
        }
    }
}

fn dec_bytes_list(d: &mut Decoder<'_>, offsets: &mut Vec<usize>) -> Result<Vec<Vec<u8>>> {
    dec_list(d, Some(offsets), |d| Ok(d.bytes()?.to_vec()))
}

/// The request, its unknown ignorable sections and its content id.
pub(super) fn decode(bytes: &[u8]) -> Result<(Transform, Vec<Extra<'_>>, Digest)> {
    let opened = open(bytes, Transform::KIND, Transform::VERSION, [TRANSFORM])?;
    let [section] = opened.sections;
    let mut d = super::required(section, opened.end, "transform request has no transform section")?;
    let mut o = Offsets::default();
    let kind = dec_enum(&mut d)?;
    o.owned = d.offset();
    let owned = dec_bytes_list(&mut d, &mut o.owned_at)?;
    let readonly = dec_bytes_list(&mut d, &mut o.readonly_at)?;
    o.indivisible = d.offset();
    let indivisible = d.bool()?;
    o.rename = d.offset();
    let rename = dec_opt(&mut d, |d| {
        o.target = d.offset();
        let target = dec_bytes_list(d, &mut o.target_at)?;
        let kind = dec_enum(d)?;
        o.new_name = d.offset();
        Ok(Rename { target, kind, new_name: d.bytes()?.to_vec() })
    })?;
    o.script = d.offset();
    let script = dec_opt(&mut d, |d| {
        let id = d.digest()?;
        let file = d.bytes()?.to_vec();
        let options = dec_list(d, None, |d| Ok(d.bytes()?.to_vec()))?;
        Ok(Script { id, file, options, steps: d.uv()?, bytes: d.uv()?, depth: d.uv()? })
    })?;
    d.finish()?;
    let t =
        Transform { kind, ownership: Ownership { owned, readonly }, indivisible, rename, script };
    if let Some((l, what)) = violation(&t) {
        return Err(malformed(o.of(l), what));
    }
    Ok((t, opened.extras, opened.content_id))
}

// ---------------------------------------------------------------------------
// JSON

const NAMES: [SectionName; 1] = [SectionName { id: TRANSFORM, name: "transform", required: true }];

fn json_list(list: &[Vec<u8>]) -> Value {
    Value::Array(list.iter().map(|b| json_bytes(b)).collect())
}

pub(super) fn render(t: &Transform, extras: &[Extra<'_>]) -> Value {
    let transform = object(vec![
        ("kind", json_enum(t.kind)),
        (
            "ownership",
            object(vec![
                ("owned", json_list(&t.ownership.owned)),
                ("readonly", json_list(&t.ownership.readonly)),
            ]),
        ),
        ("indivisible", Value::Bool(t.indivisible)),
        (
            "rename",
            json_opt(t.rename.as_ref(), |r| {
                object(vec![
                    ("target", json_list(&r.target)),
                    ("kind", json_enum(r.kind)),
                    ("new_name", json_bytes(&r.new_name)),
                ])
            }),
        ),
        (
            "script",
            json_opt(t.script.as_ref(), |s| {
                object(vec![
                    ("id", json_digest(&s.id)),
                    ("file", json_bytes(&s.file)),
                    ("options", json_list(&s.options)),
                    ("steps", json_uv(s.steps)),
                    ("bytes", json_uv(s.bytes)),
                    ("depth", json_uv(s.depth)),
                ])
            }),
        ),
    ]);
    render_container(Transform::KIND, Transform::VERSION, vec![("transform", transform)], extras)
}

/// The request and the unknown ignorable sections of a JSON rendering.
pub(super) fn read(v: &Value) -> JResult<(Transform, OwnedExtras)> {
    let s = read_container(v, Transform::KIND, Transform::VERSION, &NAMES)?;
    let [Some(section)] = s.known else {
        unreachable!("read_container returns every required section")
    };
    let root = At::Root;
    let sections = root.key("sections");
    let at = sections.key("transform");
    let [kind, ownership, indivisible, rename, script] =
        read_record(section, &at, ["kind", "ownership", "indivisible", "rename", "script"])?;
    let kind = read_enum(kind, &at.key("kind"))?;
    let o_at = at.key("ownership");
    let [owned, readonly] = read_record(ownership, &o_at, ["owned", "readonly"])?;
    let ownership = Ownership {
        owned: read_list(owned, &o_at.key("owned"), read_bytes)?,
        readonly: read_list(readonly, &o_at.key("readonly"), read_bytes)?,
    };
    let indivisible = read_bool(indivisible, &at.key("indivisible"))?;
    let rename = read_opt(rename, &at.key("rename"), |v, at| {
        let [target, kind, new_name] = read_record(v, at, ["target", "kind", "new_name"])?;
        Ok(Rename {
            target: read_list(target, &at.key("target"), read_bytes)?,
            kind: read_enum(kind, &at.key("kind"))?,
            new_name: read_bytes(new_name, &at.key("new_name"))?,
        })
    })?;
    let script = read_opt(script, &at.key("script"), |v, at| {
        let [id, file, options, steps, bytes, depth] =
            read_record(v, at, ["id", "file", "options", "steps", "bytes", "depth"])?;
        Ok(Script {
            id: read_digest(id, &at.key("id"))?,
            file: read_bytes(file, &at.key("file"))?,
            options: read_list(options, &at.key("options"), read_bytes)?,
            steps: read_uv(steps, &at.key("steps"))?,
            bytes: read_uv(bytes, &at.key("bytes"))?,
            depth: read_uv(depth, &at.key("depth"))?,
        })
    })?;
    let t = Transform { kind, ownership, indivisible, rename, script };
    if let Some((l, what)) = violation(&t) {
        return Err(JsonFormError { path: l.path(), message: what.to_owned() });
    }
    Ok((t, s.extras))
}
