//! Declared changes, kind `DECL` (`spec/verify.md` §2). Work package
//! WP-022; the encodings, the JSON forms, the order of checks, offsets and
//! paths are `spec/landed/model/WP-022.md`. Version 2 (`kept`, `files`)
//! and its checks are WP-110's (`spec/verify.md` §2, "The reader's
//! checks"; `spec/landed/digest/WP-110.md`).

use crate::Digest;
use crate::declared::{Declared, first_not_ascending};
use crate::json::Value;
use crate::keys::{
    DUPLICATE_RULE, KEY_KINDS, KeyKind, NAME_FORMS, Name, OPERATOR_ZERO, Parent, Rule, RuleTable,
    SortError, TOO_DEEP, UNNAMED_BY_MAX, UNSORTED_RULES,
};
use crate::transform::{Ownership, is_canonical};
use crate::wire::{Decoder, Encoder};

use super::common::{
    Enumeration, JResult, Result, dec_list, dec_opt, json_bytes, json_digest, json_enum, json_opt,
    json_uv, malformed, object, read_bytes, read_digest, read_enum, read_list, read_opt,
    read_record, read_uv,
};
use super::{
    At, Extra, JsonFormError, OwnedExtras, SectionName, open, read_container, render_container,
    seal,
};

/// The section id.
const DECLARED: u64 = 1;

impl Enumeration for KeyKind {
    const TABLE: &'static [(Self, u64, &'static str)] = &KEY_KINDS;
    const UNLISTED: &'static str = "unlisted KeyKind value";
    const EXPECTED: &'static str = "expected a KeyKind name of digest-kinds §3.1";
}

/// The exemption lists, in field order, with the message for each one
/// that is not strictly ascending.
const LISTS: [(&str, &str); 3] = [
    ("changed", "changed is not strictly ascending"),
    ("created", "created is not strictly ascending"),
    ("removed", "removed is not strictly ascending"),
];

const FIELDS: [&str; 4] = ["rules", "changed", "created", "removed"];
const FIELDS_V2: [&str; 6] = ["rules", "changed", "created", "removed", "kept", "files"];
const RULE_FIELDS: [&str; 6] = ["parent", "kind", "name", "disc", "to_parent", "to_name"];

fn sort_what(e: SortError) -> &'static str {
    match e {
        SortError::Invalid(what) => what,
        SortError::Duplicate => DUPLICATE_RULE,
        SortError::Unsorted => UNSORTED_RULES,
    }
}

/// Where a check of [`violation`] failed.
#[derive(Clone, Copy, Debug)]
enum Loc {
    /// The first rule.
    FirstRule,
    KeptAt(usize),
    /// The `files` field.
    Files,
    /// `files.owned`.
    Owned,
    OwnedAt(usize),
    ReadonlyAt(usize),
}

impl Loc {
    fn path(self) -> String {
        const S: &str = "sections.declared";
        match self {
            Loc::FirstRule => format!("{S}.rules[0]"),
            Loc::KeptAt(i) => format!("{S}.kept[{i}]"),
            Loc::Files => format!("{S}.files"),
            Loc::Owned => format!("{S}.files.owned"),
            Loc::OwnedAt(i) => format!("{S}.files.owned[{i}]"),
            Loc::ReadonlyAt(i) => format!("{S}.files.readonly[{i}]"),
        }
    }
}

pub(super) const KEPT_UNSORTED: &str = "kept is not strictly ascending";
pub(super) const OWNED_EMPTY: &str = "owned is empty";
pub(super) const NOT_CANONICAL: &str = "not a canonical absolute path";
pub(super) const OWNED_UNSORTED: &str = "owned is not strictly ascending";
pub(super) const READONLY_UNSORTED: &str = "readonly is not strictly ascending";
pub(super) const READONLY_OWNED: &str = "a readonly entry is also in owned";
pub(super) const V1_RULE: &str = "a version 1 container has a rule";
pub(super) const RULES_NO_FILES: &str = "rules without files";
pub(super) const FILES_NO_RULES: &str = "files without a rule";

fn paths_not_ascending(list: &[Vec<u8>]) -> Option<usize> {
    (1..list.len()).find(|&i| list[i - 1] >= list[i])
}

/// The checks after the four lists of version 1 (theirs are the
/// constructors' and the readers'), in order (`spec/verify.md` §2, "The
/// reader's checks", rows 3 to 6): `kept`; `files`, as `Transform`'s
/// ownership (`spec/edits.md` §2.1); a rule in a version 1 container;
/// `files` present exactly when the rules are not empty.
fn violation(x: &Declared, version: u32) -> Option<(Loc, &'static str)> {
    if let Some(i) = first_not_ascending(x.kept()) {
        return Some((Loc::KeptAt(i), KEPT_UNSORTED));
    }
    if let Some(o) = x.files()
        && let Some(v) = ownership_violation(o)
    {
        return Some(v);
    }
    let rules = !x.rules().rules().is_empty();
    if version == 1 && rules {
        return Some((Loc::FirstRule, V1_RULE));
    }
    match (rules, x.files().is_some()) {
        (true, false) => Some((Loc::Files, RULES_NO_FILES)),
        (false, true) => Some((Loc::Files, FILES_NO_RULES)),
        _ => None,
    }
}

/// `Transform`'s checks of an ownership, in its order and with its
/// messages (`spec/landed/edits/WP-060.md` §3).
fn ownership_violation(o: &Ownership) -> Option<(Loc, &'static str)> {
    if o.owned.is_empty() {
        return Some((Loc::Owned, OWNED_EMPTY));
    }
    if let Some(i) = o.owned.iter().position(|e| !is_canonical(e)) {
        return Some((Loc::OwnedAt(i), NOT_CANONICAL));
    }
    if let Some(i) = paths_not_ascending(&o.owned) {
        return Some((Loc::OwnedAt(i), OWNED_UNSORTED));
    }
    if let Some(i) = o.readonly.iter().position(|e| !is_canonical(e)) {
        return Some((Loc::ReadonlyAt(i), NOT_CANONICAL));
    }
    if let Some(i) = paths_not_ascending(&o.readonly) {
        return Some((Loc::ReadonlyAt(i), READONLY_UNSORTED));
    }
    if let Some(i) = o.readonly.iter().position(|r| o.owned.binary_search(r).is_ok()) {
        return Some((Loc::ReadonlyAt(i), READONLY_OWNED));
    }
    None
}

/// The four lists are checked by the constructors; what is left is
/// [`violation`], as a value written now (version 2) is read.
pub(super) fn check(x: &Declared) -> std::result::Result<(), (String, &'static str)> {
    violation(x, Declared::VERSION).map_or(Ok(()), |(l, what)| Err((l.path(), what)))
}

// ---------------------------------------------------------------------------
// Binary

pub(super) fn encode(x: &Declared, extras: &[Extra<'_>]) -> Vec<u8> {
    encode_as(x, extras, Declared::VERSION)
}

/// The file in `version`'s layout: version 1 has no `kept` and no `files`
/// (a value read from a version 1 file has neither), so that `undump` of
/// a version 1 file's `dump` gives its bytes back.
pub(super) fn encode_as(x: &Declared, extras: &[Extra<'_>], version: u32) -> Vec<u8> {
    let mut e = Encoder::new();
    let rules = x.rules().rules();
    e.count(rules.len());
    for rule in rules {
        rule.encode(&mut e);
    }
    for list in [x.changed(), x.created(), x.removed()] {
        e.count(list.len());
        for key in list {
            e.digest(key);
        }
    }
    if version >= 2 {
        e.count(x.kept().len());
        for key in x.kept() {
            e.digest(key);
        }
        match x.files() {
            None => {
                e.u8(0);
            }
            Some(o) => {
                e.u8(1);
                for list in [&o.owned, &o.readonly] {
                    e.count(list.len());
                    for path in list {
                        e.bytes(path);
                    }
                }
            }
        }
    }
    seal(Declared::KIND, version, &[(DECLARED, e.as_bytes())], extras)
}

/// The declared changes, the unknown ignorable sections and the content id.
/// The section's fields are read first, then the order of each list is
/// checked in field order (`spec/model.md` §5.4 steps 3 and 4).
pub(super) fn decode(bytes: &[u8]) -> Result<(Declared, Vec<Extra<'_>>, Digest)> {
    decode_versioned(bytes).map(|(x, extras, id, _)| (x, extras, id))
}

/// [`decode`], with the file's version.
pub(super) fn decode_versioned(bytes: &[u8]) -> Result<(Declared, Vec<Extra<'_>>, Digest, u32)> {
    let opened = open(bytes, Declared::KIND, Declared::VERSION, [DECLARED])?;
    let [section] = opened.sections;
    let mut d = super::required(section, opened.end, "declared changes have no declared section")?;
    let mut rule_at = Vec::new();
    let rules = dec_list(&mut d, Some(&mut rule_at), Rule::decode)?;
    let mut list_at: [Vec<usize>; 3] = Default::default();
    let [a0, a1, a2] = &mut list_at;
    let changed = dec_list(&mut d, Some(a0), Decoder::digest)?;
    let created = dec_list(&mut d, Some(a1), Decoder::digest)?;
    let removed = dec_list(&mut d, Some(a2), Decoder::digest)?;
    let mut o = Offsets { files: opened.end, ..Offsets::default() };
    let (kept, files) = if opened.version >= 2 {
        let kept = dec_list(&mut d, Some(&mut o.kept_at), Decoder::digest)?;
        o.files = d.offset();
        let files = dec_opt(&mut d, |d| {
            o.owned = d.offset();
            let owned = dec_list(d, Some(&mut o.owned_at), |d| Ok(d.bytes()?.to_vec()))?;
            let readonly = dec_list(d, Some(&mut o.readonly_at), |d| Ok(d.bytes()?.to_vec()))?;
            Ok(Ownership { owned, readonly })
        })?;
        (kept, files)
    } else {
        (Vec::new(), None)
    };
    d.finish()?;

    let rules =
        RuleTable::from_sorted(rules).map_err(|(i, e)| malformed(rule_at[i], sort_what(e)))?;
    for (k, list) in [&changed, &created, &removed].into_iter().enumerate() {
        if let Some(i) = first_not_ascending(list) {
            return Err(malformed(list_at[k][i], LISTS[k].1));
        }
    }
    o.first_rule = rule_at.first().copied().unwrap_or(opened.end);
    let x = Declared::from_checked(rules, [changed, created, removed, kept], files);
    if let Some((l, what)) = violation(&x, opened.version) {
        return Err(malformed(o.of(l), what));
    }
    Ok((x, opened.extras, opened.content_id, opened.version))
}

/// The offsets [`violation`]'s locations name in a binary file.
#[derive(Default)]
struct Offsets {
    first_rule: usize,
    kept_at: Vec<usize>,
    files: usize,
    owned: usize,
    owned_at: Vec<usize>,
    readonly_at: Vec<usize>,
}

impl Offsets {
    fn of(&self, l: Loc) -> usize {
        match l {
            Loc::FirstRule => self.first_rule,
            Loc::KeptAt(i) => self.kept_at[i],
            Loc::Files => self.files,
            Loc::Owned => self.owned,
            Loc::OwnedAt(i) => self.owned_at[i],
            Loc::ReadonlyAt(i) => self.readonly_at[i],
        }
    }
}

// ---------------------------------------------------------------------------
// JSON

const NAMES: [SectionName; 1] = [SectionName { id: DECLARED, name: "declared", required: true }];

/// `"root"`, `"c_linkage"`, `"builtin"` or `{ "unit": <digest> }`.
pub(super) fn json_parent(p: &Parent) -> Value {
    match p {
        Parent::Root => Value::Str("root".to_owned()),
        Parent::CLinkage => Value::Str("c_linkage".to_owned()),
        Parent::Builtin => Value::Str("builtin".to_owned()),
        Parent::Unit(id) => object(vec![("unit", json_digest(id))]),
    }
}

pub(super) fn read_parent(v: &Value, at: &At<'_>) -> JResult<Parent> {
    match v {
        Value::Str(s) => match s.as_str() {
            "root" => Ok(Parent::Root),
            "c_linkage" => Ok(Parent::CLinkage),
            "builtin" => Ok(Parent::Builtin),
            "unit" => Err(at.error("a unit parent is written {\"unit\": <digest>}")),
            _ => Err(at.error(
                "expected one of \"root\", \"c_linkage\", \"builtin\", {\"unit\": <digest>}",
            )),
        },
        Value::Object(_) => {
            let [id] = read_record(v, at, ["unit"])?;
            Ok(Parent::Unit(read_digest(id, &at.key("unit"))?))
        }
        _ => Err(at.error("wrong type: expected a string or an object")),
    }
}

/// A form without a value is its name as a string (`"constructor"`); a
/// form with one is a one-member object (`{ "identifier": "f" }`).
pub(super) fn json_name(n: &Name) -> Value {
    let value = match n {
        Name::Identifier(b) | Name::LiteralOperator(b) => json_bytes(b),
        Name::Conversion(t) => json_digest(t),
        Name::Operator(x)
        | Name::Local(x)
        | Name::UnnamedOrdinal(x)
        | Name::Property(x)
        | Name::Parameter(x) => json_uv(*x),
        Name::UnnamedBy(inner) => json_name(inner),
        Name::Constructor
        | Name::Destructor
        | Name::DeductionGuide
        | Name::Anonymous
        | Name::Content
        | Name::Order => return Value::Str(n.form().to_owned()),
    };
    object(vec![(n.form(), value)])
}

fn has_value(form: &str) -> bool {
    matches!(
        form,
        "identifier"
            | "operator"
            | "conversion"
            | "literal_operator"
            | "local"
            | "unnamed_by"
            | "unnamed_ordinal"
            | "property"
            | "parameter"
    )
}

const NAME_EXPECTED: &str = "expected a Name form of digest-kinds §3.2";

pub(super) fn read_name(v: &Value, at: &At<'_>) -> JResult<Name> {
    read_name_at(v, at, 0)
}

/// A name inside `wrappers` `unnamed_by`.
fn read_name_at(v: &Value, at: &At<'_>, wrappers: usize) -> JResult<Name> {
    match v {
        Value::Str(s) => match s.as_str() {
            "constructor" => Ok(Name::Constructor),
            "destructor" => Ok(Name::Destructor),
            "deduction_guide" => Ok(Name::DeductionGuide),
            "anonymous" => Ok(Name::Anonymous),
            "content" => Ok(Name::Content),
            "order" => Ok(Name::Order),
            form if has_value(form) => {
                Err(at.error(format!("the form {form} is written {{\"{form}\": <value>}}")))
            }
            _ => Err(at.error(NAME_EXPECTED)),
        },
        Value::Object(members) => {
            let [(form, value)] = members.as_slice() else {
                return Err(at.error("a Name with a value is an object with exactly one member"));
            };
            let here = at.key(form);
            if !NAME_FORMS.contains(&form.as_str()) {
                return Err(here.error(NAME_EXPECTED));
            }
            if !has_value(form) {
                return Err(here.error(format!("the form {form} has no value: write \"{form}\"")));
            }
            Ok(match form.as_str() {
                "identifier" => Name::Identifier(read_bytes(value, &here)?),
                "operator" => match read_uv(value, &here)? {
                    0 => return Err(here.error(OPERATOR_ZERO)),
                    n => Name::Operator(n),
                },
                "conversion" => Name::Conversion(read_digest(value, &here)?),
                "literal_operator" => Name::LiteralOperator(read_bytes(value, &here)?),
                "local" => Name::Local(read_uv(value, &here)?),
                "unnamed_by" => {
                    if wrappers == UNNAMED_BY_MAX {
                        return Err(at.error(TOO_DEEP));
                    }
                    Name::UnnamedBy(Box::new(read_name_at(value, &here, wrappers + 1)?))
                }
                "unnamed_ordinal" => Name::UnnamedOrdinal(read_uv(value, &here)?),
                "property" => Name::Property(read_uv(value, &here)?),
                "parameter" => Name::Parameter(read_uv(value, &here)?),
                _ => unreachable!("has_value lists every form with a value"),
            })
        }
        _ => Err(at.error("wrong type: expected a string or an object")),
    }
}

pub(super) fn json_rule(r: &Rule) -> Value {
    object(
        RULE_FIELDS
            .into_iter()
            .zip([
                json_parent(&r.parent),
                json_enum(r.kind),
                json_name(&r.name),
                json_opt(r.disc.as_ref(), json_digest),
                json_parent(&r.to_parent),
                json_name(&r.to_name),
            ])
            .collect(),
    )
}

pub(super) fn read_rule(v: &Value, at: &At<'_>) -> JResult<Rule> {
    let [parent, kind, name, disc, to_parent, to_name] = read_record(v, at, RULE_FIELDS)?;
    Ok(Rule {
        parent: read_parent(parent, &at.key(RULE_FIELDS[0]))?,
        kind: read_enum(kind, &at.key(RULE_FIELDS[1]))?,
        name: read_name(name, &at.key(RULE_FIELDS[2]))?,
        disc: read_opt(disc, &at.key(RULE_FIELDS[3]), read_digest)?,
        to_parent: read_parent(to_parent, &at.key(RULE_FIELDS[4]))?,
        to_name: read_name(to_name, &at.key(RULE_FIELDS[5]))?,
    })
}

fn json_keys(list: &[Digest]) -> Value {
    Value::Array(list.iter().map(json_digest).collect())
}

pub(super) fn render(x: &Declared, extras: &[Extra<'_>]) -> Value {
    render_as(x, extras, Declared::VERSION)
}

/// The rendering in `version`'s layout (see [`encode_as`]).
pub(super) fn render_as(x: &Declared, extras: &[Extra<'_>], version: u32) -> Value {
    let mut declared: Vec<(&str, Value)> = FIELDS_V2
        .into_iter()
        .zip([
            Value::Array(x.rules().rules().iter().map(json_rule).collect()),
            json_keys(x.changed()),
            json_keys(x.created()),
            json_keys(x.removed()),
            json_keys(x.kept()),
            json_opt(x.files(), |o| {
                object(vec![
                    ("owned", Value::Array(o.owned.iter().map(|p| json_bytes(p)).collect())),
                    ("readonly", Value::Array(o.readonly.iter().map(|p| json_bytes(p)).collect())),
                ])
            }),
        ])
        .collect();
    if version < 2 {
        declared.truncate(FIELDS.len());
    }
    render_container(Declared::KIND, version, vec![("declared", object(declared))], extras)
}

/// The declared changes and the unknown ignorable sections of a JSON
/// rendering. The same checks as the binary reader, in the same order,
/// with the same messages, at JSON paths.
pub(super) fn read(v: &Value) -> JResult<(Declared, OwnedExtras)> {
    read_versioned(v).map(|(x, extras, _)| (x, extras))
}

/// [`read`], with the rendering's version.
pub(super) fn read_versioned(v: &Value) -> JResult<(Declared, OwnedExtras, u32)> {
    let s = read_container(v, Declared::KIND, Declared::VERSION, &NAMES)?;
    let [Some(section)] = s.known else {
        unreachable!("read_container returns every required section")
    };
    let root = At::Root;
    let sections = root.key("sections");
    let at = sections.key("declared");
    let (rules, changed, created, removed, v2) = if s.version >= 2 {
        let [rules, changed, created, removed, kept, files] = read_record(section, &at, FIELDS_V2)?;
        (rules, changed, created, removed, Some((kept, files)))
    } else {
        let [rules, changed, created, removed] = read_record(section, &at, FIELDS)?;
        (rules, changed, created, removed, None)
    };
    let rules_at = at.key("rules");
    let rules = read_list(rules, &rules_at, read_rule)?;
    let lists_at = LISTS.map(|(name, _)| at.key(name));
    let changed = read_list(changed, &lists_at[0], read_digest)?;
    let created = read_list(created, &lists_at[1], read_digest)?;
    let removed = read_list(removed, &lists_at[2], read_digest)?;
    let (kept, files) = match v2 {
        None => (Vec::new(), None),
        Some((kept, files)) => {
            let kept = read_list(kept, &at.key("kept"), read_digest)?;
            let files = read_opt(files, &at.key("files"), |v, at| {
                let [owned, readonly] = read_record(v, at, ["owned", "readonly"])?;
                Ok(Ownership {
                    owned: read_list(owned, &at.key("owned"), read_bytes)?,
                    readonly: read_list(readonly, &at.key("readonly"), read_bytes)?,
                })
            })?;
            (kept, files)
        }
    };

    let rules =
        RuleTable::from_sorted(rules).map_err(|(i, e)| rules_at.index(i).error(sort_what(e)))?;
    for (k, list) in [&changed, &created, &removed].into_iter().enumerate() {
        if let Some(i) = first_not_ascending(list) {
            return Err(lists_at[k].index(i).error(LISTS[k].1));
        }
    }
    let x = Declared::from_checked(rules, [changed, created, removed, kept], files);
    if let Some((l, what)) = violation(&x, s.version) {
        return Err(JsonFormError { path: l.path(), message: what.to_owned() });
    }
    Ok((x, s.extras, s.version))
}
