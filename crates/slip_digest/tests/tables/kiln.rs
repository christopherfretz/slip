//! Kiln's side of the tables test: the schema descriptors
//! (`kiln_snap::Schema::hashes()`) parsed from their text, so that the
//! checks can also run on descriptors written in a test.

use std::collections::BTreeMap;

/// Where a descriptor came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum From {
    /// Reached from `kiln_ast::ast::Ast`'s descriptor.
    Ast,
    /// Reached only from a type the test registers because its `pin` is
    /// `extra`.
    Extra,
}

/// One descriptor: `<type name> = <kind> <items>`.
#[derive(Clone, Debug)]
pub struct Desc {
    /// `std::any::type_name`, generic arguments included.
    pub name: String,
    /// The name without generic arguments.
    pub base: String,
    /// `struct`, `enum`, `root`, `id`, `seq`, ...
    pub kind: String,
    /// The fields of a struct or root, in order.
    pub fields: Vec<String>,
    /// The variants of an enum and their fields, in order.
    pub variants: Vec<(String, Vec<String>)>,
    /// The `SCHEMA_HASH` the type declared.
    pub hash: Option<u64>,
    pub from: From,
}

/// The descriptor kinds §1.1 lists as having no table.
pub const UNTABLED: &[&str] = &[
    "id", "int", "float", "bool", "char", "nonzero", "unit", "phantom", "tuple", "array", "option",
    "result", "box", "seq", "str", "path", "cell", "once", "lock", "atomic", "map", "set",
    "ordmap", "ordset", "shared",
];

/// Every descriptor, by type name.
#[derive(Debug, Default)]
pub struct Descs {
    pub by_name: BTreeMap<String, Desc>,
}

impl Descs {
    /// Reads descriptors as `Schema::hashes()` gives them: those reached
    /// from `Ast`, then those of the `extra` registrations (a type already
    /// reached from `Ast` stays `From::Ast`).
    pub fn parse(
        ast: &[(&str, Option<u64>)],
        extra: &[(&str, Option<u64>)],
    ) -> Result<Descs, String> {
        let mut d = Descs::default();
        for (list, from) in [(ast, From::Ast), (extra, From::Extra)] {
            for &(text, hash) in list {
                let desc = parse_desc(text, hash, from)?;
                d.by_name.entry(desc.name.clone()).or_insert(desc);
            }
        }
        Ok(d)
    }

    /// Every instantiation of the type whose path is `base`.
    pub fn of(&self, base: &str) -> impl Iterator<Item = &Desc> {
        let base = base.to_owned();
        self.by_name.values().filter(move |d| d.base == base)
    }
}

/// Splits at spaces outside `<>`, `[]` and `()`.
fn tokens(s: &str) -> Vec<&str> {
    let (mut out, mut depth, mut start) = (vec![], 0i32, 0);
    for (k, c) in s.char_indices() {
        match c {
            '<' | '[' | '(' => depth += 1,
            '>' | ']' | ')' => depth -= 1,
            ' ' if depth == 0 => {
                if k > start {
                    out.push(&s[start..k]);
                }
                start = k + 1;
            }
            _ => {}
        }
    }
    if start < s.len() {
        out.push(&s[start..]);
    }
    out
}

/// A field item `name:type` (`name:skip:type`, `name:with(path):type`).
fn field_name(tok: &str) -> Option<&str> {
    let (name, ty) = tok.split_once(':')?;
    let ok = !name.is_empty()
        && !ty.is_empty()
        && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'.');
    ok.then_some(name)
}

/// One descriptor's text.
pub fn parse_desc(text: &str, hash: Option<u64>, from: From) -> Result<Desc, String> {
    let (name, rest) =
        text.split_once(" = ").ok_or_else(|| format!("descriptor `{text}`: no ` = `"))?;
    let toks = tokens(rest);
    let kind = (*toks.first().ok_or_else(|| format!("{name}: an empty descriptor"))?).to_owned();
    let base = match name.find('<') {
        Some(k) if name.starts_with("kiln_") => name[..k].to_owned(),
        _ => name.to_owned(),
    };
    let mut d =
        Desc { name: name.to_owned(), base, kind, fields: vec![], variants: vec![], hash, from };
    let mut note = false;
    let mut k = 1;
    while k < toks.len() {
        let t = toks[k];
        k += 1;
        if t == "|" {
            let v = toks.get(k).ok_or_else(|| format!("{name}: `|` without a variant"))?;
            k += 1;
            let v = v.split_once('=').map_or(*v, |(v, _)| v);
            d.variants.push((v.to_owned(), vec![]));
            note = false;
        } else if t.starts_with('#') {
            note = true;
        } else if let Some(f) = field_name(t) {
            note = false;
            match d.variants.last_mut() {
                Some((_, fs)) => fs.push(f.to_owned()),
                None => d.fields.push(f.to_owned()),
            }
        } else if !note {
            return Err(format!("{name}: cannot read `{t}` in its descriptor"));
        }
    }
    Ok(d)
}
