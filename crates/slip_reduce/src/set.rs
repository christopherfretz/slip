//! From the merged stream of collapsed records to the edit set
//! (`spec/edits.md` §5.2 to §5.5, §6, §6.3): each site through §5.2's
//! treatment and §5.4's table as it streams by, then the geometry of the
//! live sites, the file agreement, the groups, the effect, and the
//! records of the set.

use std::collections::{BTreeMap, BTreeSet};

use slip_model::edits::{
    Failure, FailureKind, HostRefusal, MacroSite, Piece, Place, Reason, Report, Severity,
    SourcePlace, Stance,
};
use slip_model::editset::{
    Cause, Edit, EditSet, Exempt, Failed, Refusal, Reported, SCHEME, SetState,
};
use slip_model::{Counter, Digest, FileRef};

use crate::geometry::{
    Span, at, cyclic, effect, least_crossing, least_sharing, parents, share_byte,
};
use crate::groups::Ties;
use crate::outcome::{Outcome, site};
use crate::records::{ClaimVal, Entry, FailVal, KPiece, Key, Sample, Site, Val, identity};

/// A piece with its copy's file by rank.
#[derive(Clone, Debug)]
pub(crate) enum P2 {
    Literal(Vec<u8>),
    Copy(u32, u64, u64),
    Spelled { key: Digest, text: Vec<u8> },
}

/// Why a live site was rejected after the table.
#[derive(Clone, Debug)]
pub(crate) struct Rejection {
    pub cause: Cause,
    pub other: Option<Span>,
    /// The observers to count, if not the edit's (`contested`: those of the
    /// range that contests it, §6).
    pub sample: Option<Sample>,
}

/// A live site (§5.4): a candidate edit.
#[derive(Clone, Debug)]
pub(crate) struct Live {
    pub pid: u32,
    pub lo: u64,
    pub hi: u64,
    pub pieces: Vec<P2>,
    pub val: ClaimVal,
    pub least_subject: Option<Digest>,
    pub root: Option<Digest>,
    pub rejected: Option<Rejection>,
    /// Rejected with its group (§5.5): a `with_group` refusal.
    pub gone: bool,
}

/// A site at which some `replace` claim remains after §5.2, live or not
/// (landed WP-062 §5): it blocks the live sites that cross it, cross what
/// it copies, or nest in it, as it would if it were live.
#[derive(Clone, Debug)]
pub(crate) struct Member {
    pub span: Span,
    /// The ranges its `replace` claims copy, every claim's, ascending.
    pub copies: Vec<Span>,
    /// Its index in `live`, if it is live.
    pub live: Option<usize>,
}

impl Live {
    fn span(&self) -> Span {
        (at(self.pid, self.lo), at(self.pid, self.hi))
    }

    fn alive(&self) -> bool {
        self.rejected.is_none() && !self.gone
    }

    fn copies(&self) -> impl Iterator<Item = Span> + '_ {
        self.pieces.iter().filter_map(|p| match p {
            P2::Copy(pid, lo, hi) => Some((at(*pid, *lo), at(*pid, *hi))),
            _ => None,
        })
    }

    fn reject(&mut self, cause: Cause, other: Option<Span>) {
        self.rejected = Some(Rejection { cause, other, sample: None });
    }
}

/// A refusal of a site made by the table.
#[derive(Clone, Debug)]
struct SiteRefusal {
    pid: u32,
    lo: u64,
    hi: u64,
    cause: Cause,
    reason: Reason,
    macro_site: Option<MacroSite>,
    subject: Option<Digest>,
    sample: Sample,
    other: Option<Span>,
}

type PPlace = (u32, u64, u64);
type ReportRec = (Option<PPlace>, Severity, Vec<u8>, bool, Sample);
type Pending = (Site, Vec<(Stance, Vec<KPiece>, ClaimVal)>, Vec<(Vec<Digest>, Sample)>);

/// What the stream leaves in memory: the records of the set, by rank of
/// path, and the live sites.
#[derive(Debug, Default)]
pub(crate) struct Builder {
    /// Every path a shard's `files` named, ascending: a path's rank.
    pub paths: Vec<Vec<u8>>,
    /// The `(size, content)` each path was given, ascending.
    pub versions: Vec<Vec<(u64, Digest)>>,
    pub live: Vec<Live>,
    /// Every site with a `replace` claim, ascending.
    members: Vec<Member>,
    blocking: Vec<(Span, Sample)>,
    kept: Vec<PPlace>,
    unresolved: Vec<PPlace>,
    refusals: Vec<SiteRefusal>,
    /// The rejected groups, by root, with the least site of a member with
    /// a refusal of its own: absent if a refusal of §5.2 alone took the
    /// group down.
    rejected_roots: BTreeMap<Digest, Option<Span>>,
    roots: BTreeSet<Digest>,
    /// Edits with no tie, and how many of them the table rejected.
    ungrouped: u64,
    ungrouped_rejected: u64,
    /// `replace` claims that remain `replace` after §5.2.
    replaces: u64,
    reports: Vec<ReportRec>,
    changed: Vec<(Digest, PPlace, Vec<u8>, Sample)>,
    failures: Vec<(FailureKind, String, Option<SourcePlace>, FailVal)>,
    pending: Option<Pending>,
    sites: u64,
    sites_live: u64,
    sites_kept: u64,
    sites_unresolved: u64,
}

/// Records that the group of root `r` is rejected, by a member at `site`
/// (with a refusal of its own) or by a refusal of §5.2 (`None`).
fn note_root(roots: &mut BTreeMap<Digest, Option<Span>>, r: Digest, site: Option<Span>) {
    let e = roots.entry(r).or_insert(site);
    *e = match (*e, site) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    };
}

fn corrupt(what: &str) -> String {
    format!("the merged records of reduce are corrupt: {what}")
}

/// What the stream needs to know: the selection and the ties.
pub(crate) struct Context<'a> {
    pub selected: &'a BTreeSet<Digest>,
    pub ties: &'a mut Ties,
}

impl Builder {
    fn pid(&self, path: &[u8]) -> Result<u32, String> {
        self.paths
            .binary_search_by(|p| p.as_slice().cmp(path))
            .map(|i| i as u32)
            .map_err(|_| corrupt("a record names a file no shard listed"))
    }

    fn place(&self, s: &Site) -> Result<PPlace, String> {
        Ok((self.pid(&s.path)?, s.lo, s.hi))
    }

    /// One record of the merged stream, in order.
    pub fn push(&mut self, key: Key, val: Val, cx: &mut Context<'_>) -> Result<(), String> {
        if let Key::Claim { site: s, entry } = key {
            if self.pending.as_ref().is_none_or(|p| p.0 != s) {
                self.flush(cx)?;
                self.pending = Some((s, Vec::new(), Vec::new()));
            }
            let Some((_, claims, observed)) = &mut self.pending else { unreachable!("set above") };
            match (entry, val) {
                (Entry::Claim { stance, pieces }, Val::Claim(v)) => {
                    claims.push((stance, pieces, v));
                }
                (Entry::Observed(variants), Val::Sample(v)) => observed.push((variants, v)),
                _ => return Err(corrupt("a claim without its value")),
            }
            return Ok(());
        }
        self.flush(cx)?;
        match (key, val) {
            (Key::Observer(_), _) => return Err(corrupt("an observer record after the first")),
            (Key::File { path, size, content }, Val::File) => {
                if self.paths.last() != Some(&path) {
                    self.paths.push(path);
                    self.versions.push(Vec::new());
                }
                if let Some(v) = self.versions.last_mut() {
                    v.push((size, content));
                }
            }
            (Key::Report { place, severity, message, attached }, Val::Sample(s)) => {
                let place = place.as_ref().map(|p| self.place(p)).transpose()?;
                self.reports.push((place, severity, message, attached, s));
            }
            (Key::Changed { key, place, reason }, Val::Sample(s)) => {
                let place = self.place(&place)?;
                self.changed.push((key, place, reason, s));
            }
            (Key::Failure { kind, message, place }, Val::Failure(f)) => {
                if let Some(r) = &f.refusal
                    && let Some(s) = &r.subject
                {
                    self.place(s)?;
                }
                let place = place.map(|(file, line, column)| SourcePlace { file, line, column });
                self.failures.push((kind, message, place, f));
            }
            _ => return Err(corrupt("a record with a value of another kind")),
        }
        Ok(())
    }

    /// The end of the stream.
    pub fn finish(&mut self, cx: &mut Context<'_>) -> Result<(), String> {
        self.flush(cx)
    }

    /// The site whose claims have all been read: §5.2's treatment and
    /// §5.4's table.
    fn flush(&mut self, cx: &mut Context<'_>) -> Result<(), String> {
        let Some((s, claims, observed)) = self.pending.take() else { return Ok(()) };
        let pid = self.pid(&s.path)?;
        let out = site(&s, claims, &observed, cx.selected);
        self.sites += 1;
        let rejected_site = matches!(out.outcome, Outcome::Rejected { .. });
        let span = (at(pid, s.lo), at(pid, s.hi));
        let mut live_root = None;
        let mut copies = Vec::new();
        for (pieces, _) in &out.edits {
            self.replaces += 1;
            for p in pieces {
                if let KPiece::Copy(c) = p {
                    let cp = self.pid(&c.path)?;
                    copies.push((at(cp, c.lo), at(cp, c.hi)));
                }
            }
            let root = cx.ties.find(&identity(&s, pieces));
            if let Some(r) = root {
                self.roots.insert(r);
                if rejected_site {
                    note_root(&mut self.rejected_roots, r, Some(span));
                }
            } else {
                self.ungrouped += 1;
                if rejected_site {
                    self.ungrouped_rejected += 1;
                }
            }
            live_root = root;
        }
        if !out.edits.is_empty() {
            copies.sort_unstable();
            copies.dedup();
            let live = matches!(out.outcome, Outcome::Live { .. }).then_some(self.live.len());
            self.members.push(Member { span, copies, live });
        }
        if let Some(b) = out.blocking
            && s.lo < s.hi
        {
            self.blocking.push((span, b));
        }
        match out.outcome {
            Outcome::Live { pieces, val } => {
                let mut p2 = Vec::with_capacity(pieces.len());
                for p in pieces {
                    p2.push(match p {
                        KPiece::Literal(b) => P2::Literal(b),
                        KPiece::Copy(c) => P2::Copy(self.pid(&c.path)?, c.lo, c.hi),
                        KPiece::Spelled { key, text } => P2::Spelled { key, text },
                    });
                }
                self.sites_live += 1;
                self.live.push(Live {
                    pid,
                    lo: s.lo,
                    hi: s.hi,
                    pieces: p2,
                    val,
                    least_subject: out.least_subject,
                    root: live_root,
                    rejected: None,
                    gone: false,
                });
            }
            Outcome::Kept => {
                self.sites_kept += 1;
                self.kept.push((pid, s.lo, s.hi));
            }
            Outcome::Unresolved => {
                self.sites_unresolved += 1;
                self.unresolved.push((pid, s.lo, s.hi));
            }
            Outcome::Rejected { cause, reason, macro_site, sample } => {
                self.refusals.push(SiteRefusal {
                    pid,
                    lo: s.lo,
                    hi: s.hi,
                    cause,
                    reason,
                    macro_site,
                    subject: out.least_subject,
                    sample,
                    other: None,
                });
            }
            Outcome::Nothing => {}
        }
        Ok(())
    }

    fn alive(&self) -> Vec<usize> {
        (0..self.live.len()).filter(|&i| self.live[i].alive()).collect()
    }

    /// §5.4's geometry, rule by rule, each over the live sites the rules
    /// before it left: `contested` by another range, `overlap`, `cyclic`.
    pub fn geometry(&mut self) {
        // contested
        let blocking: Vec<Span> = self.blocking.iter().map(|b| b.0).collect();
        let spans: Vec<Span> = self.live.iter().map(Live::span).collect();
        for (i, hit) in least_sharing(&blocking, &spans).into_iter().enumerate() {
            if let Some(b) = hit {
                self.live[i].rejected = Some(Rejection {
                    cause: Cause::Contested,
                    other: Some(self.blocking[b].0),
                    sample: Some(self.blocking[b].1.clone()),
                });
            }
        }

        // overlap, against every member (landed WP-062 §5): a live site
        // that crosses a member or a range a member copies, and a live site
        // whose copy a member crosses. Every pair over one set, then applied
        // together.
        let alive = self.alive();
        let s: Vec<Span> = alive.iter().map(|&i| self.live[i].span()).collect();
        let g: Vec<Span> = self.members.iter().map(|m| m.span).collect();
        let mut gc: Vec<Span> =
            self.members.iter().flat_map(|m| m.copies.iter().copied()).collect();
        gc.sort_unstable();
        gc.dedup();
        let mut copies: Vec<(Span, usize)> =
            alive.iter().flat_map(|&i| self.live[i].copies().map(move |c| (c, i))).collect();
        copies.sort_unstable();
        let mut c: Vec<Span> = copies.iter().map(|x| x.0).collect();
        c.dedup();
        let site_member = least_crossing(&g, &s);
        let site_copy = least_crossing(&gc, &s);
        let copy_member = least_crossing(&g, &c);
        let mut hit: BTreeMap<usize, Span> = BTreeMap::new();
        let mut note = |i: usize, other: Span| {
            let e = hit.entry(i).or_insert(other);
            *e = (*e).min(other);
        };
        for (k, &i) in alive.iter().enumerate() {
            if let Some(x) = site_member[k] {
                note(i, g[x]);
            }
            if let Some(x) = site_copy[k] {
                note(i, gc[x]);
            }
        }
        for (ci, x) in copy_member.iter().enumerate() {
            if let Some(x) = x {
                let lo = copies.partition_point(|y| y.0 < c[ci]);
                let hi = copies.partition_point(|y| y.0 <= c[ci]);
                for y in &copies[lo..hi] {
                    note(y.1, g[*x]);
                }
            }
        }
        for (i, other) in hit {
            self.live[i].reject(Cause::Overlap, Some(other));
        }

        // cyclic, over every member: a live site left on a cycle.
        let member_copies: Vec<(usize, Span)> = self
            .members
            .iter()
            .enumerate()
            .flat_map(|(k, m)| m.copies.iter().map(move |&c| (k, c)))
            .collect();
        for k in cyclic(&g, &member_copies) {
            if let Some(i) = self.members[k].live
                && self.live[i].alive()
            {
                self.live[i].reject(Cause::Cyclic, None);
            }
        }
    }

    /// The ranks of the files whose agreement pass 2 checks (§5.4): those
    /// of the live sites left and of the files they copy from.
    pub fn agreement_files(&self) -> BTreeSet<u32> {
        let mut out = BTreeSet::new();
        for l in self.live.iter().filter(|l| l.alive()) {
            out.insert(l.pid);
            for p in &l.pieces {
                if let P2::Copy(pid, _, _) = p {
                    out.insert(*pid);
                }
            }
        }
        out
    }

    /// `file_differs` (§5.4): `seen` holds, for a file checked, every
    /// `(size, content)` pass 2 found.
    pub fn file_agreement(&mut self, seen: &BTreeMap<u32, BTreeSet<(u64, Digest)>>) {
        let differs = |pid: u32| {
            let mut all: BTreeSet<(u64, Digest)> =
                self.versions[pid as usize].iter().copied().collect();
            if let Some(s) = seen.get(&pid) {
                all.extend(s.iter().copied());
            }
            all.len() > 1
        };
        let bad: BTreeSet<u32> =
            self.agreement_files().into_iter().filter(|&p| differs(p)).collect();
        for l in self.live.iter_mut().filter(|l| l.alive()) {
            let copies_bad =
                l.pieces.iter().any(|p| matches!(p, P2::Copy(pid, _, _) if bad.contains(pid)));
            if bad.contains(&l.pid) || copies_bad {
                l.reject(Cause::FileDiffers, None);
            }
        }
    }

    /// §5.5: groups rejected, then effect, repeated until nothing more is
    /// rejected. `indivisible`: every edit is one group. `refused_subjects`:
    /// the subjects some refusal of §5.2 names; `subject_refusals`: whether
    /// there is any.
    pub fn groups(
        &mut self,
        indivisible: bool,
        refused_subjects: &BTreeSet<Digest>,
        subject_refusals: bool,
    ) {
        self.reject_groups(indivisible, refused_subjects, subject_refusals);
        let g: Vec<Span> = self.members.iter().map(|m| m.span).collect();
        loop {
            // The carriers: the live sites that remain. Every member is a
            // container, whatever became of it (landed WP-062 §5).
            let carriers: Vec<(usize, Span)> = self
                .members
                .iter()
                .enumerate()
                .filter(|(_, m)| m.live.is_some_and(|i| self.live[i].alive()))
                .flat_map(|(k, m)| m.copies.iter().map(move |&c| (k, c)))
                .collect();
            let mut other: BTreeMap<usize, Span> = BTreeMap::new();
            for (x, p) in effect(&g, &carriers) {
                for (a, b) in [(x, p), (p, x)] {
                    if let Some(i) = self.members[a].live
                        && self.live[i].alive()
                    {
                        let e = other.entry(i).or_insert(g[b]);
                        *e = (*e).min(g[b]);
                    }
                }
            }
            if other.is_empty() {
                break;
            }
            for (i, o) in other {
                self.live[i].reject(Cause::Buried, Some(o));
            }
            self.reject_groups(indivisible, refused_subjects, subject_refusals);
        }
    }

    /// A group is rejected if one of its edits is at a rejected site, or a
    /// refusal of §5.2 names a subject one of its claims names; under an
    /// `indivisible` request, also if any site was refused (landed WP-062
    /// §5).
    fn reject_groups(
        &mut self,
        indivisible: bool,
        refused: &BTreeSet<Digest>,
        subject_refusals: bool,
    ) {
        if indivisible {
            let any = subject_refusals
                || !self.refusals.is_empty()
                || self.live.iter().any(|l| l.rejected.is_some());
            if any {
                for l in &mut self.live {
                    l.gone = l.rejected.is_none();
                }
            }
            return;
        }
        for l in &mut self.live {
            let named = l.val.subjects.iter().any(|k| refused.contains(k));
            let span = l.span();
            match l.root {
                Some(r) if l.rejected.is_some() => {
                    note_root(&mut self.rejected_roots, r, Some(span));
                }
                Some(r) if named && !l.gone => note_root(&mut self.rejected_roots, r, None),
                None if l.rejected.is_none() && named => l.gone = true,
                _ => {}
            }
        }
        for l in &mut self.live {
            if l.alive() && l.root.is_some_and(|r| self.rejected_roots.contains_key(&r)) {
                l.gone = true;
            }
        }
    }
}

/// What the edit set is made of beyond the builder.
pub(crate) struct Parts<'a> {
    pub plan: Digest,
    pub round: u64,
    pub transform: Digest,
    pub basis: Digest,
    pub indivisible: bool,
    pub subjects: Vec<slip_model::edits::Subject>,
    pub rules: Vec<slip_model::keys::Rule>,
    pub subject_refusals: Vec<crate::subjects::SubjectRefusal>,
    pub no_observer: bool,
    pub rule_conflict: bool,
    pub unparsed: Vec<Digest>,
    pub counters: BTreeMap<String, u64>,
    pub observer: &'a dyn Fn(u64) -> Digest,
}

fn place_of(s: Span) -> PPlace {
    ((s.0 >> 64) as u32, s.0 as u64, s.1 as u64)
}

impl Builder {
    /// The edit set (§6).
    #[allow(clippy::too_many_lines)]
    pub fn assemble(self, mut parts: Parts<'_>) -> EditSet {
        let first = |s: &Sample| s.first.iter().map(|&i| (parts.observer)(i)).collect::<Vec<_>>();
        let set_refused =
            !parts.subject_refusals.is_empty() || parts.no_observer || parts.rule_conflict;
        let finals: Vec<&Live> = self.live.iter().filter(|l| l.alive()).collect();
        let accepted = !set_refused && !finals.is_empty();
        let finals: Vec<&Live> = if accepted { finals } else { Vec::new() };

        // Refusals of sites.
        let mut site_refusals: Vec<SiteRefusal> = self.refusals.clone();
        for l in &self.live {
            if let Some(r) = &l.rejected {
                site_refusals.push(SiteRefusal {
                    pid: l.pid,
                    lo: l.lo,
                    hi: l.hi,
                    cause: r.cause,
                    reason: Reason::None,
                    macro_site: l.val.macro_site.clone(),
                    subject: l.least_subject,
                    sample: r.sample.clone().unwrap_or_else(|| l.val.sample.clone()),
                    other: r.other,
                });
            } else if l.gone && !parts.indivisible {
                // Taken down with its group (§5.5): named by a refusal of
                // its own, with the least member that has one (landed
                // WP-062 §6). Under an `indivisible` request the refused
                // state says it.
                site_refusals.push(SiteRefusal {
                    pid: l.pid,
                    lo: l.lo,
                    hi: l.hi,
                    cause: Cause::WithGroup,
                    reason: Reason::None,
                    macro_site: l.val.macro_site.clone(),
                    subject: l.least_subject,
                    sample: l.val.sample.clone(),
                    other: l.root.and_then(|r| self.rejected_roots.get(&r).copied().flatten()),
                });
            }
        }

        // The files the records name, and their indices.
        let mut named: BTreeSet<u32> = BTreeSet::new();
        for l in &finals {
            named.insert(l.pid);
            for p in &l.pieces {
                if let P2::Copy(pid, _, _) = p {
                    named.insert(*pid);
                }
            }
        }
        named.extend(self.changed.iter().map(|c| c.1.0));
        named.extend(self.kept.iter().map(|k| k.0));
        named.extend(self.unresolved.iter().map(|k| k.0));
        for r in &site_refusals {
            named.insert(r.pid);
            if let Some(o) = r.other {
                named.insert(place_of(o).0);
            }
        }
        named.extend(self.reports.iter().filter_map(|r| r.0.map(|p| p.0)));
        for (.., f) in &self.failures {
            if let Some(Some(s)) = f.refusal.as_ref().map(|r| r.subject.as_ref()) {
                // `push` checked the path.
                if let Ok(i) = self.paths.binary_search_by(|p| p.as_slice().cmp(&s.path)) {
                    named.insert(i as u32);
                }
            }
        }
        let mut index = vec![u64::MAX; self.paths.len()];
        let mut files = Vec::with_capacity(named.len());
        for &pid in &named {
            index[pid as usize] = files.len() as u64;
            // The greatest size, then the least content: every place any
            // shard gave in the file is within it (landed WP-062 §6).
            let vs = &self.versions[pid as usize];
            let size = vs.iter().map(|v| v.0).max().unwrap_or(0);
            let content =
                vs.iter().filter(|v| v.0 == size).map(|v| v.1).min().unwrap_or(Digest([0; 32]));
            files.push(FileRef { path: self.paths[pid as usize].clone(), size, content });
        }
        let pl = |p: PPlace| Place { file: index[p.0 as usize], lo: p.1, hi: p.2 };

        // Edits.
        let edits: Vec<Edit> = finals
            .iter()
            .map(|l| Edit {
                file: index[l.pid as usize],
                lo: l.lo,
                hi: l.hi,
                pieces: l
                    .pieces
                    .iter()
                    .map(|p| match p {
                        P2::Literal(b) => Piece::Literal(b.clone()),
                        P2::Copy(pid, lo, hi) => Piece::Copy(pl((*pid, *lo, *hi))),
                        P2::Spelled { key, text } => {
                            Piece::Spelled { key: *key, text: text.clone() }
                        }
                    })
                    .collect(),
                subjects: l.val.subjects.clone(),
                tiers: l.val.tiers,
                macro_site: l.val.macro_site.clone(),
                notes: l.val.notes.clone(),
                observers: l.val.sample.observers,
                votes: l.val.votes,
                first: first(&l.val.sample),
            })
            .collect();
        let mut groups: Vec<Vec<u64>> = Vec::new();
        if !parts.indivisible {
            let mut by_root: BTreeMap<Digest, Vec<u64>> = BTreeMap::new();
            for (i, l) in finals.iter().enumerate() {
                if let Some(r) = l.root {
                    by_root.entry(r).or_default().push(i as u64);
                }
            }
            groups = by_root.into_values().filter(|g| g.len() >= 2).collect();
            groups.sort();
        }

        // `changed`, in force where an edit shares a byte with the place.
        let spans: Vec<Span> = finals.iter().map(|l| l.span()).collect();
        let mut reach: Vec<u128> = Vec::with_capacity(spans.len());
        for s in &spans {
            reach.push(reach.last().map_or(s.1, |&m: &u128| m.max(s.1)));
        }
        let in_force = |p: PPlace| {
            let q = (at(p.0, p.1), at(p.0, p.2));
            let k = spans.partition_point(|s| s.0 < q.1);
            // The edits before `k` start before the place ends; one shares
            // a byte iff the furthest end among them is past its start.
            k > 0 && reach[k - 1] > q.0 && share_byte(q, q)
        };
        let changed: Vec<Exempt> = self
            .changed
            .iter()
            .map(|(key, p, reason, s)| Exempt {
                key: *key,
                place: pl(*p),
                reason: reason.clone(),
                in_force: in_force(*p),
                observers: s.observers,
                first: first(s),
            })
            .collect();

        // Refusals.
        let mut refusals: Vec<Refusal> = Vec::new();
        let alone = |cause| Refusal {
            cause,
            file: None,
            lo: 0,
            hi: 0,
            reason: Reason::None,
            subject: None,
            macro_site: None,
            other: None,
            observers: 0,
            first: Vec::new(),
        };
        if parts.no_observer {
            refusals.push(alone(Cause::NoObserver));
        }
        if parts.rule_conflict {
            refusals.push(alone(Cause::RuleConflict));
        }
        if self.replaces == 0 {
            refusals.push(alone(Cause::NoEdit));
        }
        for r in &parts.subject_refusals {
            refusals.push(Refusal {
                subject: r.subject,
                observers: r.sample.observers,
                first: first(&r.sample),
                ..alone(r.cause)
            });
        }
        let mut rejected_sites: BTreeSet<PPlace> = BTreeSet::new();
        for r in &site_refusals {
            rejected_sites.insert((r.pid, r.lo, r.hi));
            refusals.push(Refusal {
                cause: r.cause,
                file: Some(index[r.pid as usize]),
                lo: r.lo,
                hi: r.hi,
                reason: r.reason,
                subject: r.subject,
                macro_site: r.macro_site.clone(),
                other: r.other.map(|o| pl(place_of(o))),
                observers: r.sample.observers,
                first: first(&r.sample),
            });
        }
        refusals.sort_by(Refusal::cmp_key);

        let reports: Vec<Reported> = self
            .reports
            .iter()
            .map(|(place, severity, message, attached, s)| Reported {
                report: Report {
                    place: place.map(pl),
                    severity: *severity,
                    message: message.clone(),
                    attached: *attached,
                },
                observers: s.observers,
                first: first(s),
            })
            .collect();
        let failures: Vec<Failed> = self
            .failures
            .iter()
            .map(|(kind, message, place, f)| Failed {
                failure: Failure {
                    kind: *kind,
                    message: message.clone(),
                    place: place.clone(),
                    rendered: f.rendered.clone(),
                    refusal: f.refusal.as_ref().map(|r| HostRefusal {
                        function: r.function.clone(),
                        errc: r.errc.clone(),
                        message: r.message.clone(),
                        subject: r.subject.as_ref().and_then(|s| {
                            let pid =
                                self.paths.binary_search_by(|p| p.as_slice().cmp(&s.path)).ok()?;
                            Some(pl((pid as u32, s.lo, s.hi)))
                        }),
                    }),
                },
                observers: f.sample.observers,
                first: first(&f.sample),
            })
            .collect();

        // Counters (§6.3).
        let c = &mut parts.counters;
        let mut set = |name: &str, v: u64| {
            c.insert(name.to_owned(), v);
        };
        set("reduce.sites", self.sites);
        set("reduce.sites.live", self.sites_live);
        set("reduce.sites.kept", self.sites_kept);
        set("reduce.sites.unresolved", self.sites_unresolved);
        set("reduce.sites.rejected", rejected_sites.len() as u64);
        let (groups_all, groups_rejected) = if parts.indivisible {
            let any = self.replaces > 0;
            let rejected = any && (set_refused || edits.is_empty());
            (u64::from(any), u64::from(rejected))
        } else {
            let live_ungrouped = self.live.iter().filter(|l| l.root.is_none()).count() as u64;
            let live_ungrouped_rejected =
                self.live.iter().filter(|l| l.root.is_none() && !l.alive()).count() as u64;
            let mut rejected_roots: BTreeSet<Digest> =
                self.rejected_roots.keys().copied().collect();
            if set_refused {
                rejected_roots.clone_from(&self.roots);
            }
            let ungrouped_rejected = if set_refused {
                self.ungrouped
            } else {
                self.ungrouped_rejected + live_ungrouped_rejected
            };
            let _ = live_ungrouped;
            (
                self.roots.len() as u64 + self.ungrouped,
                rejected_roots.len() as u64 + ungrouped_rejected,
            )
        };
        set("reduce.groups", groups_all);
        set("reduce.groups.rejected", groups_rejected);
        set("reduce.edits", edits.len() as u64);
        set("reduce.edits.shared", edits.iter().filter(|e| e.observers > 1).count() as u64);
        set("reduce.edits.max_observers", edits.iter().map(|e| e.observers).max().unwrap_or(0));
        set("reduce.edits.nested", parents(&spans).len() as u64);
        set("reduce.files", edits.iter().map(|e| e.file).collect::<BTreeSet<_>>().len() as u64);
        for (sev, name) in
            [(Severity::Note, "note"), (Severity::Review, "review"), (Severity::Error, "error")]
        {
            set(
                &format!("reduce.reports.{name}"),
                reports.iter().filter(|r| r.report.severity == sev).count() as u64,
            );
        }
        set("reduce.changed", changed.len() as u64);
        set("reduce.changed.in_force", changed.iter().filter(|c| c.in_force).count() as u64);
        let counters: Vec<Counter> =
            parts.counters.into_iter().map(|(name, value)| Counter { name, value }).collect();

        EditSet {
            scheme: SCHEME,
            plan: parts.plan,
            round: parts.round,
            transform: parts.transform,
            basis: parts.basis,
            state: if accepted { SetState::Accepted } else { SetState::Refused },
            files,
            subjects: parts.subjects,
            rules: parts.rules,
            edits,
            groups,
            changed,
            kept: self.kept.iter().map(|&p| pl(p)).collect(),
            unresolved: self.unresolved.iter().map(|&p| pl(p)).collect(),
            refusals,
            reports,
            failures,
            unparsed: parts.unparsed,
            counters,
        }
    }
}
