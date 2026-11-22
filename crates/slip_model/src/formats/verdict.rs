//! The `verdict` section, id 21 of a verify shard (`spec/verify.md` §6).
//! Work package WP-015; the rules the spec leaves open are recorded in
//! `spec/landed/model/WP-015.md`.

use std::collections::BTreeSet;

use crate::json::Value;
use crate::verdict::{Finding, FindingKind, Site, Verdict, VerdictKind, Where};
use crate::wire::{Decoder, Encoder};
use crate::{Phase, Status};

use super::At;
use super::MESSAGE_MAX;
use super::common::{
    Enumeration, JResult, Result, dec_enum, dec_list, dec_opt, enc_enum, json_bytes, json_digest,
    json_enum, json_opt, json_uv, object, read_bytes, read_digest, read_enum, read_list, read_opt,
    read_record, read_str, read_uv,
};

/// The section id (`spec/model.md` §8).
pub(super) const ID: u64 = 21;

impl Enumeration for VerdictKind {
    const TABLE: &'static [(Self, u64, &'static str)] = &[
        (VerdictKind::Verified, 0, "verified"),
        (VerdictKind::Mismatch, 1, "mismatch"),
        (VerdictKind::Unverifiable, 2, "unverifiable"),
    ];
    const UNLISTED: &'static str = "unlisted VerdictKind value";
    const EXPECTED: &'static str = "expected one of \"verified\", \"mismatch\", \"unverifiable\"";
}

impl Enumeration for FindingKind {
    const TABLE: &'static [(Self, u64, &'static str)] = &[
        (FindingKind::Changed, 0, "changed"),
        (FindingKind::Removed, 1, "removed"),
        (FindingKind::Added, 2, "added"),
        (FindingKind::Indeterminate, 3, "indeterminate"),
        (FindingKind::DerivedChanged, 4, "derived_changed"),
        (FindingKind::DerivedIndeterminate, 5, "derived_indeterminate"),
    ];
    const UNLISTED: &'static str = "unlisted FindingKind value";
    const EXPECTED: &'static str = "expected one of \"changed\", \"removed\", \"added\", \
         \"indeterminate\", \"derived_changed\", \"derived_indeterminate\"";
}

// ---------------------------------------------------------------------------
// Invariants (docs/wp/WP-015.md, in that order)

/// One side of a `Where`.
#[derive(Clone, Copy, Debug)]
pub(super) enum Side {
    Pre,
    Post,
}

/// A field of a `Site` an invariant can name.
#[derive(Clone, Copy, Debug)]
pub(super) enum SiteField {
    Line,
    Node,
    What,
}

/// Where an invariant is violated: a field or element of the section.
#[derive(Clone, Copy, Debug)]
pub(super) enum Loc {
    Section,
    Reason,
    Used(usize),
    Total,
    Findings,
    Finding(usize),
    FindingKey(usize),
    FindingPre(usize),
    FindingPost(usize),
    FindingName(usize),
    Site(usize, Side, SiteField),
}

impl Loc {
    pub(super) fn path(self) -> String {
        const S: &str = "sections.verdict";
        match self {
            Loc::Section => S.to_owned(),
            Loc::Reason => format!("{S}.reason"),
            Loc::Used(j) => format!("{S}.used[{j}]"),
            Loc::Total => format!("{S}.total"),
            Loc::Findings => format!("{S}.findings"),
            Loc::Finding(i) => format!("{S}.findings[{i}]"),
            Loc::FindingKey(i) => format!("{S}.findings[{i}].key"),
            Loc::FindingPre(i) => format!("{S}.findings[{i}].pre"),
            Loc::FindingPost(i) => format!("{S}.findings[{i}].post"),
            Loc::FindingName(i) => format!("{S}.findings[{i}].name"),
            Loc::Site(i, side, field) => {
                let side = match side {
                    Side::Pre => "pre",
                    Side::Post => "post",
                };
                let field = match field {
                    SiteField::Line => "line",
                    SiteField::Node => "node",
                    SiteField::What => "what",
                };
                format!("{S}.findings[{i}].where.{side}.{field}")
            }
        }
    }
}

/// The shard's facts that rule 1 depends on.
#[derive(Clone, Copy, Debug)]
pub(super) struct Context {
    pub phase: Phase,
    pub status: Status,
    pub digests: bool,
}

/// The order of findings: by kind, then file, then key (absent first).
fn order(f: &Finding) -> (FindingKind, &[u8], Option<crate::Digest>) {
    (f.kind, &f.file, f.key)
}

fn sites(w: &Where) -> [(Side, &Site); 2] {
    [(Side::Pre, &w.pre), (Side::Post, &w.post)]
}

const LONG: &str = "longer than 4096 bytes";

/// The first violation of the section's invariants. Each rule is checked
/// over the whole section, in field and element order, before the next.
pub(super) fn violation(v: &Verdict, cx: Context) -> Option<(Loc, &'static str)> {
    // 1.
    if cx.phase != Phase::Verify {
        return Some((Loc::Section, "a verdict section in a shard whose phase is not verify"));
    }
    if cx.status != Status::Ok {
        return Some((Loc::Section, "a verdict section in a shard whose status is not ok"));
    }
    if !cx.digests {
        return Some((Loc::Section, "a verdict section in a shard without a digests section"));
    }
    // 2.
    if v.reason.len() > MESSAGE_MAX {
        return Some((Loc::Reason, LONG));
    }
    for (i, f) in v.findings.iter().enumerate() {
        if f.name.len() > MESSAGE_MAX {
            return Some((Loc::FindingName(i), LONG));
        }
        for (side, site) in f.location.as_ref().map(sites).into_iter().flatten() {
            if site.node.len() > MESSAGE_MAX {
                return Some((Loc::Site(i, side, SiteField::Node), LONG));
            }
            if site.what.len() > MESSAGE_MAX {
                return Some((Loc::Site(i, side, SiteField::What), LONG));
            }
        }
    }
    // 3.
    match v.kind {
        VerdictKind::Verified => {
            if !v.reason.is_empty() {
                return Some((Loc::Reason, "a verified verdict has a reason"));
            }
            if v.total != 0 {
                return Some((Loc::Total, "a verified verdict has a total other than 0"));
            }
            if !v.findings.is_empty() {
                return Some((Loc::Findings, "a verified verdict has findings"));
            }
        }
        VerdictKind::Mismatch => {
            if !v.reason.is_empty() {
                return Some((Loc::Reason, "a mismatch verdict has a reason"));
            }
            if v.total == 0 {
                return Some((Loc::Total, "a mismatch verdict has a total of 0"));
            }
        }
        VerdictKind::Unverifiable => {
            if v.reason.is_empty() {
                return Some((Loc::Reason, "an unverifiable verdict has no reason"));
            }
            if v.total != 0 {
                return Some((Loc::Total, "an unverifiable verdict has a total other than 0"));
            }
            if !v.findings.is_empty() {
                return Some((Loc::Findings, "an unverifiable verdict has findings"));
            }
        }
    }
    // 4.
    if v.findings.len() as u64 > v.total {
        // `total` is below `findings.len()`, so it fits a usize.
        return Some((Loc::Finding(v.total as usize), "more findings than total"));
    }
    let mut seen = BTreeSet::new();
    for (i, f) in v.findings.iter().enumerate() {
        if i > 0 && order(&v.findings[i - 1]) > order(f) {
            return Some((Loc::Finding(i), "findings are not ascending by (kind, file, key)"));
        }
        if let Some(key) = f.key
            && !seen.insert((f.kind, key))
        {
            return Some((Loc::Finding(i), "a second finding of the same kind and key"));
        }
    }
    // 5.
    for (i, f) in v.findings.iter().enumerate() {
        if f.key.is_none() && f.kind != FindingKind::Indeterminate {
            return Some((Loc::FindingKey(i), "only an indeterminate finding may have no key"));
        }
        if f.kind == FindingKind::Added && f.pre.is_some() {
            return Some((Loc::FindingPre(i), "an added finding has a pre digest"));
        }
        if f.kind == FindingKind::Removed && f.post.is_some() {
            return Some((Loc::FindingPost(i), "a removed finding has a post digest"));
        }
    }
    // 6.
    if let Some(j) = (1..v.used.len()).find(|&j| v.used[j - 1] >= v.used[j]) {
        return Some((Loc::Used(j), "used is not strictly ascending"));
    }
    // 7.
    for (i, f) in v.findings.iter().enumerate() {
        for (side, site) in f.location.as_ref().map(sites).into_iter().flatten() {
            if (site.line == 0) != (site.column == 0) {
                return Some((
                    Loc::Site(i, side, SiteField::Line),
                    "line is 0 and column is not, or column is 0 and line is not",
                ));
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Binary

fn enc_opt_digest(e: &mut Encoder, v: Option<&crate::Digest>) {
    match v {
        None => e.u8(0),
        Some(d) => e.u8(1).digest(d),
    };
}

fn enc_site(e: &mut Encoder, s: &Site) {
    e.bytes(&s.file).uv(s.line).uv(s.column).str(&s.node).str(&s.what);
}

pub(super) fn encode(e: &mut Encoder, v: &Verdict) {
    enc_enum(e, v.kind);
    e.digest(&v.pre)
        .digest(&v.rules)
        .str(&v.reason)
        .uv(v.only_pre)
        .uv(v.only_post)
        .count(v.used.len());
    for k in &v.used {
        e.digest(k);
    }
    e.uv(v.total).count(v.findings.len());
    for f in &v.findings {
        enc_enum(e, f.kind);
        enc_opt_digest(e, f.key.as_ref());
        e.bytes(&f.file);
        enc_opt_digest(e, f.pre.as_ref());
        enc_opt_digest(e, f.post.as_ref());
        e.str(&f.name);
        match &f.location {
            None => {
                e.u8(0);
            }
            Some(w) => {
                e.u8(1);
                enc_site(e, &w.pre);
                enc_site(e, &w.post);
            }
        }
    }
}

/// File offsets of the fields an invariant can name.
#[derive(Debug, Default)]
pub(super) struct Offsets {
    section: usize,
    reason: usize,
    used: Vec<usize>,
    total: usize,
    findings: usize,
    finding: Vec<FindingOffsets>,
}

#[derive(Debug, Default)]
struct FindingOffsets {
    start: usize,
    key: usize,
    pre: usize,
    post: usize,
    name: usize,
    /// `[pre, post]`; zero when the finding has no `where`.
    sites: [SiteOffsets; 2],
}

#[derive(Clone, Copy, Debug, Default)]
struct SiteOffsets {
    line: usize,
    node: usize,
    what: usize,
}

impl Offsets {
    pub(super) fn of(&self, l: Loc) -> usize {
        let f = |i: usize| &self.finding[i];
        match l {
            Loc::Section => self.section,
            Loc::Reason => self.reason,
            Loc::Used(j) => self.used[j],
            Loc::Total => self.total,
            Loc::Findings => self.findings,
            Loc::Finding(i) => f(i).start,
            Loc::FindingKey(i) => f(i).key,
            Loc::FindingPre(i) => f(i).pre,
            Loc::FindingPost(i) => f(i).post,
            Loc::FindingName(i) => f(i).name,
            Loc::Site(i, side, field) => {
                let s = &f(i).sites[side as usize];
                match field {
                    SiteField::Line => s.line,
                    SiteField::Node => s.node,
                    SiteField::What => s.what,
                }
            }
        }
    }
}

fn dec_site(d: &mut Decoder<'_>, o: &mut SiteOffsets) -> Result<Site> {
    let file = d.bytes()?.to_vec();
    o.line = d.offset();
    let line = d.uv()?;
    let column = d.uv()?;
    o.node = d.offset();
    let node = d.str()?.to_owned();
    o.what = d.offset();
    let what = d.str()?.to_owned();
    Ok(Site { file, line, column, node, what })
}

/// The section's payload, without checking the invariants (`violation`).
pub(super) fn decode(d: &mut Decoder<'_>) -> Result<(Verdict, Offsets)> {
    let mut o = Offsets { section: d.offset(), ..Offsets::default() };
    let kind = dec_enum(d)?;
    let pre = d.digest()?;
    let rules = d.digest()?;
    o.reason = d.offset();
    let reason = d.str()?.to_owned();
    let only_pre = d.uv()?;
    let only_post = d.uv()?;
    let used = dec_list(d, Some(&mut o.used), Decoder::digest)?;
    o.total = d.offset();
    let total = d.uv()?;
    o.findings = d.offset();
    let findings = dec_list(d, None, |d| {
        let mut fo = FindingOffsets { start: d.offset(), ..FindingOffsets::default() };
        let kind = dec_enum(d)?;
        fo.key = d.offset();
        let key = dec_opt(d, Decoder::digest)?;
        let file = d.bytes()?.to_vec();
        fo.pre = d.offset();
        let pre = dec_opt(d, Decoder::digest)?;
        fo.post = d.offset();
        let post = dec_opt(d, Decoder::digest)?;
        fo.name = d.offset();
        let name = d.str()?.to_owned();
        let [so_pre, so_post] = &mut fo.sites;
        let location =
            dec_opt(d, |d| Ok(Where { pre: dec_site(d, so_pre)?, post: dec_site(d, so_post)? }))?;
        o.finding.push(fo);
        Ok(Finding { kind, key, file, pre, post, name, location })
    })?;
    let v = Verdict { kind, pre, rules, reason, only_pre, only_post, used, total, findings };
    Ok((v, o))
}

// ---------------------------------------------------------------------------
// JSON

fn json_site(s: &Site) -> Value {
    object(vec![
        ("file", json_bytes(&s.file)),
        ("line", json_uv(s.line)),
        ("column", json_uv(s.column)),
        ("node", Value::Str(s.node.clone())),
        ("what", Value::Str(s.what.clone())),
    ])
}

pub(super) fn render(v: &Verdict) -> Value {
    let findings = v
        .findings
        .iter()
        .map(|f| {
            object(vec![
                ("kind", json_enum(f.kind)),
                ("key", json_opt(f.key.as_ref(), json_digest)),
                ("file", json_bytes(&f.file)),
                ("pre", json_opt(f.pre.as_ref(), json_digest)),
                ("post", json_opt(f.post.as_ref(), json_digest)),
                ("name", Value::Str(f.name.clone())),
                (
                    "where",
                    json_opt(f.location.as_ref(), |w| {
                        object(vec![("pre", json_site(&w.pre)), ("post", json_site(&w.post))])
                    }),
                ),
            ])
        })
        .collect();
    object(vec![
        ("kind", json_enum(v.kind)),
        ("pre", json_digest(&v.pre)),
        ("rules", json_digest(&v.rules)),
        ("reason", Value::Str(v.reason.clone())),
        ("only_pre", json_uv(v.only_pre)),
        ("only_post", json_uv(v.only_post)),
        ("used", Value::Array(v.used.iter().map(json_digest).collect())),
        ("total", json_uv(v.total)),
        ("findings", Value::Array(findings)),
    ])
}

fn read_site(v: &Value, at: &At<'_>) -> JResult<Site> {
    let [file, line, column, node, what] =
        read_record(v, at, ["file", "line", "column", "node", "what"])?;
    Ok(Site {
        file: read_bytes(file, &at.key("file"))?,
        line: read_uv(line, &at.key("line"))?,
        column: read_uv(column, &at.key("column"))?,
        node: read_str(node, &at.key("node"))?.to_owned(),
        what: read_str(what, &at.key("what"))?.to_owned(),
    })
}

const FIELDS: [&str; 9] =
    ["kind", "pre", "rules", "reason", "only_pre", "only_post", "used", "total", "findings"];
const FINDING_FIELDS: [&str; 7] = ["kind", "key", "file", "pre", "post", "name", "where"];

/// The section's JSON, without checking the invariants (`violation`).
pub(super) fn read(v: &Value, at: &At<'_>) -> JResult<Verdict> {
    let [kind, pre, rules, reason, only_pre, only_post, used, total, findings] =
        read_record(v, at, FIELDS)?;
    Ok(Verdict {
        kind: read_enum(kind, &at.key("kind"))?,
        pre: read_digest(pre, &at.key("pre"))?,
        rules: read_digest(rules, &at.key("rules"))?,
        reason: read_str(reason, &at.key("reason"))?.to_owned(),
        only_pre: read_uv(only_pre, &at.key("only_pre"))?,
        only_post: read_uv(only_post, &at.key("only_post"))?,
        used: read_list(used, &at.key("used"), read_digest)?,
        total: read_uv(total, &at.key("total"))?,
        findings: read_list(findings, &at.key("findings"), |v, at| {
            let [kind, key, file, pre, post, name, location] = read_record(v, at, FINDING_FIELDS)?;
            Ok(Finding {
                kind: read_enum(kind, &at.key("kind"))?,
                key: read_opt(key, &at.key("key"), read_digest)?,
                file: read_bytes(file, &at.key("file"))?,
                pre: read_opt(pre, &at.key("pre"), read_digest)?,
                post: read_opt(post, &at.key("post"), read_digest)?,
                name: read_str(name, &at.key("name"))?.to_owned(),
                location: read_opt(location, &at.key("where"), |v, at| {
                    let [pre, post] = read_record(v, at, ["pre", "post"])?;
                    Ok(Where {
                        pre: read_site(pre, &at.key("pre"))?,
                        post: read_site(post, &at.key("post"))?,
                    })
                })?,
            })
        })?,
    })
}
