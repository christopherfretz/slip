//! Declared changes by name (`spec/verify.md` §2.3), the pure part of
//! `slip declare`: the text ([`read`]), which parses it needs ([`needs`]),
//! names resolved to keys and `relocate` lines turned into rules
//! ([`resolve`]), and the `Declared` ([`finish`]). Work package WP-055;
//! `main` is WP-053's.
//!
//! Every function is over records (`slip_digest::records`), not parses, so
//! that the resolution is tested on records written by hand. A name
//! resolves only when every unit of that name (and kind) has one `KeyId`:
//! nothing is chosen among candidates.

use std::collections::{BTreeSet, HashMap};
use std::fmt;

use slip_digest::records::Record;
use slip_model::Digest;
use slip_model::declared::Declared;
use slip_model::keys::{Key, KeyKind, Name, Parent, Rule, RuleTable};

/// An entity as a line names it: `[<kind>] <text>` or `<text>`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entity {
    pub kind: Option<KeyKind>,
    /// A unit's name as `spec/verify.md` §6.1 renders it, taken literally.
    pub text: String,
}

/// One line of the text that is not ignored.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Line {
    /// `relocate <entity> => <parent> :: <name>`. `parent` is `None` for
    /// the global namespace; `name` is `None` for `=`, the name it has.
    Relocate {
        entity: Entity,
        parent: Option<Entity>,
        name: Option<String>,
    },
    Changed(Entity),
    Created(Entity),
    Removed(Entity),
}

/// The errors of `spec/verify.md` §2.3.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ErrorKind {
    Syntax,
    NoUnit,
    Ambiguous,
    Fallback,
    NotRenamable,
    Duplicate,
}

impl ErrorKind {
    /// Its name in messages.
    pub fn name(self) -> &'static str {
        match self {
            ErrorKind::Syntax => "syntax",
            ErrorKind::NoUnit => "no unit",
            ErrorKind::Ambiguous => "ambiguous",
            ErrorKind::Fallback => "fallback key",
            ErrorKind::NotRenamable => "not renamable",
            ErrorKind::Duplicate => "duplicate",
        }
    }
}

/// An error and the line (from 1) it is about.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LineError {
    pub line: u64,
    pub kind: ErrorKind,
    pub detail: String,
}

impl fmt::Display for LineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {}: {}", self.line, self.kind.name(), self.detail)
    }
}

impl std::error::Error for LineError {}

fn error(line: u64, kind: ErrorKind, detail: impl Into<String>) -> LineError {
    LineError { line, kind, detail: detail.into() }
}

// ---------------------------------------------------------------------------
// The text

/// A blank of §2.3: a space or a tab.
fn is_blank(c: char) -> bool {
    c == ' ' || c == '\t'
}

fn trim_blanks(s: &str) -> &str {
    s.trim_matches(is_blank)
}

/// Whether `s` is an identifier of §2.3: `[A-Za-z_][A-Za-z0-9_]*`.
fn is_identifier(s: &str) -> bool {
    let mut b = s.bytes();
    b.next().is_some_and(|c| c.is_ascii_alphabetic() || c == b'_')
        && b.all(|c| c.is_ascii_alphanumeric() || c == b'_')
}

/// `[<kind>] <text>` or `<text>`, already trimmed; `Err` is the detail of
/// a `Syntax` error.
fn entity(s: &str) -> Result<Entity, String> {
    let (kind, text) = match s.strip_prefix('[') {
        Some(rest) => {
            let Some((name, text)) = rest.split_once(']') else {
                return Err(format!("`[` without `]` in `{s}`"));
            };
            let kind = KeyKind::ALL
                .into_iter()
                .find(|k| k.name() == name)
                .ok_or_else(|| format!("unknown kind `[{name}]`"))?;
            (Some(kind), trim_blanks(text))
        }
        None => (None, s),
    };
    if text.is_empty() {
        return Err("a missing entity".to_owned());
    }
    Ok(Entity { kind, text: text.to_owned() })
}

/// One line, `text` without its end of line; `Ok(None)` if it is ignored.
fn line(text: &str) -> Result<Option<Line>, String> {
    let text = text.trim_start_matches(is_blank);
    if text.is_empty() || text.starts_with('#') {
        return Ok(None);
    }
    let (keyword, rest) = match text.find(is_blank) {
        Some(i) => (&text[..i], trim_blanks(&text[i..])),
        None => (text, ""),
    };
    let one = |f: fn(Entity) -> Line| -> Result<Option<Line>, String> {
        if rest.is_empty() {
            return Err(format!("a missing entity after `{keyword}`"));
        }
        Ok(Some(f(entity(rest)?)))
    };
    match keyword {
        "changed" => one(Line::Changed),
        "created" => one(Line::Created),
        "removed" => one(Line::Removed),
        "relocate" => {
            if rest.is_empty() {
                return Err("a missing entity after `relocate`".to_owned());
            }
            let Some((left, right)) = rest.split_once(" => ") else {
                return Err("`relocate` without ` => `".to_owned());
            };
            let entity = entity(trim_blanks(left))?;
            let Some((parent, name)) = right.rsplit_once("::") else {
                return Err("`relocate` without `::` after ` => `".to_owned());
            };
            let (parent, name) = (trim_blanks(parent), trim_blanks(name));
            let name = if name == "=" {
                None
            } else if is_identifier(name) {
                Some(name.to_owned())
            } else {
                return Err(format!("`{name}` is neither an identifier nor `=`"));
            };
            let parent = if parent.is_empty() { None } else { Some(self::entity(parent)?) };
            Ok(Some(Line::Relocate { entity, parent, name }))
        }
        _ => Err(format!("unknown keyword `{keyword}`")),
    }
}

/// The lines of `text` that are not ignored, with their numbers (from 1);
/// `Err` is the first `Syntax` error. A byte order mark at the start is
/// skipped. Lines end at `\n`; a `\r` before it is the end of line too.
pub fn read(text: &str) -> Result<Vec<(u64, Line)>, LineError> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut out = Vec::new();
    for (i, raw) in text.split('\n').enumerate() {
        let n = i as u64 + 1;
        let raw = raw.strip_suffix('\r').unwrap_or(raw);
        if let Some(l) = line(raw).map_err(|e| error(n, ErrorKind::Syntax, e))? {
            out.push((n, l));
        }
    }
    Ok(out)
}

/// Which post-image parses the table of §2.3 asks for: `post`, the
/// post-images' records with no rules (a `relocate` with a non-empty
/// parent); `post_mapped`, under the rules the file gives (a `created`
/// line).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Needs {
    pub post: bool,
    pub post_mapped: bool,
}

pub fn needs(lines: &[(u64, Line)]) -> Needs {
    let post = lines.iter().any(|(_, l)| matches!(l, Line::Relocate { parent: Some(_), .. }));
    let post_mapped = lines.iter().any(|(_, l)| matches!(l, Line::Created(_)));
    Needs { post, post_mapped }
}

// ---------------------------------------------------------------------------
// Resolution

/// What the lines `relocate`, `changed` and `removed` give.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Resolved {
    pub rules: RuleTable,
    /// Sorted, no two equal.
    pub changed: Vec<Digest>,
    pub removed: Vec<Digest>,
}

/// How many unit names a `NoUnit` message lists.
const SUGGESTIONS: usize = 8;

/// The part of `text` a person is likeliest to have right: what follows
/// its last `::` outside parentheses, up to its parameter or argument list.
fn last_component(text: &str) -> &str {
    let mut depth = 0i64;
    let mut start = 0;
    let b = text.as_bytes();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'(' => depth += 1,
            b')' => depth -= 1,
            b':' if depth == 0 && b.get(i + 1) == Some(&b':') => {
                start = i + 2;
                i += 1;
            }
            _ => {}
        }
        i += 1;
    }
    let tail = &text[start..];
    let cut = tail.find(['(', '<']).map_or(tail, |j| &tail[..j]);
    let cut = cut.trim();
    if cut.is_empty() { tail } else { cut }
}

/// Records indexed once per call: by name (for resolution) and by id (for
/// the chain of parents). Built from records in any order.
struct Index<'r> {
    all: &'r [Record],
    by_name: HashMap<&'r str, Vec<&'r Record>>,
    /// The `Key` of each id that some record of it has.
    keys: HashMap<Digest, &'r Key>,
}

impl<'r> Index<'r> {
    fn new(all: &'r [Record]) -> Index<'r> {
        let mut by_name: HashMap<&str, Vec<&Record>> = HashMap::new();
        let mut keys = HashMap::new();
        for r in all {
            by_name.entry(r.name.as_str()).or_default().push(r);
            if let Some(k) = &r.key {
                keys.entry(r.id).or_insert(k);
            }
        }
        Index { all, by_name, keys }
    }
}

/// The record `e` resolves to in `records` (§2.3): every unit of that name
/// (and kind, if given) has one `KeyId`, and the unit has a `Key`.
fn resolve_in<'r>(
    e: &Entity,
    index: &Index<'r>,
    set: &str,
) -> Result<&'r Record, (ErrorKind, String)> {
    let matching: Vec<&Record> = index
        .by_name
        .get(e.text.as_str())
        .into_iter()
        .flatten()
        .copied()
        .filter(|r| e.kind.is_none_or(|k| r.kind == k))
        .collect();
    let shown = match e.kind {
        Some(k) => format!("[{}] {}", k.name(), e.text),
        None => e.text.clone(),
    };
    let Some(first) = matching.first().copied() else {
        let part = last_component(&e.text);
        let near: BTreeSet<&str> =
            index.all.iter().map(|r| r.name.as_str()).filter(|n| n.contains(part)).collect();
        let detail = if near.is_empty() {
            format!("no unit of the {set} is named `{shown}`, and no unit's name contains `{part}`")
        } else {
            let list: Vec<String> =
                near.iter().take(SUGGESTIONS).map(|n| format!("`{n}`")).collect();
            let more = if near.len() > SUGGESTIONS { ", …" } else { "" };
            format!(
                "no unit of the {set} is named `{shown}`; units whose name contains `{part}`: {}{more}",
                list.join(", ")
            )
        };
        return Err((ErrorKind::NoUnit, detail));
    };
    let ids: BTreeSet<Digest> = matching.iter().map(|r| r.id).collect();
    if ids.len() > 1 {
        // One kind per key (the least name), the list sorted.
        let mut kinds: Vec<&str> = ids
            .iter()
            .filter_map(|id| matching.iter().filter(|r| r.id == *id).map(|r| r.kind.name()).min())
            .collect();
        kinds.sort_unstable();
        return Err((
            ErrorKind::Ambiguous,
            format!(
                "units named `{shown}` in the {set} have {} keys, of the kinds {}",
                ids.len(),
                kinds.join(", ")
            ),
        ));
    }
    if matching.iter().any(|r| r.key.is_none()) {
        return Err((
            ErrorKind::Fallback,
            format!("the unit `{shown}` of the {set} has a fallback key: no key to name"),
        ));
    }
    Ok(first)
}

/// How far a chain of parents is followed (an AST is far shallower).
const PARENT_LIMIT: usize = 4096;

/// `m(p)` of §2.3: `root`, `c_linkage`, `builtin` are themselves; for
/// `unit(id)`, the `Key` of that id in `post`, its parent mapped, then the
/// rules `above` applied as `RuleTable::map` does, and `unit` of the id
/// of the result. A parent whose `Key` the records do not hold (no record
/// of that id, or a fallback key), and everything above it, is kept as it
/// is: a rule built on it does not match, which verification reports.
fn map_parent(p: Parent, post: &Index<'_>, above: &RuleTable) -> Parent {
    // The chain of keys from `p` up to the first parent that is not a unit
    // with a known key.
    let mut chain: Vec<&Key> = Vec::new();
    let mut top = p;
    while let Parent::Unit(id) = top {
        if chain.len() >= PARENT_LIMIT {
            break;
        }
        let Some(k) = post.keys.get(&id) else { break };
        top = k.parent;
        chain.push(k);
    }
    if chain.is_empty() {
        return p;
    }
    // Map from the top down.
    let mut mapped = top;
    for k in chain.into_iter().rev() {
        let k = above.map(Key { parent: mapped, ..k.clone() });
        mapped = Parent::Unit(k.id());
    }
    mapped
}

/// The key of an exemption line (§2.3), with the `Duplicate` check of its
/// list.
fn exemption(
    n: u64,
    e: &Entity,
    index: &Index<'_>,
    set: &str,
    list: &mut BTreeSet<Digest>,
    word: &str,
) -> Result<(), LineError> {
    let r = resolve_in(e, index, set).map_err(|(k, d)| error(n, k, d))?;
    if !list.insert(r.id) {
        return Err(error(
            n,
            ErrorKind::Duplicate,
            format!("a second `{word}` line for `{}`", e.text),
        ));
    }
    Ok(())
}

/// The lines `relocate`, `changed` and `removed`, in file order: names
/// resolved in `pre` (an entity) and `post` (a non-empty parent), both the
/// records of the images with no rules, in any order. The first error in
/// file order.
///
/// `RuleTable` has no insertion, so the table a `relocate` line's parent is
/// mapped by is built again from the rules above it, and only when that
/// line has a parent and a rule was added since.
pub fn resolve(
    lines: &[(u64, Line)],
    pre: &[Record],
    post: &[Record],
) -> Result<Resolved, LineError> {
    let (pre, post) = (Index::new(pre), Index::new(post));
    let mut rules: Vec<Rule> = Vec::new();
    let mut matches: BTreeSet<(Parent, KeyKind, Name, Option<Digest>)> = BTreeSet::new();
    let mut table = RuleTable::empty();
    let mut stale = false;
    let (mut changed, mut removed) = (BTreeSet::new(), BTreeSet::new());
    let built = |rules: &[Rule], n: u64| {
        // Names are validated and duplicates refused as each line is read.
        RuleTable::new(rules.to_vec()).map_err(|e| {
            error(n, ErrorKind::NotRenamable, format!("the rules cannot be a rule table: {e}"))
        })
    };
    for (n, l) in lines {
        let n = *n;
        match l {
            Line::Relocate { entity, parent, name } => {
                let r = resolve_in(entity, &pre, "pre-images").map_err(|(k, d)| error(n, k, d))?;
                let Some(k) = r.key.clone() else {
                    unreachable!("resolve_in gives a record with a Key")
                };
                let new_name = match name {
                    None => k.name.clone(),
                    Some(id) => {
                        if !matches!(k.name, Name::Identifier(_)) {
                            return Err(error(
                                n,
                                ErrorKind::NotRenamable,
                                format!(
                                    "`{}` has a name of the form `{}`, not an identifier: it can be relocated with `=` only",
                                    entity.text,
                                    k.name.form()
                                ),
                            ));
                        }
                        Name::Identifier(id.as_bytes().to_vec())
                    }
                };
                let new_parent = match parent {
                    None => Parent::Root,
                    Some(p) => {
                        let r =
                            resolve_in(p, &post, "post-images").map_err(|(k, d)| error(n, k, d))?;
                        // The rule of this line is pushed below, so the
                        // table is stale again after it.
                        if stale {
                            table = built(&rules, n)?;
                        }
                        map_parent(Parent::Unit(r.id), &post, &table)
                    }
                };
                let rule = Rule {
                    parent: new_parent,
                    kind: k.kind,
                    name: new_name,
                    disc: k.disc,
                    to_parent: k.parent,
                    to_name: k.name,
                };
                // A name no reader accepts comes from no walk, but is an
                // error, not a panic.
                if let Err(what) = rule.name.validate().and_then(|()| rule.to_name.validate()) {
                    return Err(error(
                        n,
                        ErrorKind::NotRenamable,
                        format!("the rule of `{}` cannot be in a rule table: {what}", entity.text),
                    ));
                }
                if !matches.insert((rule.parent, rule.kind, rule.name.clone(), rule.disc)) {
                    return Err(error(
                        n,
                        ErrorKind::Duplicate,
                        format!(
                            "the rule of `{}` matches what the rule of an earlier `relocate` line matches",
                            entity.text
                        ),
                    ));
                }
                rules.push(rule);
                stale = true;
            }
            Line::Changed(e) => exemption(n, e, &pre, "pre-images", &mut changed, "changed")?,
            Line::Removed(e) => exemption(n, e, &pre, "pre-images", &mut removed, "removed")?,
            Line::Created(_) => {}
        }
    }
    if stale {
        let last = lines.last().map_or(0, |(n, _)| *n);
        table = built(&rules, last)?;
    }
    Ok(Resolved {
        rules: table,
        changed: changed.into_iter().collect(),
        removed: removed.into_iter().collect(),
    })
}

/// `post_mapped`: the post-images' records under `resolved.rules`, in any
/// order.
///
/// The `created` lines, in file order, resolved in `post_mapped`, and the
/// `Declared` of the file.
pub fn finish(
    lines: &[(u64, Line)],
    resolved: Resolved,
    post_mapped: &[Record],
) -> Result<Declared, LineError> {
    let index = Index::new(post_mapped);
    let mut created = BTreeSet::new();
    for (n, l) in lines {
        if let Line::Created(e) = l {
            exemption(*n, e, &index, "post-images", &mut created, "created")?;
        }
    }
    let Resolved { rules, changed, removed } = resolved;
    match Declared::new(rules.rules().to_vec(), changed, created.into_iter().collect(), removed) {
        Ok(d) => Ok(d),
        Err(e) => {
            unreachable!("the lists of resolved lines are sorted and without duplicates: {e}")
        }
    }
}
