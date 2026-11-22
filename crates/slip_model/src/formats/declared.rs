//! Declared changes, kind `DECL` (`spec/verify.md` §2). Work package
//! WP-022; the encodings, the JSON forms, the order of checks, offsets and
//! paths are `spec/landed/model/WP-022.md`.

use crate::Digest;
use crate::declared::{Declared, first_not_ascending};
use crate::json::Value;
use crate::keys::{
    DUPLICATE_RULE, KEY_KINDS, KeyKind, NAME_FORMS, Name, OPERATOR_ZERO, Parent, Rule, RuleTable,
    SortError, TOO_DEEP, UNNAMED_BY_MAX, UNSORTED_RULES,
};
use crate::wire::{Decoder, Encoder};

use super::common::{
    Enumeration, JResult, Result, dec_list, json_bytes, json_digest, json_enum, json_opt, json_uv,
    malformed, object, read_bytes, read_digest, read_enum, read_list, read_opt, read_record,
    read_uv,
};
use super::{At, Extra, OwnedExtras, SectionName, open, read_container, render_container, seal};

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
const RULE_FIELDS: [&str; 6] = ["parent", "kind", "name", "disc", "to_parent", "to_name"];

fn sort_what(e: SortError) -> &'static str {
    match e {
        SortError::Invalid(what) => what,
        SortError::Duplicate => DUPLICATE_RULE,
        SortError::Unsorted => UNSORTED_RULES,
    }
}

/// Every `Declared` is valid: its constructors check the lists.
#[allow(clippy::unnecessary_wraps)]
pub(super) fn check(_: &Declared) -> std::result::Result<(), (String, &'static str)> {
    Ok(())
}

// ---------------------------------------------------------------------------
// Binary

pub(super) fn encode(x: &Declared, extras: &[Extra<'_>]) -> Vec<u8> {
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
    seal(Declared::KIND, Declared::VERSION, &[(DECLARED, e.as_bytes())], extras)
}

/// The declared changes, the unknown ignorable sections and the content id.
/// The section's fields are read first, then the order of each list is
/// checked in field order (`spec/model.md` §5.4 steps 3 and 4).
pub(super) fn decode(bytes: &[u8]) -> Result<(Declared, Vec<Extra<'_>>, Digest)> {
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
    d.finish()?;

    let rules =
        RuleTable::from_sorted(rules).map_err(|(i, e)| malformed(rule_at[i], sort_what(e)))?;
    for (k, list) in [&changed, &created, &removed].into_iter().enumerate() {
        if let Some(i) = first_not_ascending(list) {
            return Err(malformed(list_at[k][i], LISTS[k].1));
        }
    }
    Ok((Declared::from_checked(rules, changed, created, removed), opened.extras, opened.content_id))
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
    let declared = object(
        FIELDS
            .into_iter()
            .zip([
                Value::Array(x.rules().rules().iter().map(json_rule).collect()),
                json_keys(x.changed()),
                json_keys(x.created()),
                json_keys(x.removed()),
            ])
            .collect(),
    );
    render_container(Declared::KIND, Declared::VERSION, vec![("declared", declared)], extras)
}

/// The declared changes and the unknown ignorable sections of a JSON
/// rendering. The same checks as the binary reader, in the same order,
/// with the same messages, at JSON paths.
pub(super) fn read(v: &Value) -> JResult<(Declared, OwnedExtras)> {
    let s = read_container(v, Declared::KIND, Declared::VERSION, &NAMES)?;
    let [Some(section)] = s.known else {
        unreachable!("read_container returns every required section")
    };
    let root = At::Root;
    let sections = root.key("sections");
    let at = sections.key("declared");
    let [rules, changed, created, removed] = read_record(section, &at, FIELDS)?;
    let rules_at = at.key("rules");
    let rules = read_list(rules, &rules_at, read_rule)?;
    let lists_at = LISTS.map(|(name, _)| at.key(name));
    let changed = read_list(changed, &lists_at[0], read_digest)?;
    let created = read_list(created, &lists_at[1], read_digest)?;
    let removed = read_list(removed, &lists_at[2], read_digest)?;

    let rules =
        RuleTable::from_sorted(rules).map_err(|(i, e)| rules_at.index(i).error(sort_what(e)))?;
    for (k, list) in [&changed, &created, &removed].into_iter().enumerate() {
        if let Some(i) = first_not_ascending(list) {
            return Err(lists_at[k].index(i).error(LISTS[k].1));
        }
    }
    Ok((Declared::from_checked(rules, changed, created, removed), s.extras))
}
