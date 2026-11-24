//! The `edits` section, id 17 of a map shard (`spec/edits.md` §4). Work
//! package WP-060; the invariants are `spec/edits.md` §4.7, the order of
//! checks and the messages `spec/landed/edits/WP-060.md` §4, the JSON form
//! its §2.

use std::collections::BTreeSet;

use crate::edits::{
    Changed, Claim, Edits, Failure, FailureKind, HostRefusal, Link, MacroSite, Piece, Place,
    Reason, Report, Role, SCHEME, Severity, SourcePlace, Stance, Subject, subject_flags, tiers,
};
use crate::json::Value;
use crate::keys::{DUPLICATE_RULE, Rule};
use crate::wire::{Decoder, Encoder};
use crate::{FileRef, Phase, Status};

use super::At;
use super::common::{
    Enumeration, JResult, Result, dec_enum, dec_file_ref, dec_list, dec_opt, enc_enum,
    enc_file_ref, json_bytes, json_digest, json_enum, json_file_ref, json_opt, json_uv, malformed,
    object, read_bool, read_bytes, read_digest, read_enum, read_file_ref, read_list, read_opt,
    read_record, read_str, read_uv,
};
use super::declared::{json_rule, read_rule};

/// The section id (`spec/model.md` §8).
pub(super) const ID: u64 = 17;

impl Enumeration for Role {
    const TABLE: &'static [(Self, u64, &'static str)] = &[
        (Role::Seed, 0, "seed"),
        (Role::Candidate, 1, "candidate"),
        (Role::Follower, 2, "follower"),
    ];
    const UNLISTED: &'static str = "unlisted Role value";
    const EXPECTED: &'static str = "expected one of \"seed\", \"candidate\", \"follower\"";
}

impl Enumeration for Stance {
    const TABLE: &'static [(Self, u64, &'static str)] = &[
        (Stance::Replace, 0, "replace"),
        (Stance::Keep, 1, "keep"),
        (Stance::Refuse, 2, "refuse"),
        (Stance::Unresolved, 3, "unresolved"),
    ];
    const UNLISTED: &'static str = "unlisted Stance value";
    const EXPECTED: &'static str =
        "expected one of \"replace\", \"keep\", \"refuse\", \"unresolved\"";
}

impl Enumeration for Reason {
    const TABLE: &'static [(Self, u64, &'static str)] = &[
        (Reason::None, 0, "none"),
        (Reason::MacroBody, 1, "macro_body"),
        (Reason::Pasted, 2, "pasted"),
        (Reason::Crossing, 3, "crossing"),
        (Reason::NotAFile, 4, "not_a_file"),
        (Reason::NotOwned, 5, "not_owned"),
        (Reason::FileRecoded, 6, "file_recoded"),
        (Reason::NotPlain, 7, "not_plain"),
        (Reason::MixedSet, 8, "mixed_set"),
        (Reason::MixedInstances, 9, "mixed_instances"),
        (Reason::Directive, 10, "directive"),
    ];
    const UNLISTED: &'static str = "unlisted Reason value";
    const EXPECTED: &'static str = "expected one of \"none\", \"macro_body\", \"pasted\", \
         \"crossing\", \"not_a_file\", \"not_owned\", \"file_recoded\", \"not_plain\", \
         \"mixed_set\", \"mixed_instances\", \"directive\"";
}

impl Enumeration for Severity {
    const TABLE: &'static [(Self, u64, &'static str)] = &[
        (Severity::Note, 0, "note"),
        (Severity::Review, 1, "review"),
        (Severity::Error, 2, "error"),
    ];
    const UNLISTED: &'static str = "unlisted Severity value";
    const EXPECTED: &'static str = "expected one of \"note\", \"review\", \"error\"";
}

impl Enumeration for FailureKind {
    const TABLE: &'static [(Self, u64, &'static str)] = &[
        (FailureKind::Uncaught, 0, "uncaught"),
        (FailureKind::NotConstant, 1, "not_constant"),
        (FailureKind::StepBudget, 2, "step_budget"),
        (FailureKind::MemoryBudget, 3, "memory_budget"),
        (FailureKind::DepthLimit, 4, "depth_limit"),
        (FailureKind::HostFailure, 5, "host_failure"),
    ];
    const UNLISTED: &'static str = "unlisted FailureKind value";
    const EXPECTED: &'static str = "expected one of \"uncaught\", \"not_constant\", \
         \"step_budget\", \"memory_budget\", \"depth_limit\", \"host_failure\"";
}

// The names of the output (spec/edits.md §10.5), as in JSON. Added by
// WP-075 for the `refusal`, `report` and `failed run` lines.

impl Reason {
    /// The name of §2.3 (`macro_body`, …).
    pub fn name(self) -> &'static str {
        Enumeration::name(self)
    }
}

impl Severity {
    /// The name of §4.5 (`note`, `review`, `error`).
    pub fn name(self) -> &'static str {
        Enumeration::name(self)
    }
}

impl FailureKind {
    /// The name of §4.6 (`uncaught`, …).
    pub fn name(self) -> &'static str {
        Enumeration::name(self)
    }
}

// ---------------------------------------------------------------------------
// Invariants (spec/edits.md §4.7, in the table's order)

/// The eight lists after `failure`, in field order.
#[derive(Clone, Copy, Debug)]
pub(super) enum List {
    Files = 0,
    Subjects = 1,
    Links = 2,
    Claims = 3,
    Groups = 4,
    Reports = 5,
    Changed = 6,
    Renamed = 7,
}

const LIST_NAMES: [&str; 8] =
    ["files", "subjects", "links", "claims", "groups", "reports", "changed", "renamed"];

/// Where an invariant is violated: a field or element of the section.
#[derive(Clone, Copy, Debug)]
pub(super) enum Loc {
    Section,
    Scheme,
    List(List),
    File(usize),
    Subject(usize),
    SubjectRule(usize),
    SubjectOf(usize),
    SubjectFlags(usize),
    Link(usize),
    Claim(usize),
    Pieces(usize),
    Piece(usize, usize),
    Reason(usize),
    Subjects(usize),
    Notes(usize),
    Tiers(usize),
    Votes(usize),
    Group(usize),
    Report(usize),
    Changed(usize),
    Renamed(usize),
    RefusalSubject,
}

impl Loc {
    pub(super) fn path(self) -> String {
        const S: &str = "sections.edits";
        match self {
            Loc::Section => S.to_owned(),
            Loc::RefusalSubject => format!("{S}.failure.refusal.subject"),
            Loc::Scheme => format!("{S}.scheme"),
            Loc::List(l) => format!("{S}.{}", LIST_NAMES[l as usize]),
            Loc::File(i) => format!("{S}.files[{i}]"),
            Loc::Subject(i) => format!("{S}.subjects[{i}]"),
            Loc::SubjectRule(i) => format!("{S}.subjects[{i}].rule"),
            Loc::SubjectOf(i) => format!("{S}.subjects[{i}].of"),
            Loc::SubjectFlags(i) => format!("{S}.subjects[{i}].flags"),
            Loc::Link(i) => format!("{S}.links[{i}]"),
            Loc::Claim(i) => format!("{S}.claims[{i}]"),
            Loc::Pieces(i) => format!("{S}.claims[{i}].pieces"),
            Loc::Piece(i, j) => format!("{S}.claims[{i}].pieces[{j}]"),
            Loc::Reason(i) => format!("{S}.claims[{i}].reason"),
            Loc::Subjects(i) => format!("{S}.claims[{i}].subjects"),
            Loc::Notes(i) => format!("{S}.claims[{i}].notes"),
            Loc::Tiers(i) => format!("{S}.claims[{i}].tiers"),
            Loc::Votes(i) => format!("{S}.claims[{i}].votes"),
            Loc::Group(i) => format!("{S}.groups[{i}]"),
            Loc::Report(i) => format!("{S}.reports[{i}]"),
            Loc::Changed(i) => format!("{S}.changed[{i}]"),
            Loc::Renamed(i) => format!("{S}.renamed[{i}]"),
        }
    }
}

/// The shard's facts the section's presence depends on (`spec/model.md` §8).
#[derive(Clone, Copy, Debug)]
pub(super) struct Context {
    pub phase: Phase,
    pub status: Status,
}

pub(super) fn ascending<T: Ord>(list: &[T]) -> Option<usize> {
    (1..list.len()).find(|&i| list[i - 1] >= list[i])
}

/// `place` is a range of one of `files`: `lo <= hi <=` its size.
pub(super) fn within(files: &[FileRef], place: &Place) -> bool {
    usize::try_from(place.file)
        .ok()
        .and_then(|i| files.get(i))
        .is_some_and(|f| place.lo <= place.hi && place.hi <= f.size)
}

pub(super) fn rule_names(r: &Rule) -> std::result::Result<(), &'static str> {
    r.name.validate()?;
    r.to_name.validate()
}

/// The first violation of the section's invariants, in the order of
/// `spec/edits.md` §4.7: each rule over its whole list before the next,
/// except the claim rules, which are checked for each claim in turn.
#[allow(clippy::too_many_lines, reason = "one block per row of spec/edits.md §4.7")]
pub(super) fn violation(e: &Edits, cx: Context) -> Option<(Loc, &'static str)> {
    // What a reader rejects while decoding, for a value built in memory.
    for (i, s) in e.subjects.iter().enumerate() {
        if let Some(Err(what)) = s.rule.as_ref().map(rule_names) {
            return Some((Loc::SubjectRule(i), what));
        }
    }
    for (i, r) in e.renamed.iter().enumerate() {
        if let Err(what) = rule_names(r) {
            return Some((Loc::Renamed(i), what));
        }
    }
    // spec/model.md §8.
    if cx.phase != Phase::Map {
        return Some((Loc::Section, "an edits section in a shard whose phase is not map"));
    }
    if cx.status != Status::Ok {
        return Some((Loc::Section, "an edits section in a shard whose status is not ok"));
    }
    if e.scheme != SCHEME {
        return Some((Loc::Scheme, "unknown edits scheme"));
    }
    if e.failure.is_some() {
        if e.files.len() > 1 {
            return Some((Loc::List(List::Files), "a failed run names more than one file"));
        }
        let lens = [
            (List::Subjects, e.subjects.len()),
            (List::Links, e.links.len()),
            (List::Claims, e.claims.len()),
            (List::Groups, e.groups.len()),
            (List::Reports, e.reports.len()),
            (List::Changed, e.changed.len()),
            (List::Renamed, e.renamed.len()),
        ];
        if let Some(&(l, _)) = lens.iter().find(|(_, n)| *n != 0) {
            return Some((Loc::List(l), "a failed run has a list that is not empty"));
        }
        let subject = e.failure.as_ref().and_then(|f| f.refusal.as_ref()?.subject.as_ref());
        match subject {
            Some(p) if !within(&e.files, p) => {
                return Some((Loc::RefusalSubject, "the refusal's subject is not within its file"));
            }
            None if !e.files.is_empty() => {
                return Some((
                    Loc::List(List::Files),
                    "a failed run names a file but its refusal has no subject",
                ));
            }
            _ => {}
        }
    }
    if let Some(i) = (1..e.files.len()).find(|&i| e.files[i - 1].path >= e.files[i].path) {
        return Some((Loc::File(i), "files are not strictly ascending by path"));
    }
    if let Some((field, i, what)) = subject_violation(&e.subjects) {
        let l = match field {
            SubjectField::Record => Loc::Subject(i),
            SubjectField::Of => Loc::SubjectOf(i),
            SubjectField::Flags => Loc::SubjectFlags(i),
        };
        return Some((l, what));
    }
    for (i, l) in e.links.iter().enumerate() {
        if i > 0 && e.links[i - 1] >= *l {
            return Some((Loc::Link(i), "links are not strictly ascending"));
        }
        if l.a >= l.b {
            return Some((Loc::Link(i), "a link's a is not below its b"));
        }
    }
    for (i, c) in e.claims.iter().enumerate() {
        if let Some(v) = claim_violation(e, i, c) {
            return Some(v);
        }
    }
    let mut grouped = BTreeSet::new();
    for (i, g) in e.groups.iter().enumerate() {
        let what = if g.len() < 2 {
            Some("a group has fewer than two members")
        } else if ascending(g).is_some() {
            Some("a group's members are not strictly ascending")
        } else if g.iter().any(|&m| {
            usize::try_from(m)
                .ok()
                .and_then(|m| e.claims.get(m))
                .is_none_or(|c| c.stance != Stance::Replace)
        }) {
            Some("a group member is not the index of a replace claim")
        } else if g.iter().any(|m| grouped.contains(m)) {
            Some("a claim is in two groups")
        } else if i > 0 && e.groups[i - 1] >= *g {
            Some("groups are not strictly ascending")
        } else {
            None
        };
        if let Some(what) = what {
            return Some((Loc::Group(i), what));
        }
        grouped.extend(g.iter().copied());
    }
    for (i, r) in e.reports.iter().enumerate() {
        if i > 0 && e.reports[i - 1] >= *r {
            return Some((Loc::Report(i), "reports are not strictly ascending"));
        }
        if r.place.as_ref().is_some_and(|p| !within(&e.files, p)) {
            return Some((Loc::Report(i), "a report's place is not within its file"));
        }
    }
    for (i, c) in e.changed.iter().enumerate() {
        if i > 0 && e.changed[i - 1] >= *c {
            return Some((Loc::Changed(i), "changed is not strictly ascending"));
        }
        if !within(&e.files, &c.place) || c.place.lo == c.place.hi {
            return Some((
                Loc::Changed(i),
                "a changed entry's place is empty or not within its file",
            ));
        }
    }
    let mut last: Option<Vec<u8>> = None;
    for (i, r) in e.renamed.iter().enumerate() {
        let m = r.match_bytes();
        if let Some(prev) = &last {
            if *prev == m {
                return Some((Loc::Renamed(i), DUPLICATE_RULE));
            }
            if *prev > m {
                return Some((Loc::Renamed(i), "renamed is not strictly ascending"));
            }
        }
        last = Some(m);
    }
    None
}

/// The field of a subject an invariant names.
#[derive(Clone, Copy, Debug)]
pub(super) enum SubjectField {
    Record,
    Of,
    Flags,
}

/// The subject rules of §4.7, each over the whole list: ascending by id,
/// then `of` for every subject, then the flags for every subject.
pub(super) fn subject_violation(
    subjects: &[Subject],
) -> Option<(SubjectField, usize, &'static str)> {
    if let Some(i) = (1..subjects.len()).find(|&i| subjects[i - 1].id >= subjects[i].id) {
        return Some((SubjectField::Record, i, "subjects are not strictly ascending by id"));
    }
    for (i, s) in subjects.iter().enumerate() {
        if (s.role == Role::Follower) == s.of.is_empty() {
            return Some((
                SubjectField::Of,
                i,
                "of is empty for a follower or not empty for another role",
            ));
        }
        if ascending(&s.of).is_some() {
            return Some((SubjectField::Of, i, "of is not strictly ascending"));
        }
    }
    if let Some(i) = subjects.iter().position(|s| s.flags & !subject_flags::ALL != 0) {
        return Some((SubjectField::Flags, i, "subject flags have a bit above 3 set"));
    }
    None
}

/// The piece rules of §4.7 over `pieces`, in order: the first piece that
/// breaks one, and the rule.
pub(super) fn piece_violation(
    files: &[FileRef],
    pieces: &[Piece],
) -> Option<(usize, &'static str)> {
    for (j, p) in pieces.iter().enumerate() {
        let what = match p {
            Piece::Literal(b) if b.is_empty() => Some("a literal piece is empty"),
            Piece::Literal(_) if j > 0 && matches!(pieces[j - 1], Piece::Literal(_)) => {
                Some("two literal pieces are adjacent")
            }
            Piece::Spelled { text, .. } if text.is_empty() => {
                Some("a spelled piece's text is empty")
            }
            Piece::Identifier { text, .. } if !crate::transform::is_plain(text) => {
                Some("an identifier piece's text is not one plain identifier")
            }
            Piece::Copy(place) if !within(files, place) || place.lo == place.hi => {
                Some("a copy piece's place is empty or not within its file")
            }
            _ => None,
        };
        if let Some(what) = what {
            return Some((j, what));
        }
    }
    None
}

/// The tier rules of §4.7 for a claim of stance `stance` with or without a
/// macro, in the table's order.
pub(super) fn tier_violation(t: u64, stance: Stance, has_macro: bool) -> Option<&'static str> {
    if t == 0 {
        Some("tiers is 0")
    } else if t & !tiers::ALL != 0 {
        Some("tiers has a bit above 4 set")
    } else if t & !(tiers::T0 | tiers::T1) != 0 && stance != Stance::Refuse {
        Some("tiers has a bit of tier 2, 3 or X for a claim that is not a refuse")
    } else if t == tiers::T0 && has_macro {
        Some("a claim of tier 0 alone has a macro")
    } else {
        None
    }
}

/// The claim rules of §4.7 for claim `i`, in the table's order.
fn claim_violation(e: &Edits, i: usize, c: &Claim) -> Option<(Loc, &'static str)> {
    let replace = c.stance == Stance::Replace;
    if !within(&e.files, &Place { file: c.file, lo: c.lo, hi: c.hi }) {
        return Some((Loc::Claim(i), "a claim's range is not within its file"));
    }
    if replace && c.lo == c.hi {
        return Some((Loc::Claim(i), "a replace claim's range is empty"));
    }
    if !replace && !c.pieces.is_empty() {
        return Some((Loc::Pieces(i), "a claim that is not a replace has pieces"));
    }
    if let Some((j, what)) = piece_violation(&e.files, &c.pieces) {
        return Some((Loc::Piece(i, j), what));
    }
    if (c.reason == Reason::None) == (c.stance == Stance::Refuse) {
        return Some((Loc::Reason(i), "reason is none for a refuse claim or not none for another"));
    }
    if ascending(&c.subjects).is_some() {
        return Some((Loc::Subjects(i), "a claim's subjects are not strictly ascending"));
    }
    if ascending(&c.notes).is_some() {
        return Some((Loc::Notes(i), "a claim's notes are not strictly ascending"));
    }
    if let Some(what) = tier_violation(c.tiers, c.stance, c.macro_site.is_some()) {
        return Some((Loc::Tiers(i), what));
    }
    if c.votes == 0 {
        return Some((Loc::Votes(i), "votes is 0"));
    }
    if i > 0 && e.claims[i - 1].cmp_key(c).is_ge() {
        return Some((Loc::Claim(i), "claims are not strictly ascending in the sort key"));
    }
    None
}

// ---------------------------------------------------------------------------
// Binary

pub(super) fn enc_place(e: &mut Encoder, p: &Place) {
    e.uv(p.file).uv(p.lo).uv(p.hi);
}

pub(super) fn enc_opt<T>(e: &mut Encoder, v: Option<&T>, value: impl FnOnce(&mut Encoder, &T)) {
    match v {
        None => {
            e.u8(0);
        }
        Some(v) => {
            e.u8(1);
            value(e, v);
        }
    }
}

pub(super) fn enc_failure(e: &mut Encoder, f: &Failure) {
    enc_enum(e, f.kind);
    e.str(&f.message);
    enc_opt(e, f.place.as_ref(), |e, p| {
        e.bytes(&p.file).uv(p.line).uv(p.column);
    });
    e.str(&f.rendered);
    enc_opt(e, f.refusal.as_ref(), |e, r| {
        e.str(&r.function).str(&r.errc).str(&r.message);
        enc_opt(e, r.subject.as_ref(), enc_place);
    });
}

pub(super) fn enc_subject(e: &mut Encoder, s: &Subject) {
    e.digest(&s.id);
    enc_opt(e, s.rule.as_ref(), |e, r| r.encode(e));
    enc_enum(e, s.role);
    e.count(s.of.len());
    for k in &s.of {
        e.digest(k);
    }
    e.uv(s.flags).str(&s.display);
}

pub(super) fn enc_piece(e: &mut Encoder, p: &Piece) {
    e.uv(p.tag());
    match p {
        Piece::Literal(b) => {
            e.bytes(b);
        }
        Piece::Copy(place) => enc_place(e, place),
        Piece::Spelled { key, text } => {
            e.digest(key).bytes(text);
        }
        Piece::Identifier { text, key } => {
            e.bytes(text);
            enc_opt(e, key.as_ref(), |e, k| {
                e.digest(k);
            });
        }
    }
}

pub(super) fn enc_macro(e: &mut Encoder, m: Option<&MacroSite>) {
    enc_opt(e, m, |e, m| {
        e.uv(m.name_start).bytes(&m.name);
    });
}

pub(super) fn enc_report(e: &mut Encoder, r: &Report) {
    enc_opt(e, r.place.as_ref(), enc_place);
    enc_enum(e, r.severity);
    e.bytes(&r.message).bool(r.attached);
}

pub(super) fn encode(e: &mut Encoder, x: &Edits) {
    e.uv(x.scheme).digest(&x.transform);
    enc_opt(e, x.failure.as_ref(), enc_failure);
    e.count(x.files.len());
    for f in &x.files {
        enc_file_ref(e, f);
    }
    e.count(x.subjects.len());
    for s in &x.subjects {
        enc_subject(e, s);
    }
    e.count(x.links.len());
    for l in &x.links {
        e.digest(&l.a).digest(&l.b);
    }
    e.count(x.claims.len());
    for c in &x.claims {
        e.uv(c.file).uv(c.lo).uv(c.hi);
        enc_enum(e, c.stance);
        e.count(c.pieces.len());
        for p in &c.pieces {
            enc_piece(e, p);
        }
        e.count(c.subjects.len());
        for k in &c.subjects {
            e.digest(k);
        }
        enc_enum(e, c.reason);
        e.uv(c.tiers);
        enc_macro(e, c.macro_site.as_ref());
        e.uv(c.votes).count(c.notes.len());
        for n in &c.notes {
            e.bytes(n);
        }
    }
    e.count(x.groups.len());
    for g in &x.groups {
        e.count(g.len());
        for &m in g {
            e.uv(m);
        }
    }
    e.count(x.reports.len());
    for r in &x.reports {
        enc_report(e, r);
    }
    e.count(x.changed.len());
    for c in &x.changed {
        e.digest(&c.key);
        enc_place(e, &c.place);
        e.bytes(&c.reason);
    }
    e.count(x.renamed.len());
    for r in &x.renamed {
        r.encode(e);
    }
}

/// File offsets of the fields an invariant can name.
#[derive(Debug, Default)]
pub(super) struct Offsets {
    section: usize,
    lists: [usize; 8],
    file: Vec<usize>,
    subject: Vec<[usize; 4]>,
    link: Vec<usize>,
    claim: Vec<ClaimOffsets>,
    group: Vec<usize>,
    report: Vec<usize>,
    changed: Vec<usize>,
    renamed: Vec<usize>,
    refusal_subject: usize,
}

#[derive(Debug, Default)]
struct ClaimOffsets {
    start: usize,
    pieces: usize,
    piece: Vec<usize>,
    reason: usize,
    subjects: usize,
    notes: usize,
    tiers: usize,
    votes: usize,
}

impl Offsets {
    pub(super) fn of(&self, l: Loc) -> usize {
        let c = |i: usize| &self.claim[i];
        match l {
            Loc::Section | Loc::Scheme => self.section,
            Loc::List(l) => self.lists[l as usize],
            Loc::File(i) => self.file[i],
            Loc::Subject(i) => self.subject[i][0],
            Loc::SubjectRule(i) => self.subject[i][1],
            Loc::SubjectOf(i) => self.subject[i][2],
            Loc::SubjectFlags(i) => self.subject[i][3],
            Loc::Link(i) => self.link[i],
            Loc::Claim(i) => c(i).start,
            Loc::Pieces(i) => c(i).pieces,
            Loc::Piece(i, j) => c(i).piece[j],
            Loc::Reason(i) => c(i).reason,
            Loc::Subjects(i) => c(i).subjects,
            Loc::Notes(i) => c(i).notes,
            Loc::Tiers(i) => c(i).tiers,
            Loc::Votes(i) => c(i).votes,
            Loc::Group(i) => self.group[i],
            Loc::Report(i) => self.report[i],
            Loc::Changed(i) => self.changed[i],
            Loc::Renamed(i) => self.renamed[i],
            Loc::RefusalSubject => self.refusal_subject,
        }
    }
}

pub(super) fn dec_place(d: &mut Decoder<'_>) -> Result<Place> {
    Ok(Place { file: d.uv()?, lo: d.uv()?, hi: d.uv()? })
}

pub(super) fn dec_piece(d: &mut Decoder<'_>) -> Result<Piece> {
    let at = d.offset();
    Ok(match d.uv()? {
        0 => Piece::Literal(d.bytes()?.to_vec()),
        1 => Piece::Copy(dec_place(d)?),
        2 => Piece::Spelled { key: d.digest()?, text: d.bytes()?.to_vec() },
        3 => Piece::Identifier { text: d.bytes()?.to_vec(), key: dec_opt(d, Decoder::digest)? },
        _ => return Err(malformed(at, "unlisted Piece value")),
    })
}

/// A `Failure` (§4.6) and the offset of its refusal's `subject` (0 if it
/// has no refusal).
pub(super) fn dec_failure(d: &mut Decoder<'_>) -> Result<(Failure, usize)> {
    let mut subject_at = 0;
    let f = Failure {
        kind: dec_enum(d)?,
        message: d.str()?.to_owned(),
        place: dec_opt(d, |d| {
            Ok(SourcePlace { file: d.bytes()?.to_vec(), line: d.uv()?, column: d.uv()? })
        })?,
        rendered: d.str()?.to_owned(),
        refusal: dec_opt(d, |d| {
            Ok(HostRefusal {
                function: d.str()?.to_owned(),
                errc: d.str()?.to_owned(),
                message: d.str()?.to_owned(),
                subject: {
                    subject_at = d.offset();
                    dec_opt(d, dec_place)?
                },
            })
        })?,
    };
    Ok((f, subject_at))
}

/// A `Subject` (§4.1) and the offsets of its first byte, `rule`, `of` and
/// `flags`.
pub(super) fn dec_subject(d: &mut Decoder<'_>) -> Result<(Subject, [usize; 4])> {
    let start = d.offset();
    let id = d.digest()?;
    let rule_at = d.offset();
    let subject_rule = dec_opt(d, Rule::decode)?;
    let role = dec_enum(d)?;
    let of_at = d.offset();
    let of = dec_list(d, None, Decoder::digest)?;
    let flags_at = d.offset();
    let flags = d.uv()?;
    let s = Subject { id, rule: subject_rule, role, of, flags, display: d.str()?.to_owned() };
    Ok((s, [start, rule_at, of_at, flags_at]))
}

pub(super) fn dec_macro(d: &mut Decoder<'_>) -> Result<Option<MacroSite>> {
    dec_opt(d, |d| Ok(MacroSite { name_start: d.uv()?, name: d.bytes()?.to_vec() }))
}

pub(super) fn dec_report(d: &mut Decoder<'_>) -> Result<Report> {
    Ok(Report {
        place: dec_opt(d, dec_place)?,
        severity: dec_enum(d)?,
        message: d.bytes()?.to_vec(),
        attached: d.bool()?,
    })
}

/// The section's payload, without checking the invariants (`violation`).
#[allow(clippy::too_many_lines, reason = "one statement per field of spec/edits.md §4")]
pub(super) fn decode(d: &mut Decoder<'_>) -> Result<(Edits, Offsets)> {
    let mut o = Offsets { section: d.offset(), ..Offsets::default() };
    let scheme = d.uv()?;
    let transform = d.digest()?;
    let mut subject_at = 0;
    let failure = dec_opt(d, |d| {
        let (f, at) = dec_failure(d)?;
        subject_at = at;
        Ok(f)
    })?;
    o.refusal_subject = subject_at;
    o.lists[List::Files as usize] = d.offset();
    let files = dec_list(d, Some(&mut o.file), dec_file_ref)?;
    o.lists[List::Subjects as usize] = d.offset();
    let subjects = dec_list(d, None, |d| {
        let (s, at) = dec_subject(d)?;
        o.subject.push(at);
        Ok(s)
    })?;
    o.lists[List::Links as usize] = d.offset();
    let links = dec_list(d, Some(&mut o.link), |d| Ok(Link { a: d.digest()?, b: d.digest()? }))?;
    o.lists[List::Claims as usize] = d.offset();
    let claims = dec_list(d, None, |d| {
        let mut co = ClaimOffsets { start: d.offset(), ..ClaimOffsets::default() };
        let file = d.uv()?;
        let lo = d.uv()?;
        let hi = d.uv()?;
        let stance = dec_enum(d)?;
        co.pieces = d.offset();
        let pieces = dec_list(d, Some(&mut co.piece), dec_piece)?;
        co.subjects = d.offset();
        let subjects = dec_list(d, None, Decoder::digest)?;
        co.reason = d.offset();
        let reason = dec_enum(d)?;
        co.tiers = d.offset();
        let tiers = d.uv()?;
        let macro_site = dec_macro(d)?;
        co.votes = d.offset();
        let votes = d.uv()?;
        co.notes = d.offset();
        let notes = dec_list(d, None, |d| Ok(d.bytes()?.to_vec()))?;
        o.claim.push(co);
        Ok(Claim {
            file,
            lo,
            hi,
            stance,
            pieces,
            subjects,
            reason,
            tiers,
            macro_site,
            votes,
            notes,
        })
    })?;
    o.lists[List::Groups as usize] = d.offset();
    let groups = dec_list(d, Some(&mut o.group), |d| dec_list(d, None, Decoder::uv))?;
    o.lists[List::Reports as usize] = d.offset();
    let reports = dec_list(d, Some(&mut o.report), dec_report)?;
    o.lists[List::Changed as usize] = d.offset();
    let changed = dec_list(d, Some(&mut o.changed), |d| {
        Ok(Changed { key: d.digest()?, place: dec_place(d)?, reason: d.bytes()?.to_vec() })
    })?;
    o.lists[List::Renamed as usize] = d.offset();
    let renamed = dec_list(d, Some(&mut o.renamed), Rule::decode)?;
    let edits = Edits {
        scheme,
        transform,
        failure,
        files,
        subjects,
        links,
        claims,
        groups,
        reports,
        changed,
        renamed,
    };
    Ok((edits, o))
}

// ---------------------------------------------------------------------------
// JSON

const FIELDS: [&str; 11] = [
    "scheme",
    "transform",
    "failure",
    "files",
    "subjects",
    "links",
    "claims",
    "groups",
    "reports",
    "changed",
    "renamed",
];
const FAILURE: [&str; 5] = ["kind", "message", "place", "rendered", "refusal"];
const SOURCE_PLACE: [&str; 3] = ["file", "line", "column"];
const REFUSAL: [&str; 4] = ["function", "errc", "message", "subject"];
const PLACE: [&str; 3] = ["file", "lo", "hi"];
const SUBJECT: [&str; 6] = ["id", "rule", "role", "of", "flags", "display"];
const LINK: [&str; 2] = ["a", "b"];
const CLAIM: [&str; 11] = [
    "file", "lo", "hi", "stance", "pieces", "subjects", "reason", "tiers", "macro", "votes",
    "notes",
];
const MACRO: [&str; 2] = ["name_start", "name"];
const SPELLED: [&str; 2] = ["key", "text"];
const REPORT: [&str; 4] = ["place", "severity", "message", "attached"];
const CHANGED: [&str; 3] = ["key", "place", "reason"];

pub(super) fn record<const N: usize>(fields: [&str; N], values: [Value; N]) -> Value {
    object(fields.into_iter().zip(values).collect())
}

pub(super) fn json_place(p: &Place) -> Value {
    record(PLACE, [json_uv(p.file), json_uv(p.lo), json_uv(p.hi)])
}

pub(super) fn json_digests(list: &[crate::Digest]) -> Value {
    Value::Array(list.iter().map(json_digest).collect())
}

pub(super) fn json_piece(p: &Piece) -> Value {
    match p {
        Piece::Literal(b) => object(vec![("literal", json_bytes(b))]),
        Piece::Copy(place) => object(vec![("copy", json_place(place))]),
        Piece::Spelled { key, text } => {
            object(vec![("spelled", record(SPELLED, [json_digest(key), json_bytes(text)]))])
        }
        Piece::Identifier { text, key: None } => object(vec![("identifier", json_bytes(text))]),
        Piece::Identifier { text, key: Some(key) } => {
            object(vec![("identifier", json_bytes(text)), ("key", json_digest(key))])
        }
    }
}

pub(super) fn json_failure(f: &Failure) -> Value {
    record(
        FAILURE,
        [
            json_enum(f.kind),
            Value::Str(f.message.clone()),
            json_opt(f.place.as_ref(), |p| {
                record(SOURCE_PLACE, [json_bytes(&p.file), json_uv(p.line), json_uv(p.column)])
            }),
            Value::Str(f.rendered.clone()),
            json_opt(f.refusal.as_ref(), |r| {
                record(
                    REFUSAL,
                    [
                        Value::Str(r.function.clone()),
                        Value::Str(r.errc.clone()),
                        Value::Str(r.message.clone()),
                        json_opt(r.subject.as_ref(), json_place),
                    ],
                )
            }),
        ],
    )
}

pub(super) fn json_subject(s: &Subject) -> Value {
    record(
        SUBJECT,
        [
            json_digest(&s.id),
            json_opt(s.rule.as_ref(), json_rule),
            json_enum(s.role),
            json_digests(&s.of),
            json_uv(s.flags),
            Value::Str(s.display.clone()),
        ],
    )
}

pub(super) fn json_macro(m: Option<&MacroSite>) -> Value {
    json_opt(m, |m| record(MACRO, [json_uv(m.name_start), json_bytes(&m.name)]))
}

pub(super) fn json_report(r: &Report) -> Value {
    record(
        REPORT,
        [
            json_opt(r.place.as_ref(), json_place),
            json_enum(r.severity),
            json_bytes(&r.message),
            Value::Bool(r.attached),
        ],
    )
}

pub(super) fn render(x: &Edits) -> Value {
    let failure = json_opt(x.failure.as_ref(), json_failure);
    let subjects = x.subjects.iter().map(json_subject).collect();
    let links =
        x.links.iter().map(|l| record(LINK, [json_digest(&l.a), json_digest(&l.b)])).collect();
    let claims = x
        .claims
        .iter()
        .map(|c| {
            record(
                CLAIM,
                [
                    json_uv(c.file),
                    json_uv(c.lo),
                    json_uv(c.hi),
                    json_enum(c.stance),
                    Value::Array(c.pieces.iter().map(json_piece).collect()),
                    json_digests(&c.subjects),
                    json_enum(c.reason),
                    json_uv(c.tiers),
                    json_macro(c.macro_site.as_ref()),
                    json_uv(c.votes),
                    Value::Array(c.notes.iter().map(|n| json_bytes(n)).collect()),
                ],
            )
        })
        .collect();
    let groups =
        x.groups.iter().map(|g| Value::Array(g.iter().map(|&m| json_uv(m)).collect())).collect();
    let reports = x.reports.iter().map(json_report).collect();
    let changed = x
        .changed
        .iter()
        .map(|c| {
            record(CHANGED, [json_digest(&c.key), json_place(&c.place), json_bytes(&c.reason)])
        })
        .collect();
    record(
        FIELDS,
        [
            json_uv(x.scheme),
            json_digest(&x.transform),
            failure,
            Value::Array(x.files.iter().map(json_file_ref).collect()),
            Value::Array(subjects),
            Value::Array(links),
            Value::Array(claims),
            Value::Array(groups),
            Value::Array(reports),
            Value::Array(changed),
            Value::Array(x.renamed.iter().map(json_rule).collect()),
        ],
    )
}

pub(super) fn read_place(v: &Value, at: &At<'_>) -> JResult<Place> {
    let [file, lo, hi] = read_record(v, at, PLACE)?;
    Ok(Place {
        file: read_uv(file, &at.key("file"))?,
        lo: read_uv(lo, &at.key("lo"))?,
        hi: read_uv(hi, &at.key("hi"))?,
    })
}

pub(super) fn read_string(v: &Value, at: &At<'_>) -> JResult<String> {
    Ok(read_str(v, at)?.to_owned())
}

const PIECE_EXPECTED: &str = "a Piece is an object with one member, literal, copy, spelled or \
     identifier, or with the two members identifier and key";

pub(super) fn read_piece(v: &Value, at: &At<'_>) -> JResult<Piece> {
    if let Some([(form, text), (k, key)]) = v.as_object()
        && form == "identifier"
        && k == "key"
    {
        return Ok(Piece::Identifier {
            text: read_bytes(text, &at.key(form))?,
            key: Some(read_digest(key, &at.key(k))?),
        });
    }
    let Some([(form, value)]) = v.as_object() else {
        return Err(at.error(PIECE_EXPECTED));
    };
    let here = at.key(form);
    Ok(match form.as_str() {
        "literal" => Piece::Literal(read_bytes(value, &here)?),
        "copy" => Piece::Copy(read_place(value, &here)?),
        "spelled" => {
            let [key, text] = read_record(value, &here, SPELLED)?;
            Piece::Spelled {
                key: read_digest(key, &here.key("key"))?,
                text: read_bytes(text, &here.key("text"))?,
            }
        }
        "identifier" => Piece::Identifier { text: read_bytes(value, &here)?, key: None },
        _ => return Err(here.error(PIECE_EXPECTED)),
    })
}

pub(super) fn read_failure(v: &Value, at: &At<'_>) -> JResult<Failure> {
    let [kind, message, place, rendered, refusal] = read_record(v, at, FAILURE)?;
    Ok(Failure {
        kind: read_enum(kind, &at.key("kind"))?,
        message: read_string(message, &at.key("message"))?,
        place: read_opt(place, &at.key("place"), |v, at| {
            let [file, line, column] = read_record(v, at, SOURCE_PLACE)?;
            Ok(SourcePlace {
                file: read_bytes(file, &at.key("file"))?,
                line: read_uv(line, &at.key("line"))?,
                column: read_uv(column, &at.key("column"))?,
            })
        })?,
        rendered: read_string(rendered, &at.key("rendered"))?,
        refusal: read_opt(refusal, &at.key("refusal"), |v, at| {
            let [function, errc, message, subject] = read_record(v, at, REFUSAL)?;
            Ok(HostRefusal {
                function: read_string(function, &at.key("function"))?,
                errc: read_string(errc, &at.key("errc"))?,
                message: read_string(message, &at.key("message"))?,
                subject: read_opt(subject, &at.key("subject"), read_place)?,
            })
        })?,
    })
}

pub(super) fn read_subject(v: &Value, at: &At<'_>) -> JResult<Subject> {
    let [id, subject_rule, role, of, flags, display] = read_record(v, at, SUBJECT)?;
    Ok(Subject {
        id: read_digest(id, &at.key("id"))?,
        rule: read_opt(subject_rule, &at.key("rule"), read_rule)?,
        role: read_enum(role, &at.key("role"))?,
        of: read_list(of, &at.key("of"), read_digest)?,
        flags: read_uv(flags, &at.key("flags"))?,
        display: read_string(display, &at.key("display"))?,
    })
}

pub(super) fn read_macro(v: &Value, at: &At<'_>) -> JResult<Option<MacroSite>> {
    read_opt(v, at, |v, at| {
        let [name_start, name] = read_record(v, at, MACRO)?;
        Ok(MacroSite {
            name_start: read_uv(name_start, &at.key("name_start"))?,
            name: read_bytes(name, &at.key("name"))?,
        })
    })
}

pub(super) fn read_report(v: &Value, at: &At<'_>) -> JResult<Report> {
    let [place, severity, message, attached] = read_record(v, at, REPORT)?;
    Ok(Report {
        place: read_opt(place, &at.key("place"), read_place)?,
        severity: read_enum(severity, &at.key("severity"))?,
        message: read_bytes(message, &at.key("message"))?,
        attached: read_bool(attached, &at.key("attached"))?,
    })
}

/// The section's JSON, without checking the invariants (`violation`).
pub(super) fn read(v: &Value, at: &At<'_>) -> JResult<Edits> {
    let [
        scheme,
        transform,
        failure,
        files,
        subjects,
        links,
        claims,
        groups,
        reports,
        changed,
        renamed,
    ] = read_record(v, at, FIELDS)?;
    let scheme = read_uv(scheme, &at.key("scheme"))?;
    let transform = read_digest(transform, &at.key("transform"))?;
    let failure = read_opt(failure, &at.key("failure"), read_failure)?;
    Ok(Edits {
        scheme,
        transform,
        failure,
        files: read_list(files, &at.key("files"), read_file_ref)?,
        subjects: read_list(subjects, &at.key("subjects"), read_subject)?,
        links: read_list(links, &at.key("links"), |v, at| {
            let [a, b] = read_record(v, at, LINK)?;
            Ok(Link { a: read_digest(a, &at.key("a"))?, b: read_digest(b, &at.key("b"))? })
        })?,
        claims: read_list(claims, &at.key("claims"), |v, at| {
            let [file, lo, hi, stance, pieces, subjects, reason, tiers, macro_site, votes, notes] =
                read_record(v, at, CLAIM)?;
            Ok(Claim {
                file: read_uv(file, &at.key("file"))?,
                lo: read_uv(lo, &at.key("lo"))?,
                hi: read_uv(hi, &at.key("hi"))?,
                stance: read_enum(stance, &at.key("stance"))?,
                pieces: read_list(pieces, &at.key("pieces"), read_piece)?,
                subjects: read_list(subjects, &at.key("subjects"), read_digest)?,
                reason: read_enum(reason, &at.key("reason"))?,
                tiers: read_uv(tiers, &at.key("tiers"))?,
                macro_site: read_macro(macro_site, &at.key("macro"))?,
                votes: read_uv(votes, &at.key("votes"))?,
                notes: read_list(notes, &at.key("notes"), read_bytes)?,
            })
        })?,
        groups: read_list(groups, &at.key("groups"), |v, at| read_list(v, at, read_uv))?,
        reports: read_list(reports, &at.key("reports"), read_report)?,
        changed: read_list(changed, &at.key("changed"), |v, at| {
            let [key, place, reason] = read_record(v, at, CHANGED)?;
            Ok(Changed {
                key: read_digest(key, &at.key("key"))?,
                place: read_place(place, &at.key("place"))?,
                reason: read_bytes(reason, &at.key("reason"))?,
            })
        })?,
        renamed: read_list(renamed, &at.key("renamed"), read_rule)?,
    })
}
