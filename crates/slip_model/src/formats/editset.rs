//! The edit set, kind `EDTS` (`spec/edits.md` §6). Work package WP-061;
//! the invariants are §6.1, the order of checks, the messages and the JSON
//! form `spec/landed/edits/WP-061.md` §2 and §3.

use std::cmp::Ordering;
use std::collections::BTreeSet;

use crate::edits::{FailureKind, Piece, Place, Reason, Stance};
use crate::editset::{
    Cause, Edit, EditSet, Exempt, FIRST_MAX, Failed, Refusal, Reported, SCHEME, Seen, SetState,
    cross,
};
use crate::json::Value;
use crate::keys::{DUPLICATE_RULE, RuleTable, SAME_PRODUCED_RULE, SortError, UNSORTED_RULES};
use crate::transform::{is_canonical, is_file_path};
use crate::wire::{Decoder, Encoder, Error, ErrorKind};
use crate::{Counter, Digest};

use super::common::{
    Enumeration, JResult, Result, dec_enum, dec_file_ref, dec_list, dec_opt, enc_enum,
    enc_file_ref, json_bytes, json_digest, json_enum, json_file_ref, json_opt, json_uv, malformed,
    read_bool, read_bytes, read_digest, read_enum, read_file_ref, read_list, read_opt, read_record,
    read_str, read_uv,
};
use super::declared::{json_rule, read_rule};
use super::edits::{
    SubjectField, ascending, dec_failure, dec_instance, dec_macro, dec_piece, dec_place,
    dec_report, dec_subject, enc_failure, enc_instance, enc_macro, enc_opt, enc_piece, enc_place,
    enc_report, enc_subject, json_digests, json_failure, json_instance, json_macro, json_piece,
    json_place, json_report, json_subject, piece_violation, read_failure, read_instance,
    read_macro, read_piece, read_place, read_report, read_subject, record, rule_names,
    subject_violation, tier_violation, within,
};
use super::shard::is_counter_name;
use super::{
    At, Extra, JsonFormError, OwnedExtras, SectionName, open, read_container, render_container,
    seal,
};

/// The section id (`spec/edits.md` §6).
const SET: u64 = 1;

impl Enumeration for SetState {
    const TABLE: &'static [(Self, u64, &'static str)] =
        &[(SetState::Accepted, 0, "accepted"), (SetState::Refused, 1, "refused")];
    const UNLISTED: &'static str = "unlisted SetState value";
    const EXPECTED: &'static str = "expected one of \"accepted\", \"refused\"";
}

impl Enumeration for Cause {
    const TABLE: &'static [(Self, u64, &'static str)] = &[
        (Cause::NoObserver, 0, "no_observer"),
        (Cause::NoSubject, 1, "no_subject"),
        (Cause::SubjectReadOnly, 2, "subject_read_only"),
        (Cause::SubjectUnsupported, 3, "subject_unsupported"),
        (Cause::SubjectUnreachable, 4, "subject_unreachable"),
        (Cause::NameTaken, 5, "name_taken"),
        (Cause::MixedFollower, 6, "mixed_follower"),
        (Cause::MixedSet, 7, "mixed_set"),
        (Cause::SiteRefused, 8, "site_refused"),
        (Cause::Contested, 9, "contested"),
        (Cause::Divergent, 10, "divergent"),
        (Cause::Overlap, 11, "overlap"),
        (Cause::FileDiffers, 12, "file_differs"),
        (Cause::Cyclic, 13, "cyclic"),
        (Cause::Buried, 14, "buried"),
        (Cause::RuleConflict, 15, "rule_conflict"),
        (Cause::NoEdit, 16, "no_edit"),
        (Cause::WithGroup, 17, "with_group"),
    ];
    const UNLISTED: &'static str = "unlisted Cause value";
    const EXPECTED: &'static str = "expected a Cause of spec/edits.md §6";
}

impl SetState {
    /// Every value, in code order.
    pub const ALL: [SetState; 2] = [SetState::Accepted, SetState::Refused];

    /// The name of `spec/edits.md` §6 (`accepted`, `refused`), as in JSON.
    pub fn name(self) -> &'static str {
        Enumeration::name(self)
    }
}

impl Cause {
    /// The name of `spec/edits.md` §6 (`no_observer`, …), as in JSON and
    /// in the output (§10.5).
    pub fn name(self) -> &'static str {
        Enumeration::name(self)
    }
}

// ---------------------------------------------------------------------------
// Invariants (spec/edits.md §6.1, in the table's order)

/// Where an invariant is violated.
#[derive(Clone, Copy, Debug)]
enum Loc {
    Scheme,
    State,
    File(usize),
    Subject(usize),
    SubjectRule(usize),
    SubjectOf(usize),
    SubjectFlags(usize),
    Rule(usize),
    Edit(usize),
    Piece(usize, usize),
    EditSubjects(usize),
    EditTiers(usize),
    EditNotes(usize),
    EditFirst(usize),
    Group(usize),
    Changed(usize),
    ChangedInForce(usize),
    ChangedFirst(usize),
    Kept(usize),
    Unresolved(usize),
    Refusal(usize),
    RefusalFile(usize),
    RefusalReason(usize),
    RefusalSubject(usize),
    RefusalDisplay(usize),
    RefusalMacro(usize),
    RefusalOther(usize),
    RefusalObservers(usize),
    RefusalFirst(usize),
    Report(usize),
    ReportFirst(usize),
    Failure(usize),
    FailureFirst(usize),
    FailureSubject(usize),
    CounterName(usize),
    Counter(usize),
    Instance(usize),
    InstanceSubjects(usize),
    InstanceFirst(usize),
}

impl Loc {
    fn path(self) -> String {
        const S: &str = "sections.set";
        match self {
            Loc::Scheme => format!("{S}.scheme"),
            Loc::State => format!("{S}.state"),
            Loc::File(i) => format!("{S}.files[{i}]"),
            Loc::Subject(i) => format!("{S}.subjects[{i}]"),
            Loc::SubjectRule(i) => format!("{S}.subjects[{i}].rule"),
            Loc::SubjectOf(i) => format!("{S}.subjects[{i}].of"),
            Loc::SubjectFlags(i) => format!("{S}.subjects[{i}].flags"),
            Loc::Rule(i) => format!("{S}.rules[{i}]"),
            Loc::Edit(i) => format!("{S}.edits[{i}]"),
            Loc::Piece(i, j) => format!("{S}.edits[{i}].pieces[{j}]"),
            Loc::EditSubjects(i) => format!("{S}.edits[{i}].subjects"),
            Loc::EditTiers(i) => format!("{S}.edits[{i}].tiers"),
            Loc::EditNotes(i) => format!("{S}.edits[{i}].notes"),
            Loc::EditFirst(i) => format!("{S}.edits[{i}].first"),
            Loc::Group(i) => format!("{S}.groups[{i}]"),
            Loc::Changed(i) => format!("{S}.changed[{i}]"),
            Loc::ChangedInForce(i) => format!("{S}.changed[{i}].in_force"),
            Loc::ChangedFirst(i) => format!("{S}.changed[{i}].first"),
            Loc::Kept(i) => format!("{S}.kept[{i}]"),
            Loc::Unresolved(i) => format!("{S}.unresolved[{i}]"),
            Loc::Refusal(i) => format!("{S}.refusals[{i}]"),
            Loc::RefusalFile(i) => format!("{S}.refusals[{i}].file"),
            Loc::RefusalReason(i) => format!("{S}.refusals[{i}].reason"),
            Loc::RefusalSubject(i) => format!("{S}.refusals[{i}].subject"),
            Loc::RefusalDisplay(i) => format!("{S}.refusals[{i}].display"),
            Loc::RefusalMacro(i) => format!("{S}.refusals[{i}].macro"),
            Loc::RefusalOther(i) => format!("{S}.refusals[{i}].other"),
            Loc::RefusalObservers(i) => format!("{S}.refusals[{i}].observers"),
            Loc::RefusalFirst(i) => format!("{S}.refusals[{i}].first"),
            Loc::Report(i) => format!("{S}.reports[{i}]"),
            Loc::ReportFirst(i) => format!("{S}.reports[{i}].first"),
            Loc::Failure(i) => format!("{S}.failures[{i}]"),
            Loc::FailureFirst(i) => format!("{S}.failures[{i}].first"),
            Loc::FailureSubject(i) => format!("{S}.failures[{i}].failure.refusal.subject"),
            Loc::CounterName(i) => format!("{S}.counters[{i}].name"),
            Loc::Counter(i) => format!("{S}.counters[{i}]"),
            Loc::Instance(i) => format!("{S}.instances[{i}]"),
            Loc::InstanceSubjects(i) => format!("{S}.instances[{i}].instance.subjects"),
            Loc::InstanceFirst(i) => format!("{S}.instances[{i}].first"),
        }
    }
}

type Violation = Option<(Loc, &'static str)>;

const NOT_CANONICAL: &str = "not a canonical absolute path";

/// Why `path`, which is not [`is_file_path`], cannot name a file.
pub(super) fn path_what(path: &[u8]) -> &'static str {
    if is_canonical(path) { "the root / is not a file" } else { NOT_CANONICAL }
}

fn sort_what(e: SortError) -> &'static str {
    match e {
        SortError::Invalid(what) => what,
        SortError::Duplicate => DUPLICATE_RULE,
        SortError::Unsorted => UNSORTED_RULES,
        SortError::SameProduced { .. } => SAME_PRODUCED_RULE,
    }
}

/// The first violation of §6.1, rows in order (`spec/landed/edits/WP-061.md`
/// §2): within a row, its lists in field order, each element in turn,
/// every rule of the row for that element before the next element.
fn violation(x: &EditSet) -> Violation {
    // What a reader rejects while decoding, for a value built in memory.
    for (i, s) in x.subjects.iter().enumerate() {
        if let Some(Err(what)) = s.rule.as_ref().map(rule_names) {
            return Some((Loc::SubjectRule(i), what));
        }
    }
    for (i, r) in x.rules.iter().enumerate() {
        if let Err(what) = rule_names(r) {
            return Some((Loc::Rule(i), what));
        }
    }
    if x.scheme != SCHEME {
        return Some((Loc::Scheme, "unknown edit set scheme"));
    }
    if let Some(i) = x.files.iter().position(|f| !is_file_path(&f.path)) {
        return Some((Loc::File(i), path_what(&x.files[i].path)));
    }
    if let Some(i) = (1..x.files.len()).find(|&i| x.files[i - 1].path >= x.files[i].path) {
        return Some((Loc::File(i), "files are not strictly ascending by path"));
    }
    if let Some((field, i, what)) = subject_violation(&x.subjects) {
        let l = match field {
            SubjectField::Record => Loc::Subject(i),
            SubjectField::Of => Loc::SubjectOf(i),
            SubjectField::Flags => Loc::SubjectFlags(i),
        };
        return Some((l, what));
    }
    if let Err((i, e)) = RuleTable::from_sorted(x.rules.clone()) {
        return Some((Loc::Rule(i), sort_what(e)));
    }
    if x.state == SetState::Refused {
        if !x.edits.is_empty() {
            return Some((Loc::Edit(0), "a refused edit set has edits"));
        }
        if !x.groups.is_empty() {
            return Some((Loc::Group(0), "a refused edit set has groups"));
        }
    }
    geometry_violation(&x.edits)
        .map(|(i, what)| (Loc::Edit(i), what))
        .or_else(|| edit_violation(x))
        .or_else(|| group_violation(x))
        .or_else(|| place_violation(x))
        .or_else(|| named_violation(x))
        .or_else(|| in_force_violation(x))
        .or_else(|| first_violation(x))
        .or_else(|| order_violation(x))
        .or_else(|| {
            x.refusals
                .iter()
                .position(|r| (r.reason == Reason::None) == (r.cause == Cause::SiteRefused))
                .map(|i| {
                    (
                        Loc::RefusalReason(i),
                        "reason is none for site_refused or not none for another cause",
                    )
                })
        })
        .or_else(|| refusal_violation(x))
        .or_else(|| counter_violation(&x.counters))
        .or_else(|| state_violation(x))
        .or_else(|| instance_violation(x))
}

/// Row 19 (WP-139), for each instance in turn: strictly ascending by the
/// instance; its place the range of an edit or of a refusal of a site;
/// its `subjects` strictly ascending; `first` as row 14 says.
fn instance_violation(x: &EditSet) -> Violation {
    let sites: BTreeSet<(u64, u64, u64)> =
        x.refusals.iter().filter_map(|r| Some((r.file?, r.lo, r.hi))).collect();
    let at_edit = |p: &Place| {
        x.edits.binary_search_by(|e| (e.file, e.lo, e.hi).cmp(&(p.file, p.lo, p.hi))).is_ok()
    };
    for (i, s) in x.instances.iter().enumerate() {
        let p = &s.instance.place;
        if i > 0 && x.instances[i - 1].instance >= s.instance {
            return Some((Loc::Instance(i), "instances are not strictly ascending"));
        }
        if !at_edit(p) && !sites.contains(&(p.file, p.lo, p.hi)) {
            return Some((
                Loc::Instance(i),
                "an instance's place is the range of no edit and of no refusal of a site",
            ));
        }
        if ascending(&s.instance.subjects).is_some() {
            return Some((
                Loc::InstanceSubjects(i),
                "an instance's subjects are not strictly ascending",
            ));
        }
        if let Some(what) = first_what(&s.first, s.observers) {
            return Some((Loc::InstanceFirst(i), what));
        }
    }
    None
}

/// Row 7 for every edit in turn: strictly ascending by `(file,
/// lo, hi)`; `lo < hi`; crosses no earlier edit of its file; crosses no
/// range an edit copies. `O((E + C) log C)` for `E` edits and `C` copies.
fn geometry_violation(edits: &[Edit]) -> Option<(usize, &'static str)> {
    let copies = Copies::new(edits);
    // The edits of the current file with a `lo` below the current one that
    // contain the current `lo`, outermost first (a chain: the edits before
    // the first violation are disjoint or nested), and those with the
    // current `lo`, in list order.
    let mut open: Vec<(u64, u64)> = Vec::new();
    let mut group: Vec<(u64, u64)> = Vec::new();
    let mut at: Option<(u64, u64)> = None;
    for (i, e) in edits.iter().enumerate() {
        let r = (e.lo, e.hi);
        if i > 0 {
            let p = &edits[i - 1];
            if (p.file, p.lo, p.hi) >= (e.file, e.lo, e.hi) {
                return Some((i, "edits are not strictly ascending by (file, lo, hi)"));
            }
        }
        if e.lo >= e.hi {
            return Some((i, "an edit's range is empty"));
        }
        match at {
            Some((file, lo)) if file == e.file && lo == e.lo => {}
            Some((file, _)) if file == e.file => {
                // Edits of one `lo` are nested: the last is outermost.
                open.extend(group.drain(..).rev());
                while open.last().is_some_and(|t| t.1 <= e.lo) {
                    open.pop();
                }
            }
            _ => {
                open.clear();
                group.clear();
            }
        }
        at = Some((e.file, e.lo));
        if open.last().is_some_and(|&t| cross(t, r)) {
            return Some((i, "an edit crosses another edit"));
        }
        group.push(r);
        if copies.crossed_by(e.file, r) {
            return Some((i, "an edit crosses a range an edit copies"));
        }
    }
    None
}

/// The non-empty ranges the edits copy, for the question "does this range
/// cross one of them".
struct Copies {
    /// `(file, lo, hi)` by `(file, lo)`, and a tree of the greatest `hi`.
    by_lo: Vec<(u64, u64, u64)>,
    max_hi: Tree,
    /// `(file, hi, lo)` by `(file, hi)`, and a tree of the least `lo`.
    by_hi: Vec<(u64, u64, u64)>,
    min_lo: Tree,
}

impl Copies {
    fn new(edits: &[Edit]) -> Copies {
        let mut by_lo: Vec<(u64, u64, u64)> = edits
            .iter()
            .flat_map(|e| e.pieces.iter())
            .filter_map(|p| match p {
                Piece::Copy(c) if c.lo < c.hi => Some((c.file, c.lo, c.hi)),
                _ => None,
            })
            .collect();
        let mut by_hi: Vec<(u64, u64, u64)> =
            by_lo.iter().map(|&(f, lo, hi)| (f, hi, lo)).collect();
        by_lo.sort_unstable();
        by_hi.sort_unstable();
        let max_hi = Tree::new(by_lo.iter().map(|c| c.2), 0, u64::max);
        let min_lo = Tree::new(by_hi.iter().map(|c| c.2), u64::MAX, u64::min);
        Copies { by_lo, max_hi, by_hi, min_lo }
    }

    /// Some copied range of `file` crosses `r`, a non-empty range: one
    /// starts inside `r` and ends after it, or ends inside it and starts
    /// before it.
    fn crossed_by(&self, file: u64, r: (u64, u64)) -> bool {
        let inside = |list: &[(u64, u64, u64)]| {
            let a = list.partition_point(|c| (c.0, c.1) <= (file, r.0));
            let b = list.partition_point(|c| (c.0, c.1) < (file, r.1));
            (a, b.max(a))
        };
        let (a, b) = inside(&self.by_lo);
        let starts_inside = self.max_hi.query(a, b) > r.1;
        let (a, b) = inside(&self.by_hi);
        let ends_inside = self.min_lo.query(a, b) < r.0;
        starts_inside || ends_inside
    }
}

/// A segment tree over a fixed list, answering `fold` over an index range.
struct Tree {
    n: usize,
    nodes: Vec<u64>,
    identity: u64,
    fold: fn(u64, u64) -> u64,
}

impl Tree {
    fn new(
        values: impl ExactSizeIterator<Item = u64>,
        identity: u64,
        fold: fn(u64, u64) -> u64,
    ) -> Tree {
        let n = values.len();
        let mut nodes = vec![identity; 2 * n];
        for (i, v) in values.enumerate() {
            nodes[n + i] = v;
        }
        for i in (1..n).rev() {
            nodes[i] = fold(nodes[2 * i], nodes[2 * i + 1]);
        }
        Tree { n, nodes, identity, fold }
    }

    /// `fold` over the values at indices `a..b`; the identity if empty.
    fn query(&self, a: usize, b: usize) -> u64 {
        let (mut a, mut b) = (a + self.n, b + self.n);
        let mut acc = self.identity;
        while a < b {
            if a & 1 == 1 {
                acc = (self.fold)(acc, self.nodes[a]);
                a += 1;
            }
            if b & 1 == 1 {
                b -= 1;
                acc = (self.fold)(acc, self.nodes[b]);
            }
            a /= 2;
            b /= 2;
        }
        acc
    }
}

/// Row 8 (the piece rules of §4.7), then row 9 (an edit's `subjects` and
/// `notes` strictly ascending, the tier rules of a replace claim), each
/// over every edit.
fn edit_violation(x: &EditSet) -> Violation {
    for (i, e) in x.edits.iter().enumerate() {
        if let Some((j, what)) = piece_violation(&x.files, &e.pieces) {
            return Some((Loc::Piece(i, j), what));
        }
    }
    for (i, e) in x.edits.iter().enumerate() {
        if ascending(&e.subjects).is_some() {
            return Some((Loc::EditSubjects(i), "an edit's subjects are not strictly ascending"));
        }
        if ascending(&e.notes).is_some() {
            return Some((Loc::EditNotes(i), "an edit's notes are not strictly ascending"));
        }
        if let Some(what) = tier_violation(e.tiers, Stance::Replace, e.macro_site.is_some()) {
            return Some((Loc::EditTiers(i), what));
        }
    }
    None
}

/// Row 10: the group rules of §4.7, over `edits`.
fn group_violation(x: &EditSet) -> Violation {
    let mut grouped = BTreeSet::new();
    for (i, g) in x.groups.iter().enumerate() {
        let what = if g.len() < 2 {
            Some("a group has fewer than two members")
        } else if ascending(g).is_some() {
            Some("a group's members are not strictly ascending")
        } else if g.iter().any(|&m| usize::try_from(m).ok().is_none_or(|m| m >= x.edits.len())) {
            Some("a group member is not the index of an edit")
        } else if g.iter().any(|m| grouped.contains(m)) {
            Some("an edit is in two groups")
        } else if i > 0 && x.groups[i - 1] >= *g {
            Some("groups are not strictly ascending")
        } else {
            None
        };
        if let Some(what) = what {
            return Some((Loc::Group(i), what));
        }
        grouped.extend(g.iter().copied());
    }
    None
}

/// Row 11: every place within its file's size, in field order.
fn place_violation(x: &EditSet) -> Violation {
    let f = &x.files;
    for (i, e) in x.edits.iter().enumerate() {
        if !within(f, &Place { file: e.file, lo: e.lo, hi: e.hi }) {
            return Some((Loc::Edit(i), "an edit's range is not within its file"));
        }
    }
    for (i, c) in x.changed.iter().enumerate() {
        if !within(f, &c.place) || c.place.lo == c.place.hi {
            return Some((
                Loc::Changed(i),
                "a changed entry's place is empty or not within its file",
            ));
        }
    }
    if let Some(i) = x.kept.iter().position(|p| !within(f, p)) {
        return Some((Loc::Kept(i), "a kept range is not within its file"));
    }
    if let Some(i) = x.unresolved.iter().position(|p| !within(f, p)) {
        return Some((Loc::Unresolved(i), "an unresolved range is not within its file"));
    }
    for (i, r) in x.refusals.iter().enumerate() {
        let what = match r.file {
            None if r.lo != 0 || r.hi != 0 => Some("a refusal that is not of a site has a range"),
            Some(file) if !within(f, &Place { file, lo: r.lo, hi: r.hi }) => {
                Some("a refusal's range is not within its file")
            }
            _ if r.other.as_ref().is_some_and(|p| !within(f, p)) => {
                Some("a refusal's other range is not within its file")
            }
            _ => None,
        };
        if let Some(what) = what {
            return Some((Loc::Refusal(i), what));
        }
    }
    if let Some(i) =
        x.reports.iter().position(|r| r.report.place.as_ref().is_some_and(|p| !within(f, p)))
    {
        return Some((Loc::Report(i), "a report's place is not within its file"));
    }
    let outside = |x: &Failed| {
        x.failure.refusal.as_ref().and_then(|r| r.subject.as_ref()).is_some_and(|p| !within(f, p))
    };
    if let Some(i) = x.failures.iter().position(outside) {
        return Some((Loc::FailureSubject(i), "the refusal's subject is not within its file"));
    }
    None
}

/// Row 12: every file is named by a record: an edit, a copy piece, a
/// `changed`, `kept` or `unresolved` place, a refusal's `file` or `other`,
/// a report's place, a failure's `refusal.subject`.
fn named_violation(x: &EditSet) -> Violation {
    let mut named = vec![false; x.files.len()];
    let mut name = |f: u64| {
        if let Some(n) = usize::try_from(f).ok().and_then(|i| named.get_mut(i)) {
            *n = true;
        }
    };
    for e in &x.edits {
        name(e.file);
        for p in &e.pieces {
            if let Piece::Copy(c) = p {
                name(c.file);
            }
        }
    }
    for c in &x.changed {
        name(c.place.file);
    }
    for p in x.kept.iter().chain(&x.unresolved) {
        name(p.file);
    }
    for r in &x.refusals {
        if let Some(f) = r.file {
            name(f);
        }
        if let Some(p) = &r.other {
            name(p.file);
        }
    }
    for p in x.reports.iter().filter_map(|r| r.report.place.as_ref()) {
        name(p.file);
    }
    for f in &x.failures {
        if let Some(p) = f.failure.refusal.as_ref().and_then(|r| r.subject.as_ref()) {
            name(p.file);
        }
    }
    named.iter().position(|n| !n).map(|i| (Loc::File(i), "no record names the file"))
}

/// Row 13: a `changed` entry is in force iff an edit shares a byte with
/// its place (§6.2). `O(log E)` each: the edits of a file below the
/// place's `hi`, and the greatest `hi` among them.
fn in_force_violation(x: &EditSet) -> Violation {
    let e = &x.edits;
    // The greatest `hi` of the edits of one file up to each index.
    let mut reach: Vec<u64> = Vec::with_capacity(e.len());
    for i in 0..e.len() {
        let before = if i > 0 && e[i - 1].file == e[i].file { reach[i - 1] } else { 0 };
        reach.push(before.max(e[i].hi));
    }
    // Rows 7 and 11: every edit and every `changed` place has a byte.
    let touched = |p: &Place| {
        let n = e.partition_point(|t| (t.file, t.lo) < (p.file, p.hi));
        n > 0 && e[n - 1].file == p.file && reach[n - 1] > p.lo
    };
    x.changed.iter().position(|c| c.in_force != touched(&c.place)).map(|i| {
        (Loc::ChangedInForce(i), "in_force is not whether an edit shares a byte with the place")
    })
}

fn first_what(first: &[Digest], observers: u64) -> Option<&'static str> {
    if first.len() > FIRST_MAX {
        Some("first has more than 8 entries")
    } else if first.len() as u64 > observers {
        Some("first has more entries than observers")
    } else {
        None
    }
}

/// A list's location and the `(first, observers)` of its records.
type FirstLists<'a> = (&'a dyn Fn(usize) -> Loc, Vec<(&'a [Digest], u64)>);

/// Row 14: `first` at most 8 entries and at most `observers`, an edit's
/// `notes` at most 8, in field order.
fn first_violation(x: &EditSet) -> Violation {
    for (i, e) in x.edits.iter().enumerate() {
        if e.notes.len() > FIRST_MAX {
            return Some((Loc::EditNotes(i), "notes has more than 8 entries"));
        }
        if let Some(what) = first_what(&e.first, e.observers) {
            return Some((Loc::EditFirst(i), what));
        }
    }
    let lists: [FirstLists<'_>; 4] = [
        (&Loc::ChangedFirst, x.changed.iter().map(|c| (c.first.as_slice(), c.observers)).collect()),
        (
            &Loc::RefusalFirst,
            x.refusals.iter().map(|c| (c.first.as_slice(), c.observers)).collect(),
        ),
        (&Loc::ReportFirst, x.reports.iter().map(|c| (c.first.as_slice(), c.observers)).collect()),
        (
            &Loc::FailureFirst,
            x.failures.iter().map(|c| (c.first.as_slice(), c.observers)).collect(),
        ),
    ];
    for (loc, list) in lists {
        for (i, (first, observers)) in list.into_iter().enumerate() {
            if let Some(what) = first_what(first, observers) {
                return Some((loc(i), what));
            }
        }
    }
    None
}

fn exempt_key(e: &Exempt) -> (&Digest, &Place, &[u8]) {
    (&e.key, &e.place, &e.reason)
}

/// The sort key of a failure: `(kind, message, place)`, a place as
/// `(file, line, column)`.
type FailureKey<'a> = (FailureKind, &'a [u8], Option<(&'a [u8], u64, u64)>);

/// `(kind, message, place)`, an absent place first (§6).
fn failure_cmp(a: &Failed, b: &Failed) -> Ordering {
    fn key(f: &Failed) -> FailureKey<'_> {
        let x = &f.failure;
        (
            x.kind,
            x.message.as_bytes(),
            x.place.as_ref().map(|p| (p.file.as_slice(), p.line, p.column)),
        )
    }
    key(a).cmp(&key(b))
}

/// The first `i > 0` with `list[i - 1]` not below `list[i]` by `cmp`.
fn first_unsorted<T>(list: &[T], cmp: impl Fn(&T, &T) -> Ordering) -> Option<usize> {
    (1..list.len()).find(|&i| cmp(&list[i - 1], &list[i]) != Ordering::Less)
}

/// Row 15: `changed`, `kept`, `unresolved`, `refusals`, `reports`,
/// `failures` strictly ascending in their keys.
fn order_violation(x: &EditSet) -> Violation {
    if let Some(i) = first_unsorted(&x.changed, |a, b| exempt_key(a).cmp(&exempt_key(b))) {
        return Some((
            Loc::Changed(i),
            "changed is not strictly ascending by (key, place, reason)",
        ));
    }
    if let Some(i) = ascending(&x.kept) {
        return Some((Loc::Kept(i), "kept is not strictly ascending"));
    }
    if let Some(i) = ascending(&x.unresolved) {
        return Some((Loc::Unresolved(i), "unresolved is not strictly ascending"));
    }
    if let Some(i) = first_unsorted(&x.refusals, Refusal::cmp_key) {
        return Some((
            Loc::Refusal(i),
            "refusals are not strictly ascending by (cause, file, lo, hi, subject)",
        ));
    }
    if let Some(i) = first_unsorted(&x.reports, |a, b| a.report.cmp(&b.report)) {
        return Some((Loc::Report(i), "reports are not strictly ascending"));
    }
    if let Some(i) = first_unsorted(&x.failures, failure_cmp) {
        return Some((
            Loc::Failure(i),
            "failures are not strictly ascending by (kind, message, place)",
        ));
    }
    None
}

/// Row 17: a refusal's fields against its cause, for each refusal in turn
/// (§6): `file` present iff of a site; a refusal of a subject but
/// `no_subject` has a `subject`, and `no_subject`, `no_observer`, `no_edit`
/// and `rule_conflict` have none; no `macro` but of a site; `other` only
/// for `contested`, `overlap`, `buried` and `with_group`; `observers` 0 for the four
/// without a subject (row 14 then makes `first` empty); a `no_edit`
/// refusal only in a set with no edit, and beside no refusal that needs a
/// `replace` claim (§5.5: "with `no_edit` if no observer claimed a
/// `replace`").
fn refusal_violation(x: &EditSet) -> Violation {
    let of_replace = |c: Cause| {
        matches!(
            c,
            Cause::Contested
                | Cause::Divergent
                | Cause::Overlap
                | Cause::FileDiffers
                | Cause::Cyclic
                | Cause::Buried
                | Cause::WithGroup
        )
    };
    for (i, r) in x.refusals.iter().enumerate() {
        let c = r.cause;
        let alone =
            matches!(c, Cause::NoSubject | Cause::NoObserver | Cause::NoEdit | Cause::RuleConflict);
        let found = if r.file.is_some() != c.is_of_site() {
            Some((
                Loc::RefusalFile(i),
                "file is absent for a refusal of a site or present for another cause",
            ))
        } else if c.is_of_subject() && !alone && r.subject.is_none() {
            Some((Loc::RefusalSubject(i), "a refusal of a subject has no subject"))
        } else if alone && r.subject.is_some() {
            Some((
                Loc::RefusalSubject(i),
                "a no_subject, no_observer, no_edit or rule_conflict refusal has a subject",
            ))
        } else if r.subject.is_none() && r.display.is_some() {
            Some((Loc::RefusalDisplay(i), "a refusal without a subject has a display"))
        } else if !c.is_of_site() && r.macro_site.is_some() {
            Some((Loc::RefusalMacro(i), "a refusal that is not of a site has a macro"))
        } else if r.other.is_some()
            && !matches!(c, Cause::Contested | Cause::Overlap | Cause::Buried | Cause::WithGroup)
        {
            Some((
                Loc::RefusalOther(i),
                "other is present for a cause other than contested, overlap, buried or with_group",
            ))
        } else if alone && r.observers != 0 {
            Some((
                Loc::RefusalObservers(i),
                "a no_subject, no_observer, no_edit or rule_conflict refusal has observers",
            ))
        } else if c == Cause::NoEdit && !x.edits.is_empty() {
            Some((Loc::Refusal(i), "a no_edit refusal is in a set with an edit"))
        } else if c == Cause::NoEdit && x.refusals.iter().any(|r| of_replace(r.cause)) {
            Some((
                Loc::Refusal(i),
                "a no_edit refusal is beside a contested, divergent, overlap, file_differs, cyclic, buried or with_group refusal",
            ))
        } else {
            None
        };
        if found.is_some() {
            return found;
        }
    }
    None
}

/// Row 18: the counter rules of `spec/model.md` §8.1, for each counter in turn.
fn counter_violation(counters: &[Counter]) -> Violation {
    for (i, c) in counters.iter().enumerate() {
        if !is_counter_name(&c.name) {
            return Some((Loc::CounterName(i), "counter name is not [a-z0-9_]+(.[a-z0-9_]+)*"));
        }
        if i > 0 && counters[i - 1].name >= c.name {
            return Some((Loc::Counter(i), "counters are not strictly ascending by name"));
        }
    }
    None
}

/// Row 19: an accepted set has an edit, and no refusal of a subject,
/// `no_observer` or `rule_conflict`; a refused set has a refusal (§5.5:
/// "with the refusals that explain it, or with `no_edit`").
fn state_violation(x: &EditSet) -> Violation {
    let forces = |r: &Refusal| {
        r.cause.is_of_subject() || matches!(r.cause, Cause::NoObserver | Cause::RuleConflict)
    };
    let what = match x.state {
        SetState::Accepted if x.edits.is_empty() || x.refusals.iter().any(forces) => {
            "an edit set with no edit, or with a refusal of a subject, no_observer or \
             rule_conflict, is not refused"
        }
        SetState::Refused if x.refusals.is_empty() => "a refused edit set has no refusal",
        _ => return None,
    };
    Some((Loc::State, what))
}

pub(super) fn check(x: &EditSet) -> std::result::Result<(), (String, &'static str)> {
    violation(x).map_or(Ok(()), |(l, what)| Err((l.path(), what)))
}

// ---------------------------------------------------------------------------
// Binary

fn enc_digests(e: &mut Encoder, list: &[Digest]) {
    e.count(list.len());
    for d in list {
        e.digest(d);
    }
}

fn enc_bytes_list(e: &mut Encoder, list: &[Vec<u8>]) {
    e.count(list.len());
    for b in list {
        e.bytes(b);
    }
}

#[allow(clippy::too_many_lines, reason = "one statement per field of spec/edits.md §6")]
pub(super) fn encode(x: &EditSet, extras: &[Extra<'_>]) -> Vec<u8> {
    let mut e = Encoder::new();
    e.uv(x.scheme).digest(&x.plan).uv(x.round).digest(&x.transform).digest(&x.basis);
    enc_enum(&mut e, x.state);
    e.count(x.files.len());
    for f in &x.files {
        enc_file_ref(&mut e, f);
    }
    e.count(x.subjects.len());
    for s in &x.subjects {
        enc_subject(&mut e, s);
    }
    e.count(x.rules.len());
    for r in &x.rules {
        r.encode(&mut e);
    }
    e.count(x.edits.len());
    for t in &x.edits {
        e.uv(t.file).uv(t.lo).uv(t.hi).count(t.pieces.len());
        for p in &t.pieces {
            enc_piece(&mut e, p);
        }
        enc_digests(&mut e, &t.subjects);
        e.uv(t.tiers);
        enc_macro(&mut e, t.macro_site.as_ref());
        enc_bytes_list(&mut e, &t.notes);
        e.uv(t.observers).uv(t.votes);
        enc_digests(&mut e, &t.first);
    }
    e.count(x.groups.len());
    for g in &x.groups {
        e.count(g.len());
        for &m in g {
            e.uv(m);
        }
    }
    e.count(x.changed.len());
    for c in &x.changed {
        e.digest(&c.key);
        enc_place(&mut e, &c.place);
        e.bytes(&c.reason).bool(c.in_force).uv(c.observers);
        enc_digests(&mut e, &c.first);
    }
    for list in [&x.kept, &x.unresolved] {
        e.count(list.len());
        for p in list {
            enc_place(&mut e, p);
        }
    }
    e.count(x.refusals.len());
    for r in &x.refusals {
        enc_enum(&mut e, r.cause);
        enc_opt(&mut e, r.file.as_ref(), |e, f| {
            e.uv(*f);
        });
        e.uv(r.lo).uv(r.hi);
        enc_enum(&mut e, r.reason);
        enc_opt(&mut e, r.subject.as_ref(), |e, k| {
            e.digest(k);
        });
        enc_opt(&mut e, r.display.as_ref(), |e, s| {
            e.str(s);
        });
        enc_macro(&mut e, r.macro_site.as_ref());
        enc_opt(&mut e, r.other.as_ref(), enc_place);
        e.uv(r.observers);
        enc_digests(&mut e, &r.first);
    }
    e.count(x.reports.len());
    for r in &x.reports {
        enc_report(&mut e, &r.report);
        e.uv(r.observers);
        enc_digests(&mut e, &r.first);
    }
    e.count(x.failures.len());
    for f in &x.failures {
        enc_failure(&mut e, &f.failure);
        e.uv(f.observers);
        enc_digests(&mut e, &f.first);
    }
    enc_digests(&mut e, &x.unparsed);
    e.count(x.counters.len());
    for c in &x.counters {
        e.str(&c.name).uv(c.value);
    }
    e.count(x.instances.len());
    for s in &x.instances {
        enc_instance(&mut e, &s.instance);
        e.uv(s.observers);
        enc_digests(&mut e, &s.first);
    }
    seal(EditSet::KIND, EditSet::VERSION, &[(SET, e.as_bytes())], extras)
}

/// File offsets of the fields an invariant can name.
#[derive(Debug, Default)]
struct Offsets {
    scheme: usize,
    state: usize,
    file: Vec<usize>,
    subject: Vec<[usize; 4]>,
    rule: Vec<usize>,
    edit: Vec<EditOffsets>,
    group: Vec<usize>,
    changed: Vec<[usize; 3]>,
    kept: Vec<usize>,
    unresolved: Vec<usize>,
    refusal: Vec<RefusalOffsets>,
    report: Vec<[usize; 2]>,
    failure: Vec<[usize; 3]>,
    counter: Vec<usize>,
    instance: Vec<[usize; 3]>,
}

#[derive(Debug, Default)]
struct RefusalOffsets {
    start: usize,
    file: usize,
    reason: usize,
    subject: usize,
    display: usize,
    macro_site: usize,
    other: usize,
    observers: usize,
    first: usize,
}

#[derive(Debug, Default)]
struct EditOffsets {
    start: usize,
    piece: Vec<usize>,
    subjects: usize,
    tiers: usize,
    notes: usize,
    first: usize,
}

impl Offsets {
    fn of(&self, l: Loc) -> usize {
        match l {
            Loc::Scheme => self.scheme,
            Loc::State => self.state,
            Loc::File(i) => self.file[i],
            Loc::Subject(i) => self.subject[i][0],
            Loc::SubjectRule(i) => self.subject[i][1],
            Loc::SubjectOf(i) => self.subject[i][2],
            Loc::SubjectFlags(i) => self.subject[i][3],
            Loc::Rule(i) => self.rule[i],
            Loc::Edit(i) => self.edit[i].start,
            Loc::Piece(i, j) => self.edit[i].piece[j],
            Loc::EditSubjects(i) => self.edit[i].subjects,
            Loc::EditTiers(i) => self.edit[i].tiers,
            Loc::EditNotes(i) => self.edit[i].notes,
            Loc::EditFirst(i) => self.edit[i].first,
            Loc::Group(i) => self.group[i],
            Loc::Changed(i) => self.changed[i][0],
            Loc::ChangedInForce(i) => self.changed[i][2],
            Loc::ChangedFirst(i) => self.changed[i][1],
            Loc::Kept(i) => self.kept[i],
            Loc::Unresolved(i) => self.unresolved[i],
            Loc::Refusal(i) => self.refusal[i].start,
            Loc::RefusalFile(i) => self.refusal[i].file,
            Loc::RefusalReason(i) => self.refusal[i].reason,
            Loc::RefusalSubject(i) => self.refusal[i].subject,
            Loc::RefusalDisplay(i) => self.refusal[i].display,
            Loc::RefusalMacro(i) => self.refusal[i].macro_site,
            Loc::RefusalOther(i) => self.refusal[i].other,
            Loc::RefusalObservers(i) => self.refusal[i].observers,
            Loc::RefusalFirst(i) => self.refusal[i].first,
            Loc::Report(i) => self.report[i][0],
            Loc::ReportFirst(i) => self.report[i][1],
            Loc::Failure(i) => self.failure[i][0],
            Loc::FailureFirst(i) => self.failure[i][1],
            Loc::FailureSubject(i) => self.failure[i][2],
            Loc::CounterName(i) | Loc::Counter(i) => self.counter[i],
            Loc::Instance(i) => self.instance[i][0],
            Loc::InstanceSubjects(i) => self.instance[i][1],
            Loc::InstanceFirst(i) => self.instance[i][2],
        }
    }
}

fn dec_digests(d: &mut Decoder<'_>) -> Result<Vec<Digest>> {
    dec_list(d, None, Decoder::digest)
}

/// `observers: uv, first: list<digest>`, recording the offset of `first`.
fn dec_counts(d: &mut Decoder<'_>, first_at: &mut usize) -> Result<(u64, Vec<Digest>)> {
    let observers = d.uv()?;
    *first_at = d.offset();
    Ok((observers, dec_digests(d)?))
}

/// The first version read: version 3 (`docs/wp/WP-160.md`) added
/// `Refusal.display`, and no reader of older versions is kept before a
/// release (`spec/edits.md` §6).
const SINCE: u32 = 3;

/// The message of a version this reader no longer reads.
const OLD_VERSION: &str = "container version older than supported";

/// The edit set, its unknown ignorable sections and its content id.
#[allow(clippy::too_many_lines, reason = "one statement per field of spec/edits.md §6")]
pub(super) fn decode(bytes: &[u8]) -> Result<(EditSet, Vec<Extra<'_>>, Digest)> {
    let opened = open(bytes, EditSet::KIND, EditSet::VERSION, [SET])?;
    if opened.version < SINCE {
        return Err(Error::new(ErrorKind::UnsupportedVersion, 12, OLD_VERSION));
    }
    let [section] = opened.sections;
    let mut section = super::required(section, opened.end, "edit set has no set section")?;
    let d = &mut section;
    let mut o = Offsets { scheme: d.offset(), ..Offsets::default() };
    let scheme = d.uv()?;
    let plan = d.digest()?;
    let round = d.uv()?;
    let transform = d.digest()?;
    let basis = d.digest()?;
    o.state = d.offset();
    let state = dec_enum(d)?;
    let files = dec_list(d, Some(&mut o.file), dec_file_ref)?;
    let subjects = dec_list(d, None, |d| {
        let (s, at) = dec_subject(d)?;
        o.subject.push(at);
        Ok(s)
    })?;
    let rules = dec_list(d, Some(&mut o.rule), crate::keys::Rule::decode)?;
    let edits = dec_list(d, None, |d| {
        let mut eo = EditOffsets { start: d.offset(), ..EditOffsets::default() };
        let file = d.uv()?;
        let lo = d.uv()?;
        let hi = d.uv()?;
        let pieces = dec_list(d, Some(&mut eo.piece), dec_piece)?;
        eo.subjects = d.offset();
        let subjects = dec_digests(d)?;
        eo.tiers = d.offset();
        let tiers = d.uv()?;
        let macro_site = dec_macro(d)?;
        eo.notes = d.offset();
        let notes = dec_list(d, None, |d| Ok(d.bytes()?.to_vec()))?;
        let observers = d.uv()?;
        let votes = d.uv()?;
        eo.first = d.offset();
        let first = dec_digests(d)?;
        o.edit.push(eo);
        Ok(Edit {
            file,
            lo,
            hi,
            pieces,
            subjects,
            tiers,
            macro_site,
            notes,
            observers,
            votes,
            first,
        })
    })?;
    let groups = dec_list(d, Some(&mut o.group), |d| dec_list(d, None, Decoder::uv))?;
    let changed = dec_list(d, None, |d| {
        let start = d.offset();
        let key = d.digest()?;
        let place = dec_place(d)?;
        let reason = d.bytes()?.to_vec();
        let in_force_at = d.offset();
        let in_force = d.bool()?;
        let mut first_at = 0;
        let (observers, first) = dec_counts(d, &mut first_at)?;
        o.changed.push([start, first_at, in_force_at]);
        Ok(Exempt { key, place, reason, in_force, observers, first })
    })?;
    let kept = dec_list(d, Some(&mut o.kept), dec_place)?;
    let unresolved = dec_list(d, Some(&mut o.unresolved), dec_place)?;
    let refusals = dec_list(d, None, |d| {
        let mut ro = RefusalOffsets { start: d.offset(), ..RefusalOffsets::default() };
        let cause = dec_enum(d)?;
        ro.file = d.offset();
        let file = dec_opt(d, Decoder::uv)?;
        let lo = d.uv()?;
        let hi = d.uv()?;
        ro.reason = d.offset();
        let reason = dec_enum(d)?;
        ro.subject = d.offset();
        let subject = dec_opt(d, Decoder::digest)?;
        ro.display = d.offset();
        let display = dec_opt(d, |d| Ok(d.str()?.to_owned()))?;
        ro.macro_site = d.offset();
        let macro_site = dec_macro(d)?;
        ro.other = d.offset();
        let other = dec_opt(d, dec_place)?;
        ro.observers = d.offset();
        let (observers, first) = dec_counts(d, &mut ro.first)?;
        o.refusal.push(ro);
        Ok(Refusal {
            cause,
            file,
            lo,
            hi,
            reason,
            subject,
            display,
            macro_site,
            other,
            observers,
            first,
        })
    })?;
    let reports = dec_list(d, None, |d| {
        let start = d.offset();
        let report = dec_report(d)?;
        let mut first_at = 0;
        let (observers, first) = dec_counts(d, &mut first_at)?;
        o.report.push([start, first_at]);
        Ok(Reported { report, observers, first })
    })?;
    let failures = dec_list(d, None, |d| {
        let start = d.offset();
        let (failure, subject_at) = dec_failure(d)?;
        let mut first_at = 0;
        let (observers, first) = dec_counts(d, &mut first_at)?;
        o.failure.push([start, first_at, subject_at]);
        Ok(Failed { failure, observers, first })
    })?;
    let unparsed = dec_digests(d)?;
    let counters = dec_list(d, Some(&mut o.counter), |d| {
        Ok(Counter { name: d.str()?.to_owned(), value: d.uv()? })
    })?;
    let instances = dec_list(d, None, |d| {
        let start = d.offset();
        let (instance, subjects_at) = dec_instance(d)?;
        let mut first_at = 0;
        let (observers, first) = dec_counts(d, &mut first_at)?;
        o.instance.push([start, subjects_at, first_at]);
        Ok(Seen { instance, observers, first })
    })?;
    section.finish()?;
    let x = EditSet {
        scheme,
        plan,
        round,
        transform,
        basis,
        state,
        files,
        subjects,
        rules,
        edits,
        groups,
        changed,
        kept,
        unresolved,
        refusals,
        reports,
        failures,
        unparsed,
        counters,
        instances,
    };
    if let Some((l, what)) = violation(&x) {
        return Err(malformed(o.of(l), what));
    }
    Ok((x, opened.extras, opened.content_id))
}

// ---------------------------------------------------------------------------
// JSON

const NAMES: [SectionName; 1] = [SectionName { id: SET, name: "set", required: true }];

const FIELDS: [&str; 20] = [
    "scheme",
    "plan",
    "round",
    "transform",
    "basis",
    "state",
    "files",
    "subjects",
    "rules",
    "edits",
    "groups",
    "changed",
    "kept",
    "unresolved",
    "refusals",
    "reports",
    "failures",
    "unparsed",
    "counters",
    "instances",
];
const EDIT: [&str; 11] = [
    "file",
    "lo",
    "hi",
    "pieces",
    "subjects",
    "tiers",
    "macro",
    "notes",
    "observers",
    "votes",
    "first",
];
const EXEMPT: [&str; 6] = ["key", "place", "reason", "in_force", "observers", "first"];
const REFUSAL: [&str; 11] = [
    "cause",
    "file",
    "lo",
    "hi",
    "reason",
    "subject",
    "display",
    "macro",
    "other",
    "observers",
    "first",
];
const REPORTED: [&str; 3] = ["report", "observers", "first"];
const FAILED: [&str; 3] = ["failure", "observers", "first"];
const COUNTER: [&str; 2] = ["name", "value"];
const SEEN: [&str; 3] = ["instance", "observers", "first"];

fn json_list<T>(list: &[T], f: impl Fn(&T) -> Value) -> Value {
    Value::Array(list.iter().map(f).collect())
}

#[allow(clippy::too_many_lines, reason = "one member per field of spec/edits.md §6")]
pub(super) fn render(x: &EditSet, extras: &[Extra<'_>]) -> Value {
    let edits = json_list(&x.edits, |t| {
        record(
            EDIT,
            [
                json_uv(t.file),
                json_uv(t.lo),
                json_uv(t.hi),
                json_list(&t.pieces, json_piece),
                json_digests(&t.subjects),
                json_uv(t.tiers),
                json_macro(t.macro_site.as_ref()),
                json_list(&t.notes, |n| json_bytes(n)),
                json_uv(t.observers),
                json_uv(t.votes),
                json_digests(&t.first),
            ],
        )
    });
    let changed = json_list(&x.changed, |c| {
        record(
            EXEMPT,
            [
                json_digest(&c.key),
                json_place(&c.place),
                json_bytes(&c.reason),
                Value::Bool(c.in_force),
                json_uv(c.observers),
                json_digests(&c.first),
            ],
        )
    });
    let refusals = json_list(&x.refusals, |r| {
        record(
            REFUSAL,
            [
                json_enum(r.cause),
                json_opt(r.file.as_ref(), |&f| json_uv(f)),
                json_uv(r.lo),
                json_uv(r.hi),
                json_enum(r.reason),
                json_opt(r.subject.as_ref(), json_digest),
                json_opt(r.display.as_ref(), |s| Value::Str(s.clone())),
                json_macro(r.macro_site.as_ref()),
                json_opt(r.other.as_ref(), json_place),
                json_uv(r.observers),
                json_digests(&r.first),
            ],
        )
    });
    let reports = json_list(&x.reports, |r| {
        record(REPORTED, [json_report(&r.report), json_uv(r.observers), json_digests(&r.first)])
    });
    let failures = json_list(&x.failures, |f| {
        record(FAILED, [json_failure(&f.failure), json_uv(f.observers), json_digests(&f.first)])
    });
    let set = record(
        FIELDS,
        [
            json_uv(x.scheme),
            json_digest(&x.plan),
            json_uv(x.round),
            json_digest(&x.transform),
            json_digest(&x.basis),
            json_enum(x.state),
            json_list(&x.files, json_file_ref),
            json_list(&x.subjects, json_subject),
            json_list(&x.rules, json_rule),
            edits,
            json_list(&x.groups, |g| json_list(g, |&m| json_uv(m))),
            changed,
            json_list(&x.kept, json_place),
            json_list(&x.unresolved, json_place),
            refusals,
            reports,
            failures,
            json_digests(&x.unparsed),
            json_list(&x.counters, |c| {
                record(COUNTER, [Value::Str(c.name.clone()), json_uv(c.value)])
            }),
            json_list(&x.instances, |s| {
                record(
                    SEEN,
                    [json_instance(&s.instance), json_uv(s.observers), json_digests(&s.first)],
                )
            }),
        ],
    );
    render_container(EditSet::KIND, EditSet::VERSION, vec![("set", set)], extras)
}

/// The edit set and the unknown ignorable sections of a JSON rendering.
#[allow(clippy::too_many_lines, reason = "one member per field of spec/edits.md §6")]
pub(super) fn read(v: &Value) -> JResult<(EditSet, OwnedExtras)> {
    let s = read_container(v, EditSet::KIND, EditSet::VERSION, &NAMES)?;
    if s.version < SINCE {
        let have = s.version;
        return Err(At::Root.key("version").error(format!(
            "unsupported version {have}; this reader supports {SINCE} to {}",
            EditSet::VERSION
        )));
    }
    let [Some(section)] = s.known else {
        unreachable!("read_container returns every required section")
    };
    let root = At::Root;
    let sections = root.key("sections");
    let at = sections.key("set");
    let [
        scheme,
        plan,
        round,
        transform,
        basis,
        state,
        files,
        subjects,
        rules,
        edits,
        groups,
        changed,
        kept,
        unresolved,
        refusals,
        reports,
        failures,
        unparsed,
        counters,
        instances,
    ] = read_record(section, &at, FIELDS)?;
    let x = EditSet {
        scheme: read_uv(scheme, &at.key("scheme"))?,
        plan: read_digest(plan, &at.key("plan"))?,
        round: read_uv(round, &at.key("round"))?,
        transform: read_digest(transform, &at.key("transform"))?,
        basis: read_digest(basis, &at.key("basis"))?,
        state: read_enum(state, &at.key("state"))?,
        files: read_list(files, &at.key("files"), read_file_ref)?,
        subjects: read_list(subjects, &at.key("subjects"), read_subject)?,
        rules: read_list(rules, &at.key("rules"), read_rule)?,
        edits: read_list(edits, &at.key("edits"), |v, at| {
            let [file, lo, hi, pieces, subjects, tiers, macro_site, notes, observers, votes, first] =
                read_record(v, at, EDIT)?;
            Ok(Edit {
                file: read_uv(file, &at.key("file"))?,
                lo: read_uv(lo, &at.key("lo"))?,
                hi: read_uv(hi, &at.key("hi"))?,
                pieces: read_list(pieces, &at.key("pieces"), read_piece)?,
                subjects: read_list(subjects, &at.key("subjects"), read_digest)?,
                tiers: read_uv(tiers, &at.key("tiers"))?,
                macro_site: read_macro(macro_site, &at.key("macro"))?,
                notes: read_list(notes, &at.key("notes"), read_bytes)?,
                observers: read_uv(observers, &at.key("observers"))?,
                votes: read_uv(votes, &at.key("votes"))?,
                first: read_list(first, &at.key("first"), read_digest)?,
            })
        })?,
        groups: read_list(groups, &at.key("groups"), |v, at| read_list(v, at, read_uv))?,
        changed: read_list(changed, &at.key("changed"), |v, at| {
            let [key, place, reason, in_force, observers, first] = read_record(v, at, EXEMPT)?;
            Ok(Exempt {
                key: read_digest(key, &at.key("key"))?,
                place: read_place(place, &at.key("place"))?,
                reason: read_bytes(reason, &at.key("reason"))?,
                in_force: read_bool(in_force, &at.key("in_force"))?,
                observers: read_uv(observers, &at.key("observers"))?,
                first: read_list(first, &at.key("first"), read_digest)?,
            })
        })?,
        kept: read_list(kept, &at.key("kept"), read_place)?,
        unresolved: read_list(unresolved, &at.key("unresolved"), read_place)?,
        refusals: read_list(refusals, &at.key("refusals"), |v, at| {
            let [
                cause,
                file,
                lo,
                hi,
                reason,
                subject,
                display,
                macro_site,
                other,
                observers,
                first,
            ] = read_record(v, at, REFUSAL)?;
            Ok(Refusal {
                cause: read_enum(cause, &at.key("cause"))?,
                file: read_opt(file, &at.key("file"), read_uv)?,
                lo: read_uv(lo, &at.key("lo"))?,
                hi: read_uv(hi, &at.key("hi"))?,
                reason: read_enum(reason, &at.key("reason"))?,
                subject: read_opt(subject, &at.key("subject"), read_digest)?,
                display: read_opt(display, &at.key("display"), |v, at| {
                    read_str(v, at).map(str::to_owned)
                })?,
                macro_site: read_macro(macro_site, &at.key("macro"))?,
                other: read_opt(other, &at.key("other"), read_place)?,
                observers: read_uv(observers, &at.key("observers"))?,
                first: read_list(first, &at.key("first"), read_digest)?,
            })
        })?,
        reports: read_list(reports, &at.key("reports"), |v, at| {
            let [report, observers, first] = read_record(v, at, REPORTED)?;
            Ok(Reported {
                report: read_report(report, &at.key("report"))?,
                observers: read_uv(observers, &at.key("observers"))?,
                first: read_list(first, &at.key("first"), read_digest)?,
            })
        })?,
        failures: read_list(failures, &at.key("failures"), |v, at| {
            let [failure, observers, first] = read_record(v, at, FAILED)?;
            Ok(Failed {
                failure: read_failure(failure, &at.key("failure"))?,
                observers: read_uv(observers, &at.key("observers"))?,
                first: read_list(first, &at.key("first"), read_digest)?,
            })
        })?,
        unparsed: read_list(unparsed, &at.key("unparsed"), read_digest)?,
        counters: read_list(counters, &at.key("counters"), |v, at| {
            let [name, value] = read_record(v, at, COUNTER)?;
            Ok(Counter {
                name: read_str(name, &at.key("name"))?.to_owned(),
                value: read_uv(value, &at.key("value"))?,
            })
        })?,
        instances: read_list(instances, &at.key("instances"), |v, at| {
            let [instance, observers, first] = read_record(v, at, SEEN)?;
            Ok(Seen {
                instance: read_instance(instance, &at.key("instance"))?,
                observers: read_uv(observers, &at.key("observers"))?,
                first: read_list(first, &at.key("first"), read_digest)?,
            })
        })?,
    };
    if let Some((l, what)) = violation(&x) {
        return Err(JsonFormError { path: l.path(), message: what.to_owned() });
    }
    Ok((x, s.extras))
}
