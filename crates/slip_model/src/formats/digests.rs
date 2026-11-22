//! The `digests` section, id 19 of the shard (`spec/digest.md` §11).
//! Work package WP-015; the binary form, invariants, offsets and paths are
//! `spec/model.md` §8.2.

use crate::digests::{Digests, Group, GroupClass, SCHEME, Unit, group_digest, tu_digest};
use crate::json::Value;
use crate::wire::{Decoder, Encoder};

use super::At;
use super::common::{
    Enumeration, JResult, Result, dec_enum, dec_list, dec_opt, enc_enum, json_bytes, json_digest,
    json_enum, json_opt, json_uv, object, read_bytes, read_digest, read_enum, read_list, read_opt,
    read_record, read_uv,
};

/// The section id (`spec/model.md` §8).
pub(super) const ID: u64 = 19;

impl Enumeration for GroupClass {
    const TABLE: &'static [(Self, u64, &'static str)] = &[
        (GroupClass::Declared, 0, "declared"),
        (GroupClass::Derived, 1, "derived"),
        (GroupClass::Tu, 2, "tu"),
    ];
    const UNLISTED: &'static str = "unlisted GroupClass value";
    const EXPECTED: &'static str = "expected one of \"declared\", \"derived\", \"tu\"";
}

// ---------------------------------------------------------------------------
// Invariants (docs/wp/WP-015.md, in that order)

/// Where an invariant is violated: a field or element of the section.
#[derive(Clone, Copy, Debug)]
pub(super) enum Loc {
    Section,
    Scheme,
    Tu,
    Group(usize),
    GroupFile(usize),
    GroupCount(usize),
    GroupIndeterminate(usize),
    GroupDigest(usize),
    Units(usize),
    Unit(usize, usize),
    UnitFlags(usize, usize),
}

impl Loc {
    pub(super) fn path(self) -> String {
        const S: &str = "sections.digests";
        match self {
            Loc::Section => S.to_owned(),
            Loc::Scheme => format!("{S}.scheme"),
            Loc::Tu => format!("{S}.tu"),
            Loc::Group(i) => format!("{S}.groups[{i}]"),
            Loc::GroupFile(i) => format!("{S}.groups[{i}].file"),
            Loc::GroupCount(i) => format!("{S}.groups[{i}].count"),
            Loc::GroupIndeterminate(i) => format!("{S}.groups[{i}].indeterminate"),
            Loc::GroupDigest(i) => format!("{S}.groups[{i}].digest"),
            Loc::Units(i) => format!("{S}.groups[{i}].units"),
            Loc::Unit(i, j) => format!("{S}.groups[{i}].units[{j}]"),
            Loc::UnitFlags(i, j) => format!("{S}.groups[{i}].units[{j}].flags"),
        }
    }
}

/// The first violation of the section's invariants. Each rule is checked
/// over the whole section, in element order, before the next rule.
/// `status_ok`: the shard's outcome is `ok`.
pub(super) fn violation(d: &Digests, status_ok: bool) -> Option<(Loc, &'static str)> {
    // 1.
    if !status_ok {
        return Some((Loc::Section, "a digests section in a shard whose status is not ok"));
    }
    // 2.
    if d.scheme != SCHEME {
        return Some((Loc::Scheme, "unknown digest scheme"));
    }
    let groups = &d.groups;
    // 3.
    if let Some(i) = (1..groups.len())
        .find(|&i| (groups[i - 1].class, &groups[i - 1].file) >= (groups[i].class, &groups[i].file))
    {
        return Some((Loc::Group(i), "groups are not strictly ascending by (class, file)"));
    }
    // 4.
    for (i, g) in groups.iter().enumerate() {
        let bad = match g.class {
            GroupClass::Tu => (!g.file.is_empty()).then_some("a tu group has a non-empty file"),
            GroupClass::Declared => {
                g.file.is_empty().then_some("a declared group has an empty file")
            }
            // Builtins have the empty path; any file is allowed.
            GroupClass::Derived => None,
        };
        if let Some(what) = bad {
            return Some((Loc::GroupFile(i), what));
        }
    }
    // 5a.
    if let Some(i) = groups.iter().position(|g| g.count == 0) {
        return Some((Loc::GroupCount(i), "a group has a count of 0"));
    }
    // 5b.
    if let Some(i) = groups.iter().position(|g| g.indeterminate > g.count) {
        return Some((
            Loc::GroupIndeterminate(i),
            "a group has more indeterminate units than units",
        ));
    }
    // 6a-6e, each part over every group with a unit list.
    let listed = || groups.iter().enumerate().filter_map(|(i, g)| Some((i, g, g.units.as_ref()?)));
    // 6a.
    for (i, _, units) in listed() {
        if let Some(j) = (1..units.len())
            .find(|&j| (units[j - 1].key, units[j - 1].digest) >= (units[j].key, units[j].digest))
        {
            return Some((Loc::Unit(i, j), "units are not strictly ascending by (key, digest)"));
        }
    }
    // 6b.
    for (i, g, units) in listed() {
        if units.len() as u64 != g.count {
            return Some((Loc::Units(i), "the number of units is not the group's count"));
        }
    }
    // 6c.
    for (i, _, units) in listed() {
        if let Some(j) = units.iter().position(|u| u.flags & !Unit::INDETERMINATE != 0) {
            return Some((Loc::UnitFlags(i, j), "unit flags have a bit other than bit 0 set"));
        }
    }
    // 6d.
    for (i, g, units) in listed() {
        let flagged = units.iter().filter(|u| u.is_indeterminate()).count();
        if u64::try_from(flagged).ok() != Some(g.indeterminate) {
            return Some((
                Loc::GroupIndeterminate(i),
                "indeterminate is not the number of the group's units with the flag",
            ));
        }
    }
    // 6e.
    for (i, g, units) in listed() {
        if g.digest != group_digest(units) {
            return Some((Loc::GroupDigest(i), "the group's digest is not that of its units"));
        }
    }
    // 7.
    if d.tu != tu_digest(groups) {
        return Some((Loc::Tu, "tu is not the digest of the groups' sums"));
    }
    None
}

// ---------------------------------------------------------------------------
// Binary

pub(super) fn encode(e: &mut Encoder, d: &Digests) {
    e.uv(d.scheme).digest(&d.tu).count(d.groups.len());
    for g in &d.groups {
        enc_enum(e, g.class);
        e.bytes(&g.file).uv(g.count).uv(g.indeterminate).digest(&g.digest);
        match &g.units {
            None => {
                e.u8(0);
            }
            Some(units) => {
                e.u8(1).count(units.len());
                for u in units {
                    e.digest(&u.key).digest(&u.digest).uv(u.flags);
                }
            }
        }
    }
}

/// File offsets of the fields an invariant can name.
#[derive(Debug, Default)]
pub(super) struct Offsets {
    section: usize,
    tu: usize,
    groups: Vec<GroupOffsets>,
}

#[derive(Debug, Default)]
struct GroupOffsets {
    start: usize,
    file: usize,
    count: usize,
    indeterminate: usize,
    digest: usize,
    /// The unit list's count (after the `opt` tag).
    units: usize,
    unit: Vec<usize>,
    flags: Vec<usize>,
}

impl Offsets {
    pub(super) fn of(&self, l: Loc) -> usize {
        let g = |i: usize| &self.groups[i];
        match l {
            Loc::Section | Loc::Scheme => self.section,
            Loc::Tu => self.tu,
            Loc::Group(i) => g(i).start,
            Loc::GroupFile(i) => g(i).file,
            Loc::GroupCount(i) => g(i).count,
            Loc::GroupIndeterminate(i) => g(i).indeterminate,
            Loc::GroupDigest(i) => g(i).digest,
            Loc::Units(i) => g(i).units,
            Loc::Unit(i, j) => g(i).unit[j],
            Loc::UnitFlags(i, j) => g(i).flags[j],
        }
    }
}

/// The section's payload, without checking the invariants (`violation`).
pub(super) fn decode(d: &mut Decoder<'_>) -> Result<(Digests, Offsets)> {
    let mut o = Offsets { section: d.offset(), ..Offsets::default() };
    let scheme = d.uv()?;
    o.tu = d.offset();
    let tu = d.digest()?;
    let groups = dec_list(d, None, |d| {
        let mut go = GroupOffsets { start: d.offset(), ..GroupOffsets::default() };
        let class = dec_enum(d)?;
        go.file = d.offset();
        let file = d.bytes()?.to_vec();
        go.count = d.offset();
        let count = d.uv()?;
        go.indeterminate = d.offset();
        let indeterminate = d.uv()?;
        go.digest = d.offset();
        let digest = d.digest()?;
        let units = dec_opt(d, |d| {
            go.units = d.offset();
            dec_list(d, Some(&mut go.unit), |d| {
                let key = d.digest()?;
                let digest = d.digest()?;
                go.flags.push(d.offset());
                Ok(Unit { key, digest, flags: d.uv()? })
            })
        })?;
        o.groups.push(go);
        Ok(Group { class, file, count, indeterminate, digest, units })
    })?;
    Ok((Digests { scheme, tu, groups }, o))
}

// ---------------------------------------------------------------------------
// JSON

pub(super) fn render(d: &Digests) -> Value {
    let groups = d
        .groups
        .iter()
        .map(|g| {
            object(vec![
                ("class", json_enum(g.class)),
                ("file", json_bytes(&g.file)),
                ("count", json_uv(g.count)),
                ("indeterminate", json_uv(g.indeterminate)),
                ("digest", json_digest(&g.digest)),
                (
                    "units",
                    json_opt(g.units.as_ref(), |units| {
                        Value::Array(
                            units
                                .iter()
                                .map(|u| {
                                    object(vec![
                                        ("key", json_digest(&u.key)),
                                        ("digest", json_digest(&u.digest)),
                                        ("flags", json_uv(u.flags)),
                                    ])
                                })
                                .collect(),
                        )
                    }),
                ),
            ])
        })
        .collect();
    object(vec![
        ("scheme", json_uv(d.scheme)),
        ("tu", json_digest(&d.tu)),
        ("groups", Value::Array(groups)),
    ])
}

/// The section's JSON, without checking the invariants (`violation`).
/// Members are read in field order, so the first wrong one is reported.
pub(super) fn read(v: &Value, at: &At<'_>) -> JResult<Digests> {
    let [scheme, tu, groups] = read_record(v, at, ["scheme", "tu", "groups"])?;
    let scheme = read_uv(scheme, &at.key("scheme"))?;
    let tu = read_digest(tu, &at.key("tu"))?;
    let groups = read_list(groups, &at.key("groups"), |v, at| {
        let [class, file, count, indeterminate, digest, units] =
            read_record(v, at, ["class", "file", "count", "indeterminate", "digest", "units"])?;
        Ok(Group {
            class: read_enum(class, &at.key("class"))?,
            file: read_bytes(file, &at.key("file"))?,
            count: read_uv(count, &at.key("count"))?,
            indeterminate: read_uv(indeterminate, &at.key("indeterminate"))?,
            digest: read_digest(digest, &at.key("digest"))?,
            units: read_opt(units, &at.key("units"), |v, at| {
                read_list(v, at, |v, at| {
                    let [key, digest, flags] = read_record(v, at, ["key", "digest", "flags"])?;
                    Ok(Unit {
                        key: read_digest(key, &at.key("key"))?,
                        digest: read_digest(digest, &at.key("digest"))?,
                        flags: read_uv(flags, &at.key("flags"))?,
                    })
                })
            })?,
        })
    })?;
    Ok(Digests { scheme, tu, groups })
}
