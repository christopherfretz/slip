//! Declared changes by name (`spec/verify.md` §2.3), the pure part of
//! `slip declare`: the text ([`read`]), which parses it needs ([`needs`]),
//! names resolved to keys and `relocate` lines turned into rules
//! ([`resolve`]), and the `Declared` ([`finish`]). Work package WP-055;
//! [`main`], the command (`spec/exec.md` §8.8), and the in-process parses
//! it shares with `slip units` are WP-053's; the `kept` line, the checks of
//! a `relocate` line against the run's files and the later lines, and the
//! rules of undefined friends are WP-111's.
//!
//! Every function is over records (`slip_digest::records`), not parses, so
//! that the resolution is tested on records written by hand. A name
//! resolves only when every unit of that name (and kind) has one `KeyId`:
//! nothing is chosen among candidates.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::ffi::OsString;
use std::fmt;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use slip_digest::records::{Facts, Record};
use slip_digest::{ReadOnly, ReadOnlyFiles};
use slip_exec::store::{Access, Store};
use slip_model::declared::Declared;
use slip_model::formats::content_id;
use slip_model::keys::{Key, KeyKind, Name, Parent, Rule, RuleTable};
use slip_model::transform::Ownership;
use slip_model::{Digest, Observer, Phase};

use crate::args::{DeclareArgs, ImageArgs};
use crate::commands::{self, Failure};
use crate::image::{Image, Side};
use crate::status;

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
    /// `kept <entity>` (`spec/verify.md` §9.2): an owned entity that stays
    /// in a relocated namespace.
    Kept(Entity),
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
    /// A `relocate` line whose rule `spec/verify.md` §2.1 refuses
    /// (`Rule::refusal`).
    Refused,
    /// A `relocate` line whose entity is a dependency entity and not a
    /// namespace (`spec/verify.md` §2.1: no rule renames it).
    DependencyEntity,
    /// A `relocate` line that relocates the new parent of a line above
    /// it, or a namespace or class enclosing that parent.
    Order,
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
            ErrorKind::Refused => "refused",
            ErrorKind::DependencyEntity => "dependency entity",
            ErrorKind::Order => "order",
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
        "kept" => one(Line::Kept),
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
/// parent); `post_mapped`, under the rules the file gives (a `created` or
/// `kept` line).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Needs {
    pub post: bool,
    pub post_mapped: bool,
}

pub fn needs(lines: &[(u64, Line)]) -> Needs {
    let post = lines.iter().any(|(_, l)| matches!(l, Line::Relocate { parent: Some(_), .. }));
    let post_mapped = lines.iter().any(|(_, l)| matches!(l, Line::Created(_) | Line::Kept(_)));
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
///
/// `probes` receives the key of each level as it is looked up in `above`
/// (its own parent already mapped): a rule of a later line that matches
/// one of them would have changed `m(p)` (`ErrorKind::Order`).
fn map_parent(p: Parent, post: &Index<'_>, above: &RuleTable, probes: &mut Vec<Key>) -> Parent {
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
        let probe = Key { parent: mapped, ..k.clone() };
        probes.push(probe.clone());
        let k = above.map(probe);
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

/// [`resolve_with`] with no [`Facts`]: no entity is known to be a
/// dependency entity, and no friend declaration to be undefined. For
/// records written by hand; the command gives the facts of its images.
pub fn resolve(
    lines: &[(u64, Line)],
    pre: &[Record],
    post: &[Record],
) -> Result<Resolved, LineError> {
    resolve_with(lines, pre, post, &Facts::default())
}

/// What follows the last `::` of `text` outside parentheses, the
/// parameters kept: `f(n::C)` of `n::f(n::C)`.
fn unqualified(text: &str) -> &str {
    let (mut depth, mut start) = (0i64, 0);
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
    &text[start..]
}

/// The friend declarations of the pre-images that the function of `r`
/// (key `k`) needs a second rule for (`docs/wp/WP-071.md` "Added after the
/// review of WP-103"): undefined friend declarations ([`Facts`]) of the
/// same kind and name, in a class whose nearest enclosing namespace is the
/// function's parent, whose name is that class's `::friend ` and the
/// function's own unqualified name. Sorted by id, each once.
fn undefined_friends<'r>(r: &Record, k: &Key, pre: &Index<'r>, facts: &Facts) -> Vec<&'r Record> {
    if facts.undefined_friends.is_empty() {
        return Vec::new();
    }
    let suffix = format!("::friend {}", unqualified(&r.name));
    let mut out: Vec<&Record> = pre
        .all
        .iter()
        .filter(|f| facts.undefined_friends.contains(&f.id) && f.name.ends_with(&suffix))
        .filter(|f| {
            let Some(fk) = &f.key else { return false };
            if fk.kind != k.kind || fk.name != k.name || fk.parent == k.parent {
                return false;
            }
            // Up the classes around the friend to the first parent that is
            // no class.
            let mut p = fk.parent;
            for _ in 0..PARENT_LIMIT {
                if p == k.parent {
                    return true;
                }
                let Parent::Unit(id) = p else { return false };
                let Some(c) = pre.keys.get(&id) else { return false };
                if c.kind == KeyKind::Namespace {
                    return false;
                }
                p = c.parent;
            }
            false
        })
        .collect();
    out.sort_by_key(|f| f.id);
    out.dedup_by_key(|f| f.id);
    out
}

/// A `relocate` line's checks of its entity (`r`, with `Key` `k`) and its
/// new name: `DependencyEntity` (§2.1 rule 1: no rule renames a dependency
/// entity; a namespace's rule applies in the owned view) and
/// `NotRenamable`.
fn relocated_name(
    n: u64,
    entity: &Entity,
    name: Option<&str>,
    id: Digest,
    k: &Key,
    facts: &Facts,
) -> Result<Name, LineError> {
    if k.kind != KeyKind::Namespace && facts.dependency.contains(&id) {
        return Err(error(
            n,
            ErrorKind::DependencyEntity,
            format!(
                "`{}` is a dependency entity (a declaration of it is in a read-only file, or it is a specialisation or an implicit member of one): no rule renames it",
                entity.text
            ),
        ));
    }
    match name {
        None => Ok(k.name.clone()),
        Some(_) if !matches!(k.name, Name::Identifier(_)) => Err(error(
            n,
            ErrorKind::NotRenamable,
            format!(
                "`{}` has a name of the form `{}`, not an identifier: it can be relocated with `=` only",
                entity.text,
                k.name.form()
            ),
        )),
        Some(id) => Ok(Name::Identifier(id.as_bytes().to_vec())),
    }
}

/// The checks of one rule a `relocate` line gives (`of` names it in the
/// message): `NotRenamable` for a name no reader accepts (it comes from no
/// walk, but is an error, not a panic); `Refused` for a rule §2.1 refuses
/// (the walk keys nothing under a table that holds one); `Order` when it
/// matches a probe of a line above (`looked`).
fn rule_checks(
    n: u64,
    rule: &Rule,
    of: &str,
    looked: &[(u64, String, Vec<Key>)],
) -> Result<(), LineError> {
    if let Err(what) = rule.name.validate().and_then(|()| rule.to_name.validate()) {
        return Err(error(
            n,
            ErrorKind::NotRenamable,
            format!("the rule of {of} cannot be in a rule table: {what}"),
        ));
    }
    if let Some(what) = rule.refusal() {
        return Err(error(n, ErrorKind::Refused, format!("the rule of {of} is refused: {what}")));
    }
    let hit = |q: &Key| {
        (q.parent, q.kind, &q.name, q.disc) == (rule.parent, rule.kind, &rule.name, rule.disc)
    };
    if let Some((above, text, _)) = looked.iter().find(|(_, _, levels)| levels.iter().any(hit)) {
        return Err(error(
            n,
            ErrorKind::Order,
            format!(
                "the rule of {of} relocates {text}, or a namespace or class around it, which line {above} resolved as a new parent without it: this line must come before line {above}"
            ),
        ));
    }
    Ok(())
}

/// A line's rule `main` and, when it renames, one for each undefined friend
/// declaration of the function (`friends`), which keeps its class: the
/// class's chain is mapped as a new parent is, its levels added to
/// `probes`.
fn with_friends(
    main: Rule,
    friends: &[&Record],
    post: &Index<'_>,
    table: &RuleTable,
    probes: &mut Vec<Key>,
) -> Vec<Rule> {
    let mut given = Vec::new();
    if main.name != main.to_name {
        for f in friends {
            let Some(fk) = &f.key else { continue };
            given.push(Rule {
                parent: map_parent(fk.parent, post, table, probes),
                kind: fk.kind,
                name: main.name.clone(),
                disc: fk.disc,
                to_parent: fk.parent,
                to_name: fk.name.clone(),
            });
        }
    }
    given.insert(0, main);
    given
}

/// The lines `relocate`, `changed` and `removed`, in file order: names
/// resolved in `pre` (an entity) and `post` (a non-empty parent), both the
/// records of the images with no rules, in any order; `facts`, those of the
/// pre-images under the run's files. The first error in file order.
///
/// A `relocate` line whose entity is a dependency entity (not a
/// namespace) is `DependencyEntity`; one whose rule matches a level of the
/// new parent of a line above it, as that line looked it up, is `Order`.
/// An entity with C language linkage keeps the parent `c_linkage` in its
/// rule, whatever the line's parent, which is resolved like any other. A line
/// whose entity is a function with undefined friend declarations in class
/// templates (`facts`) gives, beside its rule, a rule for each of them when
/// the name changes; a later line that gives one of these rules again is
/// accepted, and one that gives another rule with its match is `Duplicate`.
///
/// `RuleTable` has no insertion, so the table a `relocate` line's parent is
/// mapped by is built again from the rules above it, and only when that
/// line has a parent and a rule was added since.
pub fn resolve_with(
    lines: &[(u64, Line)],
    pre: &[Record],
    post: &[Record],
    facts: &Facts,
) -> Result<Resolved, LineError> {
    let (pre, post) = (Index::new(pre), Index::new(post));
    let mut rules: Vec<Rule> = Vec::new();
    let mut matches: BTreeSet<(Parent, KeyKind, Name, Option<Digest>)> = BTreeSet::new();
    // The rules a line gave for an undefined friend.
    let mut friend_rules: HashSet<Rule> = HashSet::new();
    // Each line's new parent as it looked it up: (line, text, levels).
    let mut looked: Vec<(u64, String, Vec<Key>)> = Vec::new();
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
                let new_name = relocated_name(n, entity, name.as_deref(), r.id, &k, facts)?;
                let friends = undefined_friends(r, &k, &pre, facts);
                // The rule of this line is pushed below, so the table is
                // stale again after it.
                if stale && (parent.is_some() || (!friends.is_empty() && new_name != k.name)) {
                    table = built(&rules, n)?;
                    stale = false;
                }
                let mut probes = Vec::new();
                // §2.3: a C function's rule keeps `c_linkage`; its line's
                // parent is resolved, and looks nothing up.
                let c_linkage = k.parent == Parent::CLinkage;
                let p = parent.as_ref().map(|p| resolve_in(p, &post, "post-images"));
                let new_parent = match p.transpose().map_err(|(k, d)| error(n, k, d))? {
                    _ if c_linkage => Parent::CLinkage,
                    None => Parent::Root,
                    Some(r) => map_parent(Parent::Unit(r.id), &post, &table, &mut probes),
                };
                let main = Rule {
                    parent: new_parent,
                    kind: k.kind,
                    name: new_name,
                    disc: k.disc,
                    to_parent: k.parent,
                    to_name: k.name.clone(),
                };
                let given = with_friends(main, &friends, &post, &table, &mut probes);
                for (i, rule) in given.into_iter().enumerate() {
                    let of = if i == 0 {
                        format!("`{}`", entity.text)
                    } else {
                        format!("`{}`'s undefined friend declaration", entity.text)
                    };
                    rule_checks(n, &rule, &of, &looked)?;
                    if friend_rules.contains(&rule) {
                        // Given already for a friend: the same rule again.
                        continue;
                    }
                    if !matches.insert((rule.parent, rule.kind, rule.name.clone(), rule.disc)) {
                        if i > 0 && rules.contains(&rule) {
                            continue;
                        }
                        return Err(error(
                            n,
                            ErrorKind::Duplicate,
                            format!(
                                "the rule of {of} matches what the rule of an earlier `relocate` line matches"
                            ),
                        ));
                    }
                    if i > 0 {
                        friend_rules.insert(rule.clone());
                    }
                    rules.push(rule);
                    stale = true;
                }
                if !probes.is_empty() {
                    let what = match parent {
                        Some(p) => format!("`{}`", p.text),
                        None => format!("the class of a friend declaration of `{}`", entity.text),
                    };
                    looked.push((n, what, probes));
                }
            }
            Line::Changed(e) => exemption(n, e, &pre, "pre-images", &mut changed, "changed")?,
            Line::Removed(e) => exemption(n, e, &pre, "pre-images", &mut removed, "removed")?,
            Line::Created(_) | Line::Kept(_) => {}
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
/// The `created` and `kept` lines, in file order, resolved in
/// `post_mapped`, and the `Declared` of the file.
pub fn finish(
    lines: &[(u64, Line)],
    resolved: Resolved,
    post_mapped: &[Record],
) -> Result<Declared, LineError> {
    let index = Index::new(post_mapped);
    let (mut created, mut kept) = (BTreeSet::new(), BTreeSet::new());
    for (n, l) in lines {
        match l {
            Line::Created(e) => exemption(*n, e, &index, "post-images", &mut created, "created")?,
            Line::Kept(e) => exemption(*n, e, &index, "post-images", &mut kept, "kept")?,
            _ => {}
        }
    }
    let Resolved { rules, changed, removed } = resolved;
    match Declared::new(rules.rules().to_vec(), changed, created.into_iter().collect(), removed)
        .and_then(|d| d.with_kept(kept.into_iter().collect()))
    {
        Ok(d) => Ok(d),
        Err(e) => {
            unreachable!("the lists of resolved lines are sorted and without duplicates: {e}")
        }
    }
}

// ---------------------------------------------------------------------------
// The command (spec/exec.md §8.8), and what `slip units` shares with it

/// The most observers `declare` and `units` parse (§8.8): each is parsed
/// in this process, one after another.
pub(crate) const MAX_SELECTED: usize = 64;

/// What `declare` and `units` work over: the store (opened shared), the
/// plan's selected observers and the image, every path made absolute
/// first (a parse changes the current directory).
pub(crate) struct InProcess {
    /// Kept open (shared) while the command runs.
    _store: Store,
    pub(crate) observers: Vec<Observer>,
    pub(crate) image: Image,
}

impl InProcess {
    /// §8.8: the store opened shared, the plan resolved as for `parse`, the
    /// image made as for `verify`, and the selection, which must keep from
    /// 1 to [`MAX_SELECTED`] observers. `command` names the command in the
    /// input errors.
    pub(crate) fn open(
        command: &str,
        plan: &OsString,
        image: &ImageArgs,
        store: &Path,
        only: &[Vec<u8>],
        limit: Option<u64>,
    ) -> commands::Result<InProcess> {
        let store = commands::open_store(&absolute(store)?, Access::Shared)?;
        // A plan given as a file is read here, before any parse changes the
        // current directory.
        let (plan, _) = commands::load_plan(&store, plan)?;
        let image =
            Image::from_args(image).map_err(|e| Failure::Input(format!("{command}: {e}")))?;
        let observers = commands::select(&plan.observers, only, limit);
        if observers.is_empty() || observers.len() > MAX_SELECTED {
            return Err(Failure::Input(format!(
                "{command}: the selection keeps {} of the plan's {} observer(s); {command} \
                 parses every one in this process and needs from 1 to {MAX_SELECTED}",
                observers.len(),
                plan.observers.len()
            )));
        }
        Ok(InProcess { _store: store, observers, image })
    }

    /// The unit records (`slip_digest::records`) of `side` of `o` under
    /// `rules`, with the run's files `files` (`spec/verify.md` §2.1: each
    /// buffer decided by the subject path of its real path, as the verify
    /// job decides it; `None` makes every file read-only; not read when
    /// `rules` is empty); `None`, after the warning of §8.8 on `err`, when
    /// that image does not parse `ok`. A failure of the parse itself, or of
    /// the records, is an input error naming the main file: no record is
    /// guessed, and none is left out silently.
    pub(crate) fn records(
        &self,
        command: &str,
        o: &Observer,
        side: Side,
        rules: &RuleTable,
        files: Option<&Ownership>,
        err: &mut dyn Write,
    ) -> commands::Result<Option<Vec<Record>>> {
        let walk = |input: &slip_digest::Input<'_>| {
            if rules.rules().is_empty() {
                return slip_digest::records::records(input, rules, ReadOnly::EVERY_FILE);
            }
            let read_only = ReadOnlyFiles::new(files, input.sm, input.subject_path);
            slip_digest::records::records(input, rules, ReadOnly::files(&read_only))
        };
        self.walk(command, o, side, walk, err)
    }

    /// What `walk` gives for `side` of `o`, parsed and guarded as
    /// [`InProcess::records`] says; `None`, after the warning, when the
    /// image does not parse `ok`.
    pub(crate) fn walk<T: Send>(
        &self,
        command: &str,
        o: &Observer,
        side: Side,
        walk: impl FnOnce(&slip_digest::Input<'_>) -> Result<T, String> + Send,
        err: &mut dyn Write,
    ) -> commands::Result<Option<T>> {
        let main = String::from_utf8_lossy(&o.main.path).into_owned();
        let fail = |e: String| Failure::Input(format!("{command}: {main}: {e}"));
        // The guards of the post-image parse (`spec/verify.md` §5.3: G1,
        // L1, G4, G5, G6) on a post-image of the tree form, as the verify
        // job checks them: without them the parse need not be the real
        // post-image's, and a file's subject path need not be a real path
        // (ownership decided on it can give a wrong rule parent or a wrong
        // `created` key). A failure is an input error: no record is
        // guessed. O2 holds by construction here (`spec/exec.md` §8.8).
        let (shard, records) = if side == Side::Post && !self.image.is_identity() {
            let (shard, looked) =
                self.image.parse_looking(o, side, Phase::Map, 0, walk).map_err(fail)?;
            if let Some((_, lookups)) = &looked {
                let inputs = shard.inputs.as_deref().unwrap_or_default();
                let mut resolver = self.image.resolver();
                if let Some(reason) = crate::verify_job::post_parse_guards(
                    &self.image,
                    &mut resolver,
                    &o.command,
                    inputs,
                    lookups,
                ) {
                    return Err(fail(reason));
                }
            }
            (shard, looked.map(|(records, _)| records))
        } else {
            self.image.parse(o, side, Phase::Map, 0, walk).map_err(fail)?
        };
        match records {
            Some(Ok(records)) => Ok(Some(records)),
            Some(Err(e)) => Err(fail(e)),
            None => {
                let which = match side {
                    Side::Pre => "pre",
                    Side::Post => "post",
                };
                let _ = write!(err, "slip: warning: {command}: ");
                let _ = err.write_all(&o.main.path);
                let _ = writeln!(err, ": the {which}-image is {}", shard.outcome.status.name());
                Ok(None)
            }
        }
    }
}

/// `path` made absolute against the current directory.
pub(crate) fn absolute(path: &Path) -> commands::Result<PathBuf> {
    std::path::absolute(path).map_err(|e| Failure::Input(format!("{}: {e}", path.display())))
}

/// Records of several observers as one set (§8.8): a record given by more
/// than one observer once, in the order first seen.
#[derive(Default)]
pub(crate) struct Merged {
    seen: HashSet<Record>,
    pub(crate) records: Vec<Record>,
}

impl Merged {
    pub(crate) fn add(&mut self, records: Vec<Record>) {
        for r in records {
            if !self.seen.contains(&r) {
                self.seen.insert(r.clone());
                self.records.push(r);
            }
        }
    }
}

/// `slip declare` (`spec/exec.md` §8.8).
pub fn main(args: &DeclareArgs, out: &mut dyn Write, err: &mut dyn Write) -> ExitCode {
    let result = declare(args, out, err);
    commands::finish(result, out, err)
}

fn declare(a: &DeclareArgs, out: &mut dyn Write, err: &mut dyn Write) -> commands::Result<u8> {
    let from = absolute(&a.from)?;
    let target = absolute(&a.out)?;
    let bytes =
        std::fs::read(&from).map_err(|e| Failure::Input(format!("{}: {e}", a.from.display())))?;
    let text = std::str::from_utf8(&bytes).map_err(|e| {
        Failure::Input(format!("{}: the text is not UTF-8 ({e})", a.from.display()))
    })?;
    let line_error = |e: &LineError, err: &mut dyn Write| {
        let _ =
            writeln!(err, "slip: {}:{}: {}: {}", a.from.display(), e.line, e.kind.name(), e.detail);
        status::FINDINGS
    };
    let lines = match read(text) {
        Ok(lines) => lines,
        Err(e) => return Ok(line_error(&e, err)),
    };
    // The run's files (`spec/verify.md` §2.1, §2.3): a rule renames owned
    // entities only, so a `relocate` line needs them, and there is no
    // default (knowing less never accepts more; "the roots" by default
    // would make a dependency's entity renamable by accident). Checked
    // before anything is parsed.
    if a.owned.is_empty() && lines.iter().any(|(_, l)| matches!(l, Line::Relocate { .. })) {
        return Err(Failure::Usage("a relocate line needs the run's files: --owned".to_owned()));
    }
    let files = commands::ownership(&a.owned, &a.readonly)?;
    let run = InProcess::open("declare", &a.plan, &a.image, &a.store, &a.only, a.limit)?;
    let need = needs(&lines);

    // The images with no rules: every pre-image; the post-images when a
    // `relocate` names a parent.
    let none = RuleTable::empty();
    let (mut pre, mut post) = (Merged::default(), Merged::default());
    let mut post_ok = vec![false; run.observers.len()];
    // With a `relocate` line, the pre-images' facts under the run's files
    // (`DependencyEntity`, the rules of undefined friends); with the run's
    // files, the subject paths every pre-image read (the warning below).
    let relocates = lines.iter().any(|(_, l)| matches!(l, Line::Relocate { .. }));
    let mut facts = Facts::default();
    let mut read: BTreeSet<Vec<u8>> = BTreeSet::new();
    for (i, o) in run.observers.iter().enumerate() {
        let walk = |input: &slip_digest::Input<'_>| {
            let records = slip_digest::records::records(input, &none, ReadOnly::EVERY_FILE)?;
            let mut paths = Vec::new();
            let mut facts = None;
            if let Some(files) = files.as_ref() {
                paths = input.sm.file_ids().map(|f| (input.subject_path)(f)).collect();
                if relocates {
                    let ro = ReadOnlyFiles::new(Some(files), input.sm, input.subject_path);
                    facts = Some(slip_digest::records::facts(input, ReadOnly::files(&ro)));
                }
            }
            Ok((records, paths, facts))
        };
        if let Some((r, paths, f)) = run.walk("declare", o, Side::Pre, walk, err)? {
            pre.add(r);
            read.extend(paths.into_iter().filter(|p| !p.is_empty()));
            if let Some(f) = f {
                facts.merge(f);
            }
        }
        if need.post
            && let Some(r) = run.records("declare", o, Side::Post, &none, None, err)?
        {
            post.add(r);
            post_ok[i] = true;
        }
    }
    // WP-053 review F2: an `--owned` entry that covers no file a pre-image
    // read (`--owned <post tree>`, say) makes no file owned through it.
    if let Some(files) = files.as_ref() {
        for entry in &files.owned {
            if !read.iter().any(|p| slip_model::transform::covers(entry, p)) {
                let _ = write!(err, "slip: warning: declare: --owned ");
                let _ = err.write_all(entry);
                let _ = writeln!(err, " covers no file the selected observers' pre-images read");
            }
        }
    }
    let resolved = match resolve_with(&lines, &pre.records, &post.records, &facts) {
        Ok(r) => r,
        Err(e) => return Ok(line_error(&e, err)),
    };

    // The post-images under the rules the file gives, for `created` and
    // `kept` lines.
    let mut mapped = Merged::default();
    if need.post_mapped {
        for (i, o) in run.observers.iter().enumerate() {
            // A post-image that did not parse has been warned about once.
            if need.post && !post_ok[i] {
                continue;
            }
            if let Some(r) =
                run.records("declare", o, Side::Post, &resolved.rules, files.as_ref(), err)?
            {
                mapped.add(r);
            }
        }
    }
    let declared = match finish(&lines, resolved, &mapped.records) {
        Ok(d) => d,
        Err(e) => return Ok(line_error(&e, err)),
    };

    // `files` is present exactly when there is a rule (`spec/verify.md`
    // §2 rule 6: a second encoding of the empty table is refused). Without
    // a rule the options were checked above and are not written.
    let files = if declared.rules().rules().is_empty() { None } else { files };
    let declared = declared.with_files(files);
    let bytes = declared.to_bytes();
    let id = content_id(&bytes)
        .unwrap_or_else(|e| unreachable!("a Declared encodes as a valid DECL: {e}"));
    write_beside(&target, &bytes)
        .map_err(|e| Failure::Input(format!("{}: {e}", a.out.display())))?;
    let _ = writeln!(out, "declared {id}");
    let _ = writeln!(out, "rules {} {}", declared.rules_id(), declared.rules().rules().len());
    let _ = writeln!(out, "changed {}", declared.changed().len());
    let _ = writeln!(out, "created {}", declared.created().len());
    let _ = writeln!(out, "removed {}", declared.removed().len());
    let _ = writeln!(out, "kept {}", declared.kept().len());
    Ok(status::OK)
}

/// `bytes` written to a file beside `path` and renamed over it: `path` is
/// the old file or the whole new one, never part of it.
fn write_beside(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let tmp = crate::verify_cmd::beside(path);
    let written = std::fs::write(&tmp, bytes).and_then(|()| std::fs::rename(&tmp, path));
    if written.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    written
}
