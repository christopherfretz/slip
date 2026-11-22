//! The checks (`spec/digest-kinds.md` §1, "Tests"; `spec/digest.md` §13
//! obligations 4 and 5). Each returns every violation it finds, as
//! `digest-kinds.md:<line>: <what>` or `<Kiln type>: <what>`.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use super::kiln::{Descs, From, UNTABLED};
use super::{Base, Class, Decision, Doc, Encoding, Kind, Number, Pin, Row, Type};

fn at(line: usize, what: impl std::fmt::Display) -> String {
    format!("digest-kinds.md:{line}: {what}")
}

/// The tabled types by short name.
pub fn by_short(doc: &Doc) -> HashMap<&str, &Type> {
    doc.types.iter().map(|t| (t.short.as_str(), t)).collect()
}

/// §1.1 to §1.4, on the file alone.
pub fn grammar(doc: &Doc) -> Vec<String> {
    let mut e = doc.errors.clone();
    let mut shorts: HashMap<&str, usize> = HashMap::new();
    let mut paths: HashMap<&str, &Type> = HashMap::new();
    for t in &doc.types {
        if let Some(first) = shorts.insert(&t.short, t.line) {
            e.push(at(
                t.line,
                format!("short name `{}` is also the type of line {first}", t.short),
            ));
        }
        paths.insert(&t.path, t);
    }
    let shorts = by_short(doc);
    let mut ranges: Vec<(u32, u32, usize, &str)> = vec![];
    for t in &doc.types {
        table(t, &shorts, &paths, &mut e);
        if let Some(b) = t.base {
            let n = u32::try_from(t.variant_rows().count()).unwrap_or(u32::MAX);
            ranges.push((b + 1, b + n.max(1), t.line, &t.short));
        }
    }
    ranges.sort_unstable();
    for w in ranges.windows(2) {
        if w[1].0 <= w[0].1 {
            e.push(at(
                w[1].2,
                format!(
                    "the tags of `{}` ({}..={}) overlap those of `{}` ({}..={})",
                    w[1].3, w[1].0, w[1].1, w[0].3, w[0].0, w[0].1
                ),
            ));
        }
    }
    for (what, list) in
        [("KeyKind", &doc.key_kinds), ("Name", &doc.names), ("property", &doc.properties)]
    {
        numbers(what, list, &mut e);
    }
    e
}

/// §3.1, §3.2 and §11.1: numbers from 1 in order, names unique.
fn numbers(what: &str, list: &[Number], e: &mut Vec<String>) {
    if list.is_empty() {
        e.push(format!("digest-kinds.md: no table of `{what}`"));
    }
    let mut seen = HashSet::new();
    for (k, n) in list.iter().enumerate() {
        if usize::try_from(n.value).ok() != Some(k + 1) {
            e.push(at(
                n.line,
                format!("{what} number {} is out of order: {} expected", n.value, k + 1),
            ));
        }
        if let Some(name) = &n.name
            && !seen.insert(name)
        {
            e.push(at(n.line, format!("{what} `{name}` is listed twice")));
        }
    }
}

/// The first line of a member's row of its own and of its first fact row.
type OwnAndFact = (Option<usize>, Option<usize>);

fn table(
    t: &Type,
    shorts: &HashMap<&str, &Type>,
    paths: &HashMap<&str, &Type>,
    e: &mut Vec<String>,
) {
    let name = &t.short;
    if t.base.is_some() && !t.has_variants() {
        e.push(at(t.line, format!("`{name}`: `base` is for a table of variants")));
    }
    if t.selector.is_some() && t.has_variants() {
        e.push(at(t.line, format!("`{name}`: `selector` is for a record")));
    }
    if t.rows.is_empty() {
        e.push(at(t.line, format!("`{name}`: the table has no rows")));
    }
    // The selector names a tabled enumeration; kind prefixes are its rows.
    let selector = t.selector.as_deref().and_then(|s| match paths.get(s) {
        Some(sel) if sel.has_variants() => Some(*sel),
        _ => {
            e.push(at(t.line, format!("`{name}`: selector `{s}` is not a tabled enumeration")));
            None
        }
    });
    let mut texts = HashSet::new();
    // member (with its kind prefix) -> (has a row of its own, a fact row)
    let mut facts: BTreeMap<(Option<&str>, &str), OwnAndFact> = BTreeMap::new();
    for r in &t.rows {
        if !texts.insert(&r.text) {
            e.push(at(r.line, format!("`{name}`: row `{}` is listed twice", r.text)));
        }
        if let (Some(k), Some(sel)) = (&r.kind, selector)
            && !sel.variant_rows().any(|v| &v.member == k)
        {
            e.push(at(r.line, format!("`{}`: `{k}` is not a variant of `{}`", r.text, sel.short)));
        }
        let slot = facts.entry((r.kind.as_deref(), &r.member)).or_default();
        if r.fact.is_some() {
            slot.1.get_or_insert(r.line);
        } else {
            slot.0.get_or_insert(r.line);
        }
        check_row(t, r, shorts, e);
    }
    for ((_, member), (own, fact)) in facts {
        if let (Some(own), Some(fact)) = (own, fact) {
            e.push(at(
                own,
                format!("`{name}`: `{member}` has a row of its own and fact rows (line {fact}); a packed field has only the latter"),
            ));
        }
    }
}

fn check_row(t: &Type, r: &Row, shorts: &HashMap<&str, &Type>, errors: &mut Vec<String>) {
    let e = errors;
    let text = &r.text;
    let variant = t.has_variants() && r.is_variant();
    match (&r.encoding, r.decision) {
        (Encoding::Tag(n), Decision::In) => {
            if !variant {
                e.push(at(r.line, format!("`{text}`: `tag N` is for a variant row")));
            } else if let Some(want) = t.number_of(&r.member)
                && *n != want
            {
                e.push(at(r.line, format!("`{text}`: tag {n}, but §1.4 gives {want}")));
            }
        }
        (Encoding::Form(..), Decision::In) if variant => {
            e.push(at(r.line, format!("`{text}`: an `in` variant row is `tag N`")));
        }
        (Encoding::Form(base, _), Decision::In) => match base {
            Base::Enum(ty) | Base::Record(ty) => {
                let is_enum = matches!(base, Base::Enum(_));
                match shorts.get(ty.as_str()) {
                    None => e.push(at(r.line, format!("`{text}`: `{ty}` names no tabled type"))),
                    Some(target) if is_enum && !target.has_variants() => {
                        e.push(at(
                            r.line,
                            format!("`{text}`: `enum {ty}` names a table without variants"),
                        ));
                    }
                    Some(target)
                        if !is_enum && matches!(target.kind, Kind::Enum | Kind::Consts) =>
                    {
                        e.push(at(r.line, format!("`{text}`: `record {ty}` names an enumeration")));
                    }
                    Some(_) => {}
                }
            }
            _ => {}
        },
        (Encoding::None, Decision::In) => {
            e.push(at(r.line, format!("`{text}`: an `in` row needs an encoding")));
        }
        (Encoding::None, _) | (Encoding::Unbuilt, Decision::Unsupported) => {}
        (Encoding::As(v), Decision::Out) if variant => {
            if v == &r.member
                || !t
                    .variant_rows()
                    .any(|x| &x.member == v && !matches!(x.encoding, Encoding::As(_)))
            {
                e.push(at(
                    r.line,
                    format!("`{text}`: `as {v}` names no other variant of `{}`", t.short),
                ));
            }
        }
        (enc, decision) => {
            let enc = match enc {
                Encoding::Unbuilt => "unbuilt".to_owned(),
                Encoding::As(v) => format!("as {v}"),
                Encoding::Tag(n) => format!("tag {n}"),
                Encoding::Form(..) => "an encoding".to_owned(),
                Encoding::None => unreachable!("handled above"),
            };
            e.push(at(r.line, format!("`{text}`: a `{}` row cannot have {enc}", d_word(decision))));
        }
    }
    if r.note.is_empty() {
        let needs = match r.decision {
            Decision::Out => !matches!(r.class, Class::Location | Class::Bookkeeping),
            Decision::Unsupported => true,
            _ => false,
        };
        if needs {
            e.push(at(
                r.line,
                format!("`{text}`: a `{}` row gives its reason in Note", d_word(r.decision)),
            ));
        }
    }
}

fn d_word(d: Decision) -> &'static str {
    match d {
        Decision::In => "in",
        Decision::Out => "out",
        Decision::Symbolic => "symbolic",
        Decision::Demand => "demand",
        Decision::Unsupported => "unsupported",
    }
}

/// Obligation 4: the tables against Kiln's descriptors, both ways, and
/// every `pin`.
pub fn against_kiln(doc: &Doc, descs: &Descs) -> Vec<String> {
    let mut e = vec![];
    let tabled: HashMap<&str, &Type> = doc.types.iter().map(|t| (t.path.as_str(), t)).collect();
    // Every descriptor that needs a table has one.
    for d in descs.by_name.values() {
        let k = d.kind.as_str();
        if matches!(k, "struct" | "enum" | "root") {
            if !tabled.contains_key(d.base.as_str()) {
                e.push(format!(
                    "{}: a `{k}` descriptor with no heading in digest-kinds.md",
                    d.name
                ));
            }
        } else if !UNTABLED.contains(&k) {
            e.push(format!(
                "{}: descriptor kind `{k}` is neither tabled nor listed in §1.1",
                d.name
            ));
        }
    }
    let nothing: HashSet<&str> = doc.nothing_pins.iter().map(String::as_str).collect();
    for t in &doc.types {
        let ds: Vec<_> = descs.of(&t.path).collect();
        let reached = ds.iter().any(|d| d.from == From::Ast);
        let registered = ds.iter().any(|d| d.from == From::Extra);
        let want = if reached {
            Pin::Schema
        } else if registered {
            Pin::Extra
        } else {
            Pin::None
        };
        let line = t.line;
        match (t.pin, want) {
            (p, w) if p == w => {}
            (Pin::Extra, Pin::None) => e.push(at(
                line,
                format!("`{}`: `pin=extra`, but registering it gives no descriptor", t.path),
            )),
            (Pin::Schema, Pin::None) => e.push(at(
                line,
                format!("`{}`: `pin=schema`, but `Ast`'s descriptor does not reach it", t.path),
            )),
            (p, w) => e.push(at(line, format!("`{}`: `pin={p}`, but it is `pin={w}`", t.path))),
        }
        if t.pin == Pin::None && !nothing.contains(t.short.as_str()) {
            e.push(at(
                line,
                format!("`{}`: `pin=none`, but \"Types nothing pins\" does not list it", t.short),
            ));
        }
        let selector = t.selector.as_deref().map(|s| (s, descs.of(s).next()));
        for d in ds {
            if UNTABLED.contains(&d.kind.as_str()) {
                e.push(format!(
                    "{}: a descriptor of kind `{}` has no table (§1.1)",
                    d.name, d.kind
                ));
                continue;
            }
            let want_kind = match t.kind {
                Kind::Struct | Kind::Consts => "struct",
                Kind::Enum => "enum",
                Kind::Root => "root",
                Kind::View => d.kind.as_str(),
            };
            if d.kind != want_kind {
                e.push(format!(
                    "{}: descriptor kind `{}`, but the table says `kind={}`",
                    d.name,
                    d.kind,
                    kind_word(t.kind)
                ));
                continue;
            }
            match d.kind.as_str() {
                "enum" => variants(t, d, &mut e),
                _ if t.kind == Kind::Consts => {}
                _ => fields(t, d, selector, &mut e),
            }
        }
    }
    e
}

fn kind_word(k: Kind) -> &'static str {
    match k {
        Kind::Struct => "struct",
        Kind::Enum => "enum",
        Kind::Root => "root",
        Kind::View => "view",
        Kind::Consts => "consts",
    }
}

fn variants(ty: &Type, desc: &super::kiln::Desc, errors: &mut Vec<String>) {
    let mut seen = HashSet::new();
    for r in &ty.rows {
        let (variant, field) = match r.member.split_once('.') {
            Some((v, f)) => (v, Some(f)),
            None => (r.member.as_str(), None),
        };
        let Some((_, fields)) = desc.variants.iter().find(|(n, _)| n == variant) else {
            if !r.is_retired() {
                errors.push(at(
                    r.line,
                    format!("`{}`: `{variant}` is not a variant of `{}`", r.text, desc.name),
                ));
            }
            continue;
        };
        match field {
            None => {
                seen.insert(variant.to_owned());
            }
            Some(f) if fields.iter().any(|x| x == f) => {
                seen.insert(format!("{variant}.{f}"));
            }
            Some(f) => errors.push(at(
                r.line,
                format!("`{}`: `{f}` is not a field of `{}::{variant}`", r.text, desc.name),
            )),
        }
    }
    for (variant, fields) in &desc.variants {
        if !seen.contains(variant) {
            errors.push(format!("{}: variant `{variant}` has no row", desc.name));
        }
        for f in fields {
            if !seen.contains(&format!("{variant}.{f}")) {
                errors.push(format!("{}: field `{variant}.{f}` has no row", desc.name));
            }
        }
    }
}

fn fields(
    t: &Type,
    d: &super::kiln::Desc,
    selector: Option<(&str, Option<&super::kiln::Desc>)>,
    e: &mut Vec<String>,
) {
    let mut seen = HashSet::new();
    for r in &t.rows {
        if let (Some(k), Some((s, sel))) = (&r.kind, selector) {
            match sel {
                Some(sel) if sel.kind == "enum" && !sel.variants.iter().any(|(n, _)| n == k) => {
                    e.push(at(r.line, format!("`{}`: `{k}` is not a variant of `{s}`", r.text)));
                }
                _ => {}
            }
        }
        if d.fields.iter().any(|f| f == &r.member) {
            seen.insert(r.member.as_str());
        } else if !r.is_retired() {
            e.push(at(
                r.line,
                format!("`{}`: `{}` is not a field of `{}`", r.text, r.member, d.name),
            ));
        }
    }
    for f in &d.fields {
        if !seen.contains(f.as_str()) {
            e.push(format!("{}: field `{f}` has no row", d.name));
        }
    }
}

/// The test name of every row that obliges a directory (§1.5), with the
/// directory's root and the row's line.
pub fn obligations(doc: &Doc) -> BTreeMap<String, (&'static str, usize)> {
    let mut out = BTreeMap::new();
    for t in &doc.types {
        for r in &t.rows {
            if let Some(dir) = r.obliges() {
                out.insert(format!("{}/{}", t.short, r.text), (dir, r.line));
            }
        }
    }
    out
}

/// The test roots of §1.5.
pub const ROOTS: &[&str] = &["differ", "equal", "demand", "indeterminate"];

/// The names in `untested.txt`, with their lines; malformed lines are
/// errors.
pub fn untested(text: &str) -> (Vec<(String, usize)>, Vec<String>) {
    let (mut names, mut e) = (vec![], vec![]);
    let mut comments = true;
    let mut prev: Option<String> = None;
    for (k, line) in text.lines().enumerate() {
        let n = k + 1;
        if line.starts_with('#') {
            if !comments {
                e.push(format!("untested.txt:{n}: a comment is allowed at the top only"));
            }
            continue;
        }
        comments = false;
        if line.trim() != line || line.is_empty() || !line.contains('/') {
            e.push(format!("untested.txt:{n}: `{line}` is not `<short name>/<row>`"));
            continue;
        }
        if let Some(p) = &prev
            && p.as_str() >= line
        {
            e.push(format!("untested.txt:{n}: `{line}` is out of order or repeated"));
        }
        prev = Some(line.to_owned());
        names.push((line.to_owned(), n));
    }
    (names, e)
}

/// Obligation 5 with the ratchet: `dirs` is every `<root>/<short>/<row>`
/// under `tests/` that holds a file.
pub fn ratchet(doc: &Doc, untested_text: &str, dirs: &BTreeSet<(String, String)>) -> Vec<String> {
    let want = obligations(doc);
    let (listed, mut e) = untested(untested_text);
    let listed_set: HashMap<&str, usize> = listed.iter().map(|(n, l)| (n.as_str(), *l)).collect();
    for (name, (root, line)) in &want {
        let has = dirs.contains(&((*root).to_owned(), name.clone()));
        match (has, listed_set.get(name.as_str())) {
            (false, None) => e.push(at(*line, format!("`{name}` has no test: `tests/{root}/{name}/` is missing (or list it in untested.txt)"))),
            (true, Some(l)) => e.push(format!("untested.txt:{l}: `{name}` has its test `tests/{root}/{name}/`: remove the line (SLIP_BLESS=1 does)")),
            _ => {}
        }
    }
    for (name, l) in &listed {
        if !want.contains_key(name) {
            e.push(format!(
                "untested.txt:{l}: `{name}` is no row's test name, or its row obliges no test"
            ));
        }
    }
    for (root, name) in dirs {
        if want.get(name).is_none_or(|(r, _)| r != root) {
            e.push(format!("tests/{root}/{name}/: no row names this test"));
        }
    }
    e
}

/// What `SLIP_BLESS=1` writes to `untested.txt`: the lines whose
/// directory does not exist; nothing is added.
pub fn bless_untested(doc: &Doc, untested_text: &str, dirs: &BTreeSet<(String, String)>) -> String {
    let want = obligations(doc);
    let mut out = String::new();
    for line in untested_text.lines() {
        let done = want
            .get(line)
            .is_some_and(|(root, _)| dirs.contains(&((*root).to_owned(), line.to_owned())));
        if !done {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}
