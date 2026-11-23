//! The rule table of a case of `tests/rules.rs` (WP-103), from its
//! `rules.txt` and the unit records of its two sides: a second
//! implementation, written from `spec/verify.md` §2.1 and `spec/digest.md`
//! §3.4, of what `slip declare` does for its `relocate` lines
//! (`crates/slip_cli/src/declare.rs`), and a form of its own for rules no
//! declared change would produce. Pure over records, so that
//! `crates/slip_cli/tests/declare.rs` can include this file and compare
//! the two implementations' tables.
//!
//! # `rules.txt`
//!
//! UTF-8. A line that is blank or whose first character after blanks is
//! `#` is none. Every other line is one rule, in one of two forms; its
//! number (from 1) is how `expect.txt` names it (`use` lines).
//!
//! ```text
//! relocate <entity> => <parent> :: <name>
//! rule <kind> | <parent'> | <name'> | <disc'> | <parent'> | <name'>
//! ```
//!
//! **`relocate`** says what a declared relocation says (`spec/verify.md`
//! §2.3, the same text): the entity `<entity>` of the pre-image is, in the
//! post-image, the member `<name>` of `<parent>`. `<entity>` is `[<kind>]
//! <text>` or `<text>`: the unit's name as `spec/verify.md` §6.1 prints
//! it, taken literally, with a key kind (`digest-kinds.md` §3.1) for a
//! name units of two kinds share. The right side is split at its last
//! `::`; `<name>` is an identifier or `=` (the name it has), `<parent>`
//! an `<entity>` of the post-image or empty (the global namespace).
//! Let `K` be the `Key` of `<entity>` in the pre-image and `P'` the new
//! parent (`root`, or `unit` of the post-image id of `<parent>`). The line
//! is the rule that undoes the relocation (§2.1):
//!
//! ```text
//! Rule { parent: m(P'), kind: K.kind, name: N', disc: K.disc, to_parent: K.parent, to_name: K.name }
//! ```
//!
//! where `m` is the mapping of §2.1 under **the whole table**: `m(root)`,
//! `m(c_linkage)`, `m(builtin)` are themselves; `m(unit(id))` takes the
//! post-image's `Key` `k` with that id, maps its parent by `m`, applies
//! the table (`RuleTable::map`), and is `unit` of the result's id. The
//! table is found as a fixed point (every `m` computed under the table of
//! the previous round, from the table whose parents are unmapped, until
//! no rule changes), so the order of the lines does not matter: unlike
//! `slip declare`, which maps under the lines above only. A discriminator
//! on the way is not mapped (a record holds it as a digest).
//!
//! **`rule`** is a rule written field by field, for a rule no declared
//! change gives (a wrong one, a chained one, one that matches nothing):
//!
//! - `<kind>`: a key kind's name (`function`).
//! - a parent: `root`, `c_linkage`, `builtin`, `pre <entity>` (the
//!   entity's id in the pre-image), `post <entity>` (in the post-image,
//!   unmapped), `mapped <entity>` (`m` of its post-image id, as above).
//! - a name: `id <identifier>`, `name-of pre <entity>`, `name-of post
//!   <entity>` (the `Name` of that entity's key).
//! - the discriminator: `none`, `disc-of pre <entity>`, `disc-of post
//!   <entity>`.
//!
//! Names resolve as `spec/verify.md` §2.3 "Records" says: to the one
//! `KeyId` every record of that name (and kind) has. No record, two ids,
//! or a fallback key is an error of the case; so is a table
//! `RuleTable::new` refuses.

use slip_digest::records::Record;
use slip_model::keys::{Key, KeyKind, Name, Parent, Rule, RuleTable};

/// Which image an entity is resolved in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Image {
    Pre,
    Post,
}

/// An entity as a line names it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entity {
    pub kind: Option<KeyKind>,
    pub text: String,
}

/// A parent of a `rule` line, or the new parent of a `relocate` line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParentRef {
    Root,
    CLinkage,
    Builtin,
    /// The entity's id in that image, no rules.
    Of(Image, Entity),
    /// `m` of the entity's post-image id.
    Mapped(Entity),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NameRef {
    Id(String),
    Of(Image, Entity),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DiscRef {
    None,
    Of(Image, Entity),
}

/// One line of `rules.txt`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Line {
    /// `<entity> => <parent> :: <name>`; `parent` `None` is the global
    /// namespace, `name` `None` is `=`.
    Relocate { entity: Entity, parent: Option<Entity>, name: Option<String> },
    Rule {
        kind: KeyKind,
        parent: ParentRef,
        name: NameRef,
        disc: DiscRef,
        to_parent: ParentRef,
        to_name: NameRef,
    },
}

/// The lines of `text`, each with its number (from 1).
pub fn read(text: &str) -> Result<Vec<(usize, Line)>, String> {
    let mut out = Vec::new();
    for (i, raw) in text.lines().enumerate() {
        let n = i + 1;
        let line = raw.trim_matches(|c| c == ' ' || c == '\t' || c == '\r');
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let err = |what: &str| format!("rules.txt:{n}: {what}: `{line}`");
        let (word, rest) = line.split_once([' ', '\t']).ok_or_else(|| err("no entity"))?;
        let rest = rest.trim();
        let parsed = match word {
            "relocate" => relocate(rest).map_err(|e| err(&e))?,
            "rule" => rule(rest).map_err(|e| err(&e))?,
            _ => return Err(err("unknown keyword")),
        };
        out.push((n, parsed));
    }
    Ok(out)
}

fn kind_named(name: &str) -> Option<KeyKind> {
    KeyKind::ALL.into_iter().find(|k| k.name() == name)
}

fn is_identifier(s: &str) -> bool {
    let mut b = s.bytes();
    b.next().is_some_and(|c| c.is_ascii_alphabetic() || c == b'_')
        && b.all(|c| c.is_ascii_alphanumeric() || c == b'_')
}

/// `[<kind>] <text>` or `<text>`.
fn entity(s: &str) -> Result<Entity, String> {
    let s = s.trim();
    if let Some(rest) = s.strip_prefix('[') {
        let (kind, text) = rest.split_once(']').ok_or("an unclosed `[`")?;
        let kind = kind_named(kind.trim()).ok_or_else(|| format!("no key kind `{kind}`"))?;
        let text = text.trim();
        if text.is_empty() {
            return Err("an entity without a name".into());
        }
        return Ok(Entity { kind: Some(kind), text: text.to_owned() });
    }
    if s.is_empty() {
        return Err("an entity without a name".into());
    }
    Ok(Entity { kind: None, text: s.to_owned() })
}

fn relocate(rest: &str) -> Result<Line, String> {
    let (left, right) = rest.split_once(" => ").ok_or("no ` => `")?;
    let (parent, name) = right.rsplit_once("::").ok_or("no `::` on the right")?;
    let (parent, name) = (parent.trim(), name.trim());
    let name = match name {
        "=" => None,
        n if is_identifier(n) => Some(n.to_owned()),
        n => return Err(format!("`{n}` is neither an identifier nor `=`")),
    };
    let parent = if parent.is_empty() { None } else { Some(entity(parent)?) };
    Ok(Line::Relocate { entity: entity(left)?, parent, name })
}

fn image(s: &str) -> Result<(Image, Entity), String> {
    let (word, rest) = s.split_once(' ').ok_or_else(|| format!("`{s}`: no image"))?;
    let image = match word {
        "pre" => Image::Pre,
        "post" => Image::Post,
        _ => return Err(format!("`{word}` is neither `pre` nor `post`")),
    };
    Ok((image, entity(rest)?))
}

fn parent_ref(s: &str) -> Result<ParentRef, String> {
    match s {
        "root" => return Ok(ParentRef::Root),
        "c_linkage" => return Ok(ParentRef::CLinkage),
        "builtin" => return Ok(ParentRef::Builtin),
        _ => {}
    }
    if let Some(rest) = s.strip_prefix("mapped ") {
        return Ok(ParentRef::Mapped(entity(rest)?));
    }
    let (i, e) = image(s)?;
    Ok(ParentRef::Of(i, e))
}

fn name_ref(s: &str) -> Result<NameRef, String> {
    if let Some(id) = s.strip_prefix("id ") {
        let id = id.trim();
        return if is_identifier(id) {
            Ok(NameRef::Id(id.to_owned()))
        } else {
            Err(format!("`{id}` is no identifier"))
        };
    }
    let rest = s.strip_prefix("name-of ").ok_or_else(|| format!("`{s}`: not a name"))?;
    let (i, e) = image(rest)?;
    Ok(NameRef::Of(i, e))
}

fn disc_ref(s: &str) -> Result<DiscRef, String> {
    if s == "none" {
        return Ok(DiscRef::None);
    }
    let rest = s.strip_prefix("disc-of ").ok_or_else(|| format!("`{s}`: not a discriminator"))?;
    let (i, e) = image(rest)?;
    Ok(DiscRef::Of(i, e))
}

fn rule(rest: &str) -> Result<Line, String> {
    let (kind, fields) = rest.split_once(' ').ok_or("no fields")?;
    let kind = kind_named(kind).ok_or_else(|| format!("no key kind `{kind}`"))?;
    let f: Vec<&str> = fields
        .trim()
        .strip_prefix('|')
        .ok_or("no `|` after the kind")?
        .split('|')
        .map(str::trim)
        .collect();
    let [parent, name, disc, to_parent, to_name] = f.as_slice() else {
        return Err(format!("{} fields, not 5", f.len()));
    };
    Ok(Line::Rule {
        kind,
        parent: parent_ref(parent)?,
        name: name_ref(name)?,
        disc: disc_ref(disc)?,
        to_parent: parent_ref(to_parent)?,
        to_name: name_ref(to_name)?,
    })
}

/// The `Key` an entity resolves to in `records` (`spec/verify.md` §2.3
/// "Records"): the one id every record of its name and kind has.
pub fn resolve<'r>(records: &'r [Record], e: &Entity) -> Result<&'r Key, String> {
    let mut found: Option<&Record> = None;
    for r in records {
        if r.name != e.text || e.kind.is_some_and(|k| k != r.kind) {
            continue;
        }
        match found {
            Some(f) if f.id != r.id => {
                return Err(format!(
                    "`{}` is ambiguous: kinds {} and {}",
                    e.text,
                    f.kind.name(),
                    r.kind.name()
                ));
            }
            Some(_) => {}
            None => found = Some(r),
        }
    }
    let r = found.ok_or_else(|| format!("no unit named `{}`", e.text))?;
    r.key.as_ref().ok_or_else(|| format!("`{}` has a fallback key", e.text))
}

/// The table of a case's lines.
pub struct Table {
    pub table: RuleTable,
    /// The rule of each line, by line number, in the lines' order.
    pub rules: Vec<(usize, Rule)>,
}

/// `m` of §2.1 for `p`, under `table`, over the post-image's records.
fn mapped(p: Parent, table: &RuleTable, post: &[Record], depth: u32) -> Result<Parent, String> {
    let Parent::Unit(id) = p else { return Ok(p) };
    if depth > 256 {
        return Err("a parent chain deeper than 256".into());
    }
    let k = post
        .iter()
        .find(|r| r.id == id)
        .and_then(|r| r.key.clone())
        .ok_or_else(|| format!("no post-image record with id {id}"))?;
    let k = Key { parent: mapped(k.parent, table, post, depth + 1)?, ..k };
    Ok(Parent::Unit(table.map(k).id()))
}

fn image_records<'r>(i: Image, pre: &'r [Record], post: &'r [Record]) -> &'r [Record] {
    match i {
        Image::Pre => pre,
        Image::Post => post,
    }
}

/// The rule of one line under `table` (the previous round's).
fn rule_of(
    line: &Line,
    table: &RuleTable,
    pre: &[Record],
    post: &[Record],
) -> Result<Rule, String> {
    let parent = |p: &ParentRef| -> Result<Parent, String> {
        Ok(match p {
            ParentRef::Root => Parent::Root,
            ParentRef::CLinkage => Parent::CLinkage,
            ParentRef::Builtin => Parent::Builtin,
            ParentRef::Of(i, e) => Parent::Unit(resolve(image_records(*i, pre, post), e)?.id()),
            ParentRef::Mapped(e) => mapped(Parent::Unit(resolve(post, e)?.id()), table, post, 0)?,
        })
    };
    let name = |n: &NameRef| -> Result<Name, String> {
        Ok(match n {
            NameRef::Id(s) => Name::Identifier(s.as_bytes().to_vec()),
            NameRef::Of(i, e) => resolve(image_records(*i, pre, post), e)?.name.clone(),
        })
    };
    match line {
        Line::Relocate { entity, parent: new_parent, name: new_name } => {
            let k = resolve(pre, entity)?;
            let p = match new_parent {
                None => Parent::Root,
                Some(e) => mapped(Parent::Unit(resolve(post, e)?.id()), table, post, 0)?,
            };
            let n = match new_name {
                None => k.name.clone(),
                Some(s) => {
                    if !matches!(k.name, Name::Identifier(_)) {
                        return Err(format!("`{}`: its name is no identifier", entity.text));
                    }
                    Name::Identifier(s.as_bytes().to_vec())
                }
            };
            Ok(Rule {
                parent: p,
                kind: k.kind,
                name: n,
                disc: k.disc,
                to_parent: k.parent,
                to_name: k.name.clone(),
            })
        }
        Line::Rule { kind, parent: p, name: n, disc, to_parent, to_name } => Ok(Rule {
            parent: parent(p)?,
            kind: *kind,
            name: name(n)?,
            disc: match disc {
                DiscRef::None => None,
                DiscRef::Of(i, e) => resolve(image_records(*i, pre, post), e)?.disc,
            },
            to_parent: parent(to_parent)?,
            to_name: name(to_name)?,
        }),
    }
}

/// The table of `lines` over the records of the two images (no rules):
/// the fixed point described at the head of this file.
pub fn table(lines: &[(usize, Line)], pre: &[Record], post: &[Record]) -> Result<Table, String> {
    let mut table = RuleTable::empty();
    let mut last: Option<Vec<(usize, Rule)>> = None;
    for _ in 0..=lines.len() + 1 {
        let mut rules = Vec::with_capacity(lines.len());
        for (n, line) in lines {
            rules.push((
                *n,
                rule_of(line, &table, pre, post).map_err(|e| format!("rules.txt:{n}: {e}"))?,
            ));
        }
        table = RuleTable::new(rules.iter().map(|(_, r)| r.clone()).collect())
            .map_err(|e| format!("rules.txt: not a table: {e}"))?;
        if last.as_ref() == Some(&rules) {
            return Ok(Table { table, rules });
        }
        last = Some(rules);
    }
    Err("rules.txt: the parents of the rules do not settle".into())
}
