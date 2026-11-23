//! Test support for `tests/tables.rs`: a parser of `spec/digest-kinds.md`
//! by the grammar of its §1, the checks of digest §13 obligations 4 and 5,
//! and the generator of `src/tables.rs` (WP-023).
//!
//! The parser knows nothing of Kiln: it reads the file into [`Doc`]. Kiln's
//! side is [`kiln::Descs`], the schema descriptors as text, so that every
//! check can be run on small tables and descriptors written in a test.

#![allow(dead_code)]

pub mod check;
#[rustfmt::skip]
pub mod extra;
pub mod generate;
pub mod kiln;

use std::fmt;

/// The whole file.
#[derive(Debug, Default)]
pub struct Doc {
    /// Every Kiln type, in file order.
    pub types: Vec<Type>,
    /// §3.1, `KeyKind`.
    pub key_kinds: Vec<Number>,
    /// §3.2, `Name`.
    pub names: Vec<Number>,
    /// §11.1, the resolved properties.
    pub properties: Vec<Number>,
    /// §1.7, the rows of the walk: tags [`WALK_BASE`] plus their position.
    pub walk: Vec<Number>,
    /// The code spans of "Open questions / Types nothing pins".
    pub nothing_pins: Vec<String>,
    /// Lines that look like part of the grammar and do not parse.
    pub errors: Vec<String>,
}

/// A level-3 heading naming a Kiln type, its property line and its table.
#[derive(Debug)]
pub struct Type {
    pub line: usize,
    /// The path as the heading writes it, `kiln_ast::decl::DeclKind`.
    pub path: String,
    /// The last segment of the path.
    pub short: String,
    pub kind: Kind,
    pub pin: Pin,
    pub selector: Option<String>,
    pub base: Option<u32>,
    pub rows: Vec<Row>,
}

impl Type {
    /// Whether the table's rows are variants (§1.2: kinds enum, view, consts).
    pub fn has_variants(&self) -> bool {
        matches!(self.kind, Kind::Enum | Kind::View | Kind::Consts)
    }

    /// The variant rows (§1.4), in table order.
    pub fn variant_rows(&self) -> impl Iterator<Item = &Row> {
        let v = self.has_variants();
        self.rows.iter().filter(move |r| v && r.is_variant())
    }

    /// The number of the variant `name` (§1.4): its position among the
    /// variant rows, from 1, plus `base`.
    pub fn number_of(&self, name: &str) -> Option<u32> {
        let at = self.variant_rows().position(|r| r.member == name)?;
        Some(self.base.unwrap_or(0) + u32::try_from(at).ok()? + 1)
    }
}

/// The `kind` of a property line (§1.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Struct,
    Enum,
    Root,
    View,
    Consts,
}

/// The `pin` of a property line (§1.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pin {
    Schema,
    Extra,
    None,
}

impl fmt::Display for Pin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Pin::Schema => "schema",
            Pin::Extra => "extra",
            Pin::None => "none",
        })
    }
}

/// One row of a type's table (§1.2).
#[derive(Debug, Clone)]
pub struct Row {
    pub line: usize,
    /// The row cell without its backticks.
    pub text: String,
    /// The `kind` prefix (a variant of the selector).
    pub kind: Option<String>,
    /// The member: a field, a variant or `variant.field`.
    pub member: String,
    pub fact: Option<String>,
    pub class: Class,
    pub decision: Decision,
    pub encoding: Encoding,
    pub note: String,
}

impl Row {
    /// A row of a variant itself, in a table of kind enum, view or consts:
    /// no field, no fact.
    pub fn is_variant(&self) -> bool {
        self.kind.is_none() && self.fact.is_none() && !self.member.contains('.')
    }

    /// §1.4: a variant Kiln removed.
    pub fn is_retired(&self) -> bool {
        self.decision == Decision::Unsupported
            && self.encoding == Encoding::Unbuilt
            && self.note == "retired"
    }

    /// The directory of §1.5 this row obliges, if any.
    pub fn obliges(&self) -> Option<&'static str> {
        match (self.decision, &self.encoding) {
            (Decision::In, _) => Some("differ"),
            (Decision::Out, _) if self.class == Class::Written => Some("equal"),
            (Decision::Symbolic, _) => Some("equal"),
            (Decision::Demand, _) => Some("demand"),
            (Decision::Unsupported, Encoding::Unbuilt) | (Decision::Out, _) => None,
            (Decision::Unsupported, _) => Some("indeterminate"),
        }
    }
}

/// The Class column (§1.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Class {
    Entity,
    Type,
    Value,
    Child,
    Name,
    Flag,
    Written,
    Location,
    Bookkeeping,
}

/// The Decision column (§1.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decision {
    In,
    Out,
    Symbolic,
    Demand,
    Unsupported,
}

impl Decision {
    pub fn rust(self) -> &'static str {
        match self {
            Decision::In => "In",
            Decision::Out => "Out",
            Decision::Symbolic => "Symbolic",
            Decision::Demand => "Demand",
            Decision::Unsupported => "Unsupported",
        }
    }
}

/// The Encoding column (§1.2, §1.3).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Encoding {
    /// `-`
    None,
    /// `unbuilt`
    Unbuilt,
    /// `as V`
    As(String),
    /// `tag N`
    Tag(u32),
    /// A form of §1.3 and its suffixes, innermost first.
    Form(Base, Vec<Suffix>),
}

/// The forms of §1.3 other than `tag N`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Base {
    Bit,
    Bool,
    Uv,
    Sv,
    Bytes,
    Sym,
    String,
    Name,
    Ref,
    Key,
    Keyed,
    Type,
    Value,
    Child,
    Decl,
    Enum(String),
    Record(String),
    Attrs,
    Profile,
}

/// `?`, `*` and `*sorted` (§1.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Suffix {
    Opt,
    List,
    Sorted,
}

/// A row of §1.7, §3.1, §3.2 or §11.1: Slip's own numbers.
#[derive(Debug, Clone)]
pub struct Number {
    pub line: usize,
    pub value: u32,
    /// `None` for a retired number.
    pub name: Option<String>,
}

/// §1.7: the base of the walk's own tags, the next range of 256 after
/// the node enumerations' (§1.4).
pub const WALK_BASE: u32 = 1536;

/// The header and separator every type's table has (§1.2).
pub const HEADER: &str = "| Row | Class | Decision | Encoding | Note |";
const SEPARATOR: &str = "|---|---|---|---|---|";

/// Parses the file. Never fails: what does not parse is in `errors`, each
/// `digest-kinds.md:<line>: <what>`.
pub fn parse(text: &str) -> Doc {
    let mut doc = Doc::default();
    let lines: Vec<&str> = text.lines().collect();
    let mut fenced = false;
    let mut i = 0;
    // The section a Slip table of §3 or §11.1 is read from.
    let mut slip: Option<&'static str> = None;
    let mut in_nothing_pins = false;
    while i < lines.len() {
        let line = lines[i];
        let n = i + 1;
        i += 1;
        if line.starts_with("```") {
            fenced = !fenced;
            continue;
        }
        if fenced {
            continue;
        }
        if let Some((level, title)) = heading(line) {
            slip = match title {
                t if level == 3 && t.starts_with("3.1 ") => Some("key_kinds"),
                t if level == 3 && t.starts_with("3.2 ") => Some("names"),
                t if level == 3 && t.starts_with("11.1 ") => Some("properties"),
                t if level == 3 && t.starts_with("1.7 ") => Some("walk"),
                _ => None,
            };
            in_nothing_pins = level == 3 && title == "Types nothing pins";
            if let Some(path) = kiln_path(title) {
                if level != 3 {
                    doc.errors
                        .push(err(n, format!("a Kiln type's heading is level 3, not {level}")));
                    continue;
                }
                let (ty, next) = parse_type(&lines, i, n, path, &mut doc.errors);
                if let Some(ty) = ty {
                    doc.types.push(ty);
                }
                i = next;
            }
            continue;
        }
        if in_nothing_pins {
            doc.nothing_pins.extend(code_spans(line).map(str::to_owned));
        }
        if let Some(which) = slip
            && line.starts_with('|')
            && let Some(row) = parse_number(line, n, which, &mut doc.errors)
        {
            match which {
                "key_kinds" => doc.key_kinds.push(row),
                "names" => doc.names.push(row),
                "walk" => doc.walk.push(row),
                _ => doc.properties.push(row),
            }
        }
    }
    doc
}

fn err(line: usize, what: impl fmt::Display) -> String {
    format!("digest-kinds.md:{line}: {what}")
}

/// A Markdown ATX heading: its level and text.
fn heading(line: &str) -> Option<(usize, &str)> {
    let level = line.bytes().take_while(|&b| b == b'#').count();
    if level == 0 || level > 6 {
        return None;
    }
    let rest = &line[level..];
    rest.strip_prefix(' ').map(|t| (level, t.trim()))
}

/// The heading text is one code span holding a path that begins `kiln_`.
fn kiln_path(title: &str) -> Option<&str> {
    let inner = title.strip_prefix('`')?.strip_suffix('`')?;
    (!inner.contains('`') && inner.starts_with("kiln_")).then_some(inner)
}

fn code_spans(line: &str) -> impl Iterator<Item = &str> {
    line.split('`').skip(1).step_by(2)
}

fn is_ident(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

fn is_rust_path(s: &str) -> bool {
    s.split("::").all(|seg| is_ident(seg) && !seg.as_bytes()[0].is_ascii_digit())
}

/// Reads the property line and the table after a type's heading at line
/// `head` (1-based); `at` is the index of the line after the heading. Returns
/// the type and the index of the line where the caller resumes: the next
/// heading.
fn parse_type(
    lines: &[&str],
    mut at: usize,
    head: usize,
    path: &str,
    errors: &mut Vec<String>,
) -> (Option<Type>, usize) {
    let resume = |mut j: usize| {
        while j < lines.len() && heading(lines[j]).is_none() {
            j += 1;
        }
        j
    };
    if !is_rust_path(path) {
        errors.push(err(head, format!("`{path}` is not a Rust path without generic arguments")));
        return (None, resume(at));
    }
    while at < lines.len() && lines[at].trim().is_empty() {
        at += 1;
    }
    let Some(props) = lines.get(at).and_then(|l| l.strip_prefix('`')?.strip_suffix('`')) else {
        errors.push(err(at + 1, format!("`{path}`: no property line after the heading")));
        return (None, resume(at));
    };
    let props_line = at + 1;
    at += 1;
    let Some((kind, pin, selector, base)) = properties(props, props_line, errors) else {
        return (None, resume(at));
    };
    // The first table after the property line, before the next heading.
    while at < lines.len() && heading(lines[at]).is_none() && !lines[at].starts_with('|') {
        at += 1;
    }
    if lines.get(at).copied() != Some(HEADER) {
        errors.push(err(at + 1, format!("`{path}`: no table `{HEADER}` before the next heading")));
        return (None, resume(at));
    }
    if lines.get(at + 1).copied() != Some(SEPARATOR) {
        errors.push(err(at + 2, format!("the table's separator is `{SEPARATOR}`")));
    }
    at += 2;
    let short = path.rsplit("::").next().unwrap_or(path).to_owned();
    let mut ty =
        Type { line: head, path: path.to_owned(), short, kind, pin, selector, base, rows: vec![] };
    while at < lines.len() && lines[at].starts_with('|') {
        if let Some(row) = parse_row(lines[at], at + 1, &ty, errors) {
            ty.rows.push(row);
        }
        at += 1;
    }
    // Nothing else under the heading may look like a table row.
    let end = resume(at);
    for (j, l) in lines.iter().enumerate().take(end).skip(at) {
        if l.starts_with('|') {
            errors.push(err(j + 1, format!("`{path}`: a table row outside the type's table")));
        }
    }
    (Some(ty), end)
}

/// The properties of a property line (§1.1).
type Properties = (Kind, Pin, Option<String>, Option<u32>);

fn properties(props: &str, pn: usize, errors: &mut Vec<String>) -> Option<Properties> {
    let (mut kind, mut pin, mut selector, mut base) = (None, None, None, None);
    for pair in props.split("; ") {
        let Some((k, v)) = pair.split_once('=') else {
            errors.push(err(pn, format!("property `{pair}` is not `key=value`")));
            continue;
        };
        let dup = match k {
            "kind" => kind
                .replace(match v {
                    "struct" => Kind::Struct,
                    "enum" => Kind::Enum,
                    "root" => Kind::Root,
                    "view" => Kind::View,
                    "consts" => Kind::Consts,
                    _ => {
                        errors.push(err(pn, format!("unknown kind `{v}`")));
                        continue;
                    }
                })
                .is_some(),
            "pin" => pin
                .replace(match v {
                    "schema" => Pin::Schema,
                    "extra" => Pin::Extra,
                    "none" => Pin::None,
                    _ => {
                        errors.push(err(pn, format!("unknown pin `{v}`")));
                        continue;
                    }
                })
                .is_some(),
            "selector" => {
                if !is_rust_path(v) || !v.starts_with("kiln_") {
                    errors.push(err(pn, format!("selector `{v}` is not a Kiln path")));
                }
                selector.replace(v.to_owned()).is_some()
            }
            "base" => {
                let Some(b) = number(v) else {
                    errors.push(err(pn, format!("base `{v}` is not a number")));
                    continue;
                };
                base.replace(b).is_some()
            }
            _ => {
                errors.push(err(pn, format!("unknown property `{k}`")));
                continue;
            }
        };
        if dup {
            errors.push(err(pn, format!("property `{k}` given twice")));
        }
    }
    let (Some(kind), Some(pin)) = (kind, pin) else {
        errors.push(err(pn, "the property line needs `kind` and `pin`"));
        return None;
    };
    Some((kind, pin, selector, base))
}

fn number(s: &str) -> Option<u32> {
    if s.is_empty() || (s.len() > 1 && s.starts_with('0')) || !s.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    s.parse().ok()
}

/// The cells of a table row, without the outer pipes; a `|` inside a code
/// span does not separate.
fn cells(line: &str) -> Option<Vec<&str>> {
    let inner = line.strip_prefix('|')?.strip_suffix('|')?;
    let mut out = vec![];
    let (mut start, mut code) = (0, false);
    for (k, c) in inner.char_indices() {
        match c {
            '`' => code = !code,
            '|' if !code => {
                out.push(inner[start..k].trim());
                start = k + 1;
            }
            _ => {}
        }
    }
    out.push(inner[start..].trim());
    Some(out)
}

fn parse_row(line: &str, n: usize, ty: &Type, errors: &mut Vec<String>) -> Option<Row> {
    let mut fail = |what: String| {
        errors.push(err(n, what));
        None
    };
    let Some(c) = cells(line) else {
        return fail("a table row starts and ends with `|`".into());
    };
    if c.len() != 5 {
        return fail(format!("a row has 5 cells, this one {}", c.len()));
    }
    let Some(text) = c[0].strip_prefix('`').and_then(|t| t.strip_suffix('`')) else {
        return fail(format!("the Row cell `{}` is not one code span", c[0]));
    };
    if text.contains('`') {
        return fail(format!("the Row cell `{}` is not one code span", c[0]));
    }
    let (head, fact) = match text.split_once('-') {
        Some((h, f)) => (h, Some(f)),
        None => (text, None),
    };
    if let Some(f) = fact
        && !f.split('-').all(|w| {
            !w.is_empty() && w.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
        })
    {
        return fail(format!("`{text}`: the fact `{f}` is not lower-case words joined by `-`"));
    }
    let (kind, member) = match head.split_once('.') {
        Some((k, m)) if ty.selector.is_some() => (Some(k.to_owned()), m.to_owned()),
        _ => (None, head.to_owned()),
    };
    let segs: Vec<&str> = member.split('.').collect();
    if !segs.iter().all(|s| is_ident(s)) || kind.as_deref().is_some_and(|k| !is_ident(k)) {
        return fail(format!("`{text}`: not `[kind.]member[-fact]`"));
    }
    if ty.has_variants() && (segs.len() > 2 || ty.selector.is_some()) {
        return fail(format!("`{text}`: a row of a variant table is `variant` or `variant.field`"));
    }
    let class = match c[1] {
        "entity" => Class::Entity,
        "type" => Class::Type,
        "value" => Class::Value,
        "child" => Class::Child,
        "name" => Class::Name,
        "flag" => Class::Flag,
        "written" => Class::Written,
        "location" => Class::Location,
        "bookkeeping" => Class::Bookkeeping,
        w => return fail(format!("`{text}`: unknown class `{w}`")),
    };
    let decision = match c[2] {
        "in" => Decision::In,
        "out" => Decision::Out,
        "symbolic" => Decision::Symbolic,
        "demand" => Decision::Demand,
        "unsupported" => Decision::Unsupported,
        w => return fail(format!("`{text}`: unknown decision `{w}`")),
    };
    let encoding = match parse_encoding(c[3]) {
        Ok(e) => e,
        Err(e) => return fail(format!("`{text}`: {e}")),
    };
    Some(Row {
        line: n,
        text: text.to_owned(),
        kind,
        member,
        fact: fact.map(str::to_owned),
        class,
        decision,
        encoding,
        note: c[4].to_owned(),
    })
}

/// An Encoding cell (§1.2, §1.3).
pub fn parse_encoding(cell: &str) -> Result<Encoding, String> {
    match cell {
        "-" => return Ok(Encoding::None),
        "unbuilt" => return Ok(Encoding::Unbuilt),
        _ => {}
    }
    if let Some(v) = cell.strip_prefix("as ") {
        return if is_ident(v) {
            Ok(Encoding::As(v.to_owned()))
        } else {
            Err(format!("`as {v}`: not a variant name"))
        };
    }
    if let Some(t) = cell.strip_prefix("tag ") {
        return number(t).map(Encoding::Tag).ok_or_else(|| format!("`tag {t}`: not a number"));
    }
    let (base, rest) =
        if let Some(r) = cell.strip_prefix("enum ").or_else(|| cell.strip_prefix("record ")) {
            let end = r.find(|c: char| !(c.is_ascii_alphanumeric() || c == '_')).unwrap_or(r.len());
            let t = r[..end].to_owned();
            if t.is_empty() {
                return Err(format!("`{cell}`: no type name"));
            }
            (if cell.starts_with("enum ") { Base::Enum(t) } else { Base::Record(t) }, &r[end..])
        } else {
            let end = cell.find(|c: char| !c.is_ascii_lowercase()).unwrap_or(cell.len());
            let b = match &cell[..end] {
                "bit" => Base::Bit,
                "bool" => Base::Bool,
                "uv" => Base::Uv,
                "sv" => Base::Sv,
                "bytes" => Base::Bytes,
                "sym" => Base::Sym,
                "string" => Base::String,
                "name" => Base::Name,
                "ref" => Base::Ref,
                "key" => Base::Key,
                "keyed" => Base::Keyed,
                "type" => Base::Type,
                "value" => Base::Value,
                "child" => Base::Child,
                "decl" => Base::Decl,
                "attrs" => Base::Attrs,
                "profile" => Base::Profile,
                _ => return Err(format!("unknown encoding `{cell}`")),
            };
            (b, &cell[end..])
        };
    let mut suffixes = vec![];
    let mut s = rest;
    while !s.is_empty() {
        if let Some(r) = s.strip_prefix("*sorted") {
            suffixes.push(Suffix::Sorted);
            s = r;
        } else if let Some(r) = s.strip_prefix('*') {
            suffixes.push(Suffix::List);
            s = r;
        } else if let Some(r) = s.strip_prefix('?') {
            suffixes.push(Suffix::Opt);
            s = r;
        } else {
            return Err(format!("unknown encoding `{cell}`"));
        }
    }
    if matches!(base, Base::Bit | Base::Keyed) && !suffixes.is_empty() {
        return Err(format!("`{cell}`: `bit` and `keyed` take no suffix"));
    }
    Ok(Encoding::Form(base, suffixes))
}

/// A row of §1.7, §3.1, §3.2 or §11.1: `| N | `name` | ... |`, or
/// `retired` in §11.1.
fn parse_number(line: &str, n: usize, which: &str, errors: &mut Vec<String>) -> Option<Number> {
    let c = cells(line)?;
    if c.len() < 2 || c[0].bytes().all(|b| b == b'-') || matches!(c[0], "Tag" | "Number") {
        return None;
    }
    let Some(number) = number(c[0]) else {
        errors.push(err(n, format!("`{}` is not a number", c[0])));
        return None;
    };
    let name = match c[1].strip_prefix('`').and_then(|t| t.strip_suffix('`')) {
        Some(t) if is_ident(t) => Some(t.to_owned()),
        _ if c[1] == "retired" && which == "properties" => None,
        _ => {
            errors.push(err(n, format!("`{}` is not one code span holding a name", c[1])));
            return None;
        }
    };
    Some(Number { line: n, value: number, name })
}

/// `CamelCase` to `snake_case`: an underscore before an upper-case letter
/// that follows a lower-case letter or a digit, or that starts a word
/// after a run of upper-case letters (`CXXRecord` is `cxx_record`).
pub fn snake(s: &str) -> String {
    let b: Vec<char> = s.chars().collect();
    let mut out = String::new();
    for (k, &c) in b.iter().enumerate() {
        if c.is_ascii_uppercase() && k > 0 {
            let prev = b[k - 1];
            let next_lower = b.get(k + 1).is_some_and(char::is_ascii_lowercase);
            if prev.is_ascii_lowercase()
                || prev.is_ascii_digit()
                || (prev.is_ascii_uppercase() && next_lower)
            {
                out.push('_');
            }
        }
        out.push(c.to_ascii_lowercase());
    }
    out
}

/// A constant's name: `SCREAMING_SNAKE_CASE`.
pub fn screaming(s: &str) -> String {
    if s.bytes().any(|b| b.is_ascii_lowercase()) {
        snake(s).to_ascii_uppercase()
    } else {
        s.to_owned()
    }
}
