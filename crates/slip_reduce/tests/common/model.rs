//! A second, deliberately naive implementation of reduce (`spec/edits.md`
//! §5, §6, with the decisions of `spec/landed/edits/WP-062.md`):
//! everything in memory, sets of observer indices instead of samples, every
//! pair of ranges compared, fixpoints by iteration, no runs. `order.rs`
//! compares it with `slip_reduce::reduce` on every generated set. It reads
//! shards that are valid (§4.7) and raise no `Input` error.

#![allow(clippy::single_match_else, clippy::many_single_char_names)]

use std::collections::{BTreeMap, BTreeSet};

use slip_model::edits::{
    Failure, HostRefusal, MacroSite, Piece, Place, Reason, Report, Role, Severity, Stance, Subject,
    subject_flags,
};
use slip_model::editset::{
    Basis, Cause, Edit, EditSet, Exempt, FIRST_MAX, Failed, Refusal, Reported, SCHEME, SetState,
    basis_id, cross, properly_inside, share_byte, within,
};
use slip_model::keys::{KeyKind, Rule};
use slip_model::transform::{Transform, TransformKind};
use slip_model::{Counter, Digest, FileRef, Shard, Status};

/// A place by path.
type PP = (Vec<u8>, u64, u64);

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum MP {
    L(Vec<u8>),
    C(PP),
    S(Digest, Vec<u8>),
}

#[derive(Clone, Debug, Default)]
struct Joined {
    subjects: BTreeSet<Digest>,
    tiers: u64,
    macro_site: Option<MacroSite>,
    reason: Option<Reason>,
    notes: BTreeSet<Vec<u8>>,
    votes: u64,
    obs: BTreeSet<usize>,
}

fn first(obs: &BTreeSet<usize>, observers: &[Digest]) -> Vec<Digest> {
    obs.iter().take(FIRST_MAX).map(|&i| observers[i]).collect()
}

struct Subj {
    rule: Option<Rule>,
    role: Role,
    of: BTreeSet<Digest>,
    flags: u64,
    display: String,
    seen: BTreeSet<usize>,
    flag_seen: [BTreeSet<usize>; 4],
}

#[derive(Clone, Debug)]
struct Live {
    site: PP,
    pieces: Vec<MP>,
    j: Joined,
    least_subject: Option<Digest>,
    rejected: Option<(Cause, Option<PP>, Option<BTreeSet<usize>>)>,
    gone: bool,
    id: (PP, Vec<MP>),
}

impl Live {
    fn alive(&self) -> bool {
        self.rejected.is_none() && !self.gone
    }
    fn copies(&self) -> Vec<PP> {
        self.pieces
            .iter()
            .filter_map(|p| if let MP::C(c) = p { Some(c.clone()) } else { None })
            .collect()
    }
}

fn same_file_share(a: &PP, b: &PP) -> bool {
    a.0 == b.0 && share_byte((a.1, a.2), (b.1, b.2))
}
fn same_file_cross(a: &PP, b: &PP) -> bool {
    a.0 == b.0 && cross((a.1, a.2), (b.1, b.2))
}
fn same_file_within(a: &PP, b: &PP) -> bool {
    a.0 == b.0 && within((a.1, a.2), (b.1, b.2))
}
fn same_file_inside(a: &PP, b: &PP) -> bool {
    a.0 == b.0 && properly_inside((a.1, a.2), (b.1, b.2))
}

struct SiteRef {
    site: PP,
    cause: Cause,
    reason: Reason,
    macro_site: Option<MacroSite>,
    subject: Option<Digest>,
    obs: BTreeSet<usize>,
    other: Option<PP>,
}

fn note_root(roots: &mut BTreeMap<(PP, Vec<MP>), Option<PP>>, r: (PP, Vec<MP>), site: Option<PP>) {
    let e = roots.entry(r).or_insert_with(|| site.clone());
    if let Some(s) = site
        && e.as_ref().is_none_or(|x| s < *x)
    {
        *e = Some(s);
    }
}

/// Union-find by plain maps, least root.
#[derive(Default)]
struct Uf(BTreeMap<(PP, Vec<MP>), (PP, Vec<MP>)>);

impl Uf {
    fn root(&self, x: &(PP, Vec<MP>)) -> Option<(PP, Vec<MP>)> {
        let mut r = self.0.get(x)?.clone();
        while let Some(p) = self.0.get(&r) {
            if *p == r {
                break;
            }
            r = p.clone();
        }
        Some(r)
    }
}

fn least_macro(a: Option<MacroSite>, b: Option<MacroSite>) -> Option<MacroSite> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    }
}

/// `(rule_conflict, table)`: as §6.2 and landed WP-062 §7.
fn conflicts(rules: &BTreeSet<Rule>) -> bool {
    let v: Vec<&Rule> = rules.iter().collect();
    for (i, a) in v.iter().enumerate() {
        for b in &v[i + 1..] {
            let same_match =
                (a.parent, a.kind, &a.name, a.disc) == (b.parent, b.kind, &b.name, b.disc);
            let same_target = a.kind != KeyKind::Namespace
                && (a.to_parent, a.kind, &a.to_name, a.disc)
                    == (b.to_parent, b.kind, &b.to_name, b.disc)
                && (a.parent, &a.name) != (b.parent, &b.name);
            if same_match || same_target {
                return true;
            }
        }
    }
    false
}

/// The edit set of `shards` (plan order: observer, shard, its id).
#[allow(clippy::too_many_lines)]
pub fn reduce(
    request: &Transform,
    request_id: Digest,
    plan: Digest,
    round: u64,
    shards: &[(Digest, Option<Shard>, Option<Digest>)],
) -> EditSet {
    let observers: Vec<Digest> = shards.iter().map(|s| s.0).collect();
    let mut basis = Vec::new();
    let mut unparsed = Vec::new();
    let mut ok: Vec<(usize, &Shard)> = Vec::new();
    for (i, (o, s, id)) in shards.iter().enumerate() {
        match s {
            Some(s) if s.outcome.status == Status::Ok => {
                basis.push(Basis { observer: *o, shard: *id });
                ok.push((i, s));
            }
            _ => {
                basis.push(Basis { observer: *o, shard: None });
                unparsed.push(*o);
            }
        }
    }

    // §5.1, §5.2 rule 1, §5.3.
    let mut subjects: BTreeMap<Digest, Subj> = BTreeMap::new();
    let mut links: BTreeSet<(Digest, Digest)> = BTreeSet::new();
    let mut renamed: BTreeSet<Rule> = BTreeSet::new();
    let mut claims: BTreeMap<PP, BTreeMap<(Stance, Vec<MP>), Joined>> = BTreeMap::new();
    let mut per_observer: BTreeMap<PP, BTreeMap<usize, BTreeSet<(Stance, Vec<MP>)>>> =
        BTreeMap::new();
    let mut reports: BTreeMap<(Option<PP>, Severity, Vec<u8>, bool), BTreeSet<usize>> =
        BTreeMap::new();
    let mut changed: BTreeMap<(Digest, PP, Vec<u8>), BTreeSet<usize>> = BTreeMap::new();
    #[allow(clippy::type_complexity)]
    let mut failures: BTreeMap<
        (u8, String, Option<(Vec<u8>, u64, u64)>),
        (BTreeSet<usize>, Failure, Option<Option<PP>>),
    > = BTreeMap::new();
    let mut versions: BTreeMap<Vec<u8>, BTreeSet<(u64, Digest)>> = BTreeMap::new();
    let mut all_versions: BTreeMap<Vec<u8>, BTreeSet<(u64, Digest)>> = BTreeMap::new();
    let mut ties: Vec<Vec<(PP, Vec<MP>)>> = Vec::new();
    let mut shard_counters: BTreeMap<String, u64> = BTreeMap::new();
    let mut n_claims = 0u64;
    let mut by_stance = [0u64; 4];
    let mut failed_runs = 0;
    for &(i, s) in &ok {
        for c in &s.counters {
            if c.name.starts_with("api.") || c.name.starts_with("edits.") {
                let e = shard_counters.entry(c.name.clone()).or_insert(0);
                *e = if c.name == "api.peak_bytes" { (*e).max(c.value) } else { *e + c.value };
            }
        }
        let e = s.edits.as_ref().unwrap();
        for f in &e.files {
            versions.entry(f.path.clone()).or_default().insert((f.size, f.content));
            all_versions.entry(f.path.clone()).or_default().insert((f.size, f.content));
        }
        for f in s.inputs.iter().flatten() {
            all_versions.entry(f.path.clone()).or_default().insert((f.size, f.content));
        }
        let pp = |p: &Place| (e.files[p.file as usize].path.clone(), p.lo, p.hi);
        if let Some(f) = &e.failure {
            failed_runs += 1;
            let key = (
                f.kind as u8,
                f.message.clone(),
                f.place.as_ref().map(|p| (p.file.clone(), p.line, p.column)),
            );
            let subject = f.refusal.as_ref().map(|r| r.subject.as_ref().map(pp));
            let entry = failures
                .entry(key)
                .or_insert_with(|| (BTreeSet::new(), f.clone(), subject.clone()));
            entry.0.insert(i);
            continue;
        }
        let mp = |p: &Piece| match p {
            Piece::Literal(b) => MP::L(b.clone()),
            Piece::Copy(c) => MP::C(pp(c)),
            Piece::Spelled { key, text } => MP::S(*key, text.clone()),
        };
        for c in &e.claims {
            n_claims += 1;
            by_stance[c.stance as usize] += 1;
            let site = (e.files[c.file as usize].path.clone(), c.lo, c.hi);
            let pieces: Vec<MP> = c.pieces.iter().map(mp).collect();
            per_observer
                .entry(site.clone())
                .or_default()
                .entry(i)
                .or_default()
                .insert((c.stance, pieces.clone()));
            let j = claims.entry(site).or_default().entry((c.stance, pieces)).or_default();
            j.subjects.extend(c.subjects.iter().copied());
            j.tiers |= c.tiers;
            j.macro_site = least_macro(j.macro_site.take(), c.macro_site.clone());
            if c.reason != Reason::None {
                j.reason = Some(j.reason.map_or(c.reason, |r| r.min(c.reason)));
            }
            j.notes.extend(c.notes.iter().cloned());
            j.votes += c.votes;
            j.obs.insert(i);
        }
        for g in &e.groups {
            ties.push(
                g.iter()
                    .map(|&m| {
                        let c = &e.claims[m as usize];
                        (
                            (e.files[c.file as usize].path.clone(), c.lo, c.hi),
                            c.pieces.iter().map(mp).collect(),
                        )
                    })
                    .collect(),
            );
        }
        for r in &e.reports {
            reports
                .entry((r.place.as_ref().map(pp), r.severity, r.message.clone(), r.attached))
                .or_default()
                .insert(i);
        }
        for c in &e.changed {
            changed.entry((c.key, pp(&c.place), c.reason.clone())).or_default().insert(i);
        }
        renamed.extend(e.renamed.iter().cloned());
        for s in &e.subjects {
            let j = subjects.entry(s.id).or_insert_with(|| Subj {
                rule: s.rule.clone(),
                role: s.role,
                of: BTreeSet::new(),
                flags: 0,
                display: s.display.clone(),
                seen: BTreeSet::new(),
                flag_seen: Default::default(),
            });
            j.role = j.role.min(s.role);
            j.of.extend(s.of.iter().copied());
            j.flags |= s.flags;
            j.seen.insert(i);
            for bit in 0..4 {
                if s.flags & (1 << bit) != 0 {
                    j.flag_seen[bit].insert(i);
                }
            }
        }
        links.extend(e.links.iter().map(|l| (l.a, l.b)));
    }

    // §5.2 rule 2: selection by iteration.
    let mut selected: BTreeSet<Digest> =
        subjects.iter().filter(|(_, s)| s.role == Role::Seed).map(|(k, _)| *k).collect();
    loop {
        let before = selected.len();
        for &(a, b) in &links {
            if selected.contains(&a) || selected.contains(&b) {
                selected.insert(a);
                selected.insert(b);
            }
        }
        for (id, s) in &subjects {
            if s.role == Role::Follower
                && !s.of.is_empty()
                && s.of.iter().all(|k| selected.contains(k))
            {
                selected.insert(*id);
            }
        }
        if selected.len() == before {
            break;
        }
    }
    // §5.2 rule 3.
    let mut subject_refusals: Vec<(Cause, Option<Digest>, BTreeSet<usize>)> = Vec::new();
    if request.kind == TransformKind::Rename && !subjects.values().any(|s| s.role == Role::Seed) {
        subject_refusals.push((Cause::NoSubject, None, BTreeSet::new()));
    }
    for (id, s) in &subjects {
        if !selected.contains(id) {
            continue;
        }
        for (flag, bit, cause) in [
            (subject_flags::READ_ONLY, 0, Cause::SubjectReadOnly),
            (subject_flags::UNSUPPORTED, 1, Cause::SubjectUnsupported),
            (subject_flags::UNREACHABLE, 3, Cause::SubjectUnreachable),
            (subject_flags::NAME_TAKEN, 2, Cause::NameTaken),
        ] {
            if s.flags & flag != 0 {
                subject_refusals.push((cause, Some(*id), s.flag_seen[bit].clone()));
            }
        }
    }
    for (id, s) in &subjects {
        if s.of.is_empty() {
            continue;
        }
        let n = s.of.iter().filter(|k| selected.contains(k)).count();
        let all = n == s.of.len();
        if (n > 0 && !all) || selected.contains(id) != all {
            subject_refusals.push((Cause::MixedFollower, Some(*id), s.seen.clone()));
        }
    }
    let refused_subjects: BTreeSet<Digest> = subject_refusals.iter().filter_map(|r| r.1).collect();

    // Ties.
    let mut uf = Uf::default();
    if !request.indivisible {
        for g in &ties {
            for m in g {
                uf.0.entry(m.clone()).or_insert_with(|| m.clone());
            }
            for m in &g[1..] {
                let (a, b) = (uf.root(&g[0]).unwrap(), uf.root(m).unwrap());
                if a != b {
                    let (lo, hi) = if a < b { (a, b) } else { (b, a) };
                    uf.0.insert(hi, lo);
                }
            }
        }
    }

    // §5.2's treatment and §5.4's table, site by site.
    let mut lives: Vec<Live> = Vec::new();
    let mut blocking: Vec<(PP, BTreeSet<usize>)> = Vec::new();
    let mut kept = Vec::new();
    let mut unresolved_sites = Vec::new();
    let mut site_refusals: Vec<SiteRef> = Vec::new();
    let mut replaces_left = 0u64;
    let mut roots: BTreeSet<(PP, Vec<MP>)> = BTreeSet::new();
    // A rejected group's root, with the least site of a member with a
    // refusal of its own (absent: a refusal of §5.2 alone).
    let mut rejected_roots: BTreeMap<(PP, Vec<MP>), Option<PP>> = BTreeMap::new();
    // Every site with a replace claim left: its site, everything its
    // replace claims copy, and its index in `lives` if it is live.
    let mut members: Vec<(PP, Vec<PP>, Option<usize>)> = Vec::new();
    let (mut ungrouped, mut ungrouped_rejected) = (0u64, 0u64);
    let (mut n_live, mut n_kept, mut n_unres) = (0u64, 0u64, 0u64);
    for (site, keys) in &claims {
        let obs_of = |class: &BTreeSet<(Stance, Vec<MP>)>| -> BTreeSet<usize> {
            per_observer[site]
                .iter()
                .filter(|(_, mine)| mine.iter().any(|k| class.contains(k)))
                .map(|(i, _)| *i)
                .collect()
        };
        let (mut keep, mut unres, mut refuse, mut replace, mut mixed) =
            (BTreeSet::new(), BTreeSet::new(), BTreeSet::new(), BTreeSet::new(), BTreeSet::new());
        let mut subjects_left: BTreeSet<Digest> = BTreeSet::new();
        let mut edit_keys = Vec::new();
        for (k, j) in keys {
            let n = j.subjects.len();
            let s = j.subjects.iter().filter(|x| selected.contains(x)).count();
            let none = n > 0 && s == 0;
            let some_not = n > 0 && s > 0 && s < n;
            match k.0 {
                Stance::Replace if none => {
                    keep.insert(k.clone());
                }
                Stance::Replace if some_not => {
                    mixed.insert(k.clone());
                    edit_keys.push(k.1.clone());
                }
                Stance::Replace => {
                    replace.insert(k.clone());
                    edit_keys.push(k.1.clone());
                }
                Stance::Refuse if none => continue,
                Stance::Refuse if some_not => {
                    mixed.insert(k.clone());
                }
                Stance::Refuse => {
                    refuse.insert(k.clone());
                }
                Stance::Keep => {
                    keep.insert(k.clone());
                }
                Stance::Unresolved => {
                    unres.insert(k.clone());
                }
            }
            subjects_left.extend(j.subjects.iter().copied());
        }
        let least_subject = subjects_left.first().copied();
        let mac = |class: &BTreeSet<(Stance, Vec<MP>)>| {
            class.iter().fold(None, |m, k| least_macro(m, keys[k].macro_site.clone()))
        };
        let mut rejected = None;
        let mut live = None;
        if !mixed.is_empty() {
            rejected = Some((Cause::MixedSet, Reason::None, mac(&mixed), obs_of(&mixed)));
        } else if !refuse.is_empty() {
            let reason = refuse.iter().filter_map(|k| keys[k].reason).min().unwrap_or(Reason::None);
            rejected = Some((Cause::SiteRefused, reason, mac(&refuse), obs_of(&refuse)));
        } else if replace.len() >= 2 {
            rejected = Some((Cause::Divergent, Reason::None, mac(&replace), obs_of(&replace)));
        } else if let Some(k) = replace.first() {
            if keep.is_empty() {
                live = Some(k.clone());
            } else {
                rejected = Some((
                    Cause::Contested,
                    Reason::None,
                    keys[k].macro_site.clone(),
                    obs_of(&keep),
                ));
            }
        }
        for pieces in &edit_keys {
            replaces_left += 1;
            match uf.root(&(site.clone(), pieces.clone())) {
                Some(r) => {
                    roots.insert(r.clone());
                    if rejected.is_some() {
                        note_root(&mut rejected_roots, r, Some(site.clone()));
                    }
                }
                None => {
                    ungrouped += 1;
                    if rejected.is_some() {
                        ungrouped_rejected += 1;
                    }
                }
            }
        }
        if !edit_keys.is_empty() {
            let copies: Vec<PP> = edit_keys
                .iter()
                .flatten()
                .filter_map(|p| if let MP::C(c) = p { Some(c.clone()) } else { None })
                .collect();
            let live_index = (rejected.is_none() && live.is_some()).then_some(lives.len());
            members.push((site.clone(), copies, live_index));
        }
        let both: BTreeSet<_> = keep.union(&unres).cloned().collect();
        if site.1 < site.2 {
            if live.is_some() {
                if !unres.is_empty() {
                    blocking.push((site.clone(), obs_of(&unres)));
                }
            } else if !both.is_empty() {
                blocking.push((site.clone(), obs_of(&both)));
            }
        }
        if let Some((cause, reason, macro_site, obs)) = rejected {
            site_refusals.push(SiteRef {
                site: site.clone(),
                cause,
                reason,
                macro_site,
                subject: least_subject,
                obs,
                other: None,
            });
        } else if let Some(k) = live {
            n_live += 1;
            let mut j = keys[&k].clone();
            j.notes = j.notes.into_iter().take(8).collect();
            lives.push(Live {
                site: site.clone(),
                pieces: k.1.clone(),
                j,
                least_subject,
                rejected: None,
                gone: false,
                id: (site.clone(), k.1.clone()),
            });
        } else if !keep.is_empty() {
            n_kept += 1;
            kept.push(site.clone());
        } else if !unres.is_empty() {
            n_unres += 1;
            unresolved_sites.push(site.clone());
        }
    }

    // §5.4 geometry: contested by another range.
    for l in &mut lives {
        let hit = blocking
            .iter()
            .filter(|b| b.0 != l.site && same_file_share(&b.0, &l.site))
            .min_by(|a, b| a.0.cmp(&b.0));
        if let Some(b) = hit {
            l.rejected = Some((Cause::Contested, Some(b.0.clone()), Some(b.1.clone())));
        }
    }
    // overlap, all pairs over one set, against every member (landed
    // WP-062 §5).
    let alive: Vec<usize> = (0..lives.len()).filter(|&i| lives[i].alive()).collect();
    let mut other: BTreeMap<usize, PP> = BTreeMap::new();
    let mut note = |i: usize, o: &PP| {
        let e = other.entry(i).or_insert_with(|| o.clone());
        if *o < *e {
            *e = o.clone();
        }
    };
    for &i in &alive {
        for (m, copies, _) in &members {
            if same_file_cross(&lives[i].site, m) {
                note(i, m);
            }
            for c in copies {
                if same_file_cross(&lives[i].site, c) {
                    note(i, c);
                }
            }
        }
        for c in lives[i].copies() {
            for (m, _, _) in &members {
                if same_file_cross(m, &c) {
                    note(i, m);
                }
            }
        }
    }
    for (i, o) in other {
        lives[i].rejected = Some((Cause::Overlap, Some(o), None));
    }
    // cyclic: the closure of "draws on", over every member.
    let draws =
        |a: usize, b: usize| members[a].1.iter().any(|c| same_file_inside(&members[b].0, c));
    for a in 0..members.len() {
        let Some(i) = members[a].2 else { continue };
        if !lives[i].alive() {
            continue;
        }
        let mut seen = BTreeSet::new();
        let mut stack: Vec<usize> = (0..members.len()).filter(|&b| draws(a, b)).collect();
        while let Some(x) = stack.pop() {
            if seen.insert(x) {
                stack.extend((0..members.len()).filter(|&b| draws(x, b)));
            }
        }
        if seen.contains(&a) {
            lives[i].rejected = Some((Cause::Cyclic, None, None));
        }
    }
    // file agreement.
    let differs = |p: &[u8]| all_versions.get(p).is_some_and(|v| v.len() > 1);
    for l in &mut lives {
        if l.alive() && (differs(&l.site.0) || l.copies().iter().any(|c| differs(&c.0))) {
            l.rejected = Some((Cause::FileDiffers, None, None));
        }
    }

    // §5.5: groups, then effect, repeated.
    let any_refusal = !subject_refusals.is_empty();
    let reject_groups = |lives: &mut Vec<Live>,
                         rejected_roots: &mut BTreeMap<(PP, Vec<MP>), Option<PP>>,
                         site_refusals: &Vec<SiteRef>| {
        if request.indivisible {
            if any_refusal
                || !site_refusals.is_empty()
                || lives.iter().any(|l| l.rejected.is_some())
            {
                for l in lives.iter_mut() {
                    l.gone = l.rejected.is_none();
                }
            }
            return;
        }
        for l in lives.iter_mut() {
            let named = l.j.subjects.iter().any(|k| refused_subjects.contains(k));
            if l.rejected.is_some() || (named && !l.gone) {
                match uf.root(&l.id) {
                    Some(r) => {
                        let at = l.rejected.is_some().then(|| l.site.clone());
                        note_root(rejected_roots, r, at);
                    }
                    None if l.rejected.is_none() => l.gone = true,
                    None => {}
                }
            }
        }
        for l in lives.iter_mut() {
            if l.alive() && uf.root(&l.id).is_some_and(|r| rejected_roots.contains_key(&r)) {
                l.gone = true;
            }
        }
    };
    reject_groups(&mut lives, &mut rejected_roots, &site_refusals);
    // Effect: every member a container, the live sites that remain the
    // carriers (landed WP-062 §5).
    let contains = |y: usize, x: usize| x != y && same_file_within(&members[x].0, &members[y].0);
    loop {
        let carrier = |e: usize| members[e].2.is_some_and(|i| lives[i].alive());
        let n = members.len();
        let mut effective: BTreeSet<usize> =
            (0..n).filter(|&x| !(0..n).any(|y| contains(y, x))).collect();
        loop {
            let before = effective.len();
            for &e in &effective.clone() {
                if !carrier(e) {
                    continue;
                }
                for r in &members[e].1 {
                    let inside = |x: usize| same_file_inside(&members[x].0, r);
                    for x in 0..n {
                        if inside(x) && !(0..n).any(|y| inside(y) && contains(y, x)) {
                            effective.insert(x);
                        }
                    }
                }
            }
            if effective.len() == before {
                break;
            }
        }
        let mut buried: BTreeMap<usize, PP> = BTreeMap::new();
        for x in 0..n {
            if effective.contains(&x) {
                continue;
            }
            // The innermost container: of those that contain x, the one
            // that starts last, then the shortest.
            let p = (0..n)
                .filter(|&y| contains(y, x))
                .max_by_key(|&y| (members[y].0.1, std::cmp::Reverse(members[y].0.2)))
                .unwrap();
            for (a, b) in [(x, p), (p, x)] {
                let Some(i) = members[a].2 else { continue };
                if !lives[i].alive() {
                    continue;
                }
                let o = members[b].0.clone();
                let e = buried.entry(i).or_insert_with(|| o.clone());
                if o < *e {
                    *e = o;
                }
            }
        }
        if buried.is_empty() {
            break;
        }
        for (i, o) in buried {
            lives[i].rejected = Some((Cause::Buried, Some(o), None));
        }
        reject_groups(&mut lives, &mut rejected_roots, &site_refusals);
    }

    // §6: the records.
    let union: BTreeSet<Rule> = renamed
        .iter()
        .cloned()
        .chain(
            subjects
                .iter()
                .filter(|(k, _)| selected.contains(*k))
                .filter_map(|(_, s)| s.rule.clone()),
        )
        .collect();
    let rule_conflict = conflicts(&union);
    let no_observer = ok.len() == failed_runs;
    let set_refused = !subject_refusals.is_empty() || no_observer || rule_conflict;
    let finals: Vec<&Live> = lives.iter().filter(|l| l.alive()).collect();
    let accepted = !set_refused && !finals.is_empty();
    let finals: Vec<&Live> = if accepted { finals } else { Vec::new() };

    for l in &lives {
        if let Some((cause, other, obs)) = &l.rejected {
            site_refusals.push(SiteRef {
                site: l.site.clone(),
                cause: *cause,
                reason: Reason::None,
                macro_site: l.j.macro_site.clone(),
                subject: l.least_subject,
                obs: obs.clone().unwrap_or_else(|| l.j.obs.clone()),
                other: other.clone(),
            });
        } else if l.gone && !request.indivisible {
            site_refusals.push(SiteRef {
                site: l.site.clone(),
                cause: Cause::WithGroup,
                reason: Reason::None,
                macro_site: l.j.macro_site.clone(),
                subject: l.least_subject,
                obs: l.j.obs.clone(),
                other: uf.root(&l.id).and_then(|r| rejected_roots[&r].clone()),
            });
        }
    }
    let mut named: BTreeSet<Vec<u8>> = BTreeSet::new();
    for l in &finals {
        named.insert(l.site.0.clone());
        named.extend(l.copies().into_iter().map(|c| c.0));
    }
    named.extend(changed.keys().map(|k| k.1.0.clone()));
    named.extend(kept.iter().map(|k| k.0.clone()));
    named.extend(unresolved_sites.iter().map(|k| k.0.clone()));
    for r in &site_refusals {
        named.insert(r.site.0.clone());
        if let Some(o) = &r.other {
            named.insert(o.0.clone());
        }
    }
    named.extend(reports.keys().filter_map(|k| k.0.as_ref().map(|p| p.0.clone())));
    for f in failures.values() {
        if let Some(Some(s)) = &f.2 {
            named.insert(s.0.clone());
        }
    }
    let files: Vec<FileRef> = named
        .iter()
        .map(|p| {
            let v = &versions[p];
            let size = v.iter().map(|x| x.0).max().unwrap();
            let content = v.iter().filter(|x| x.0 == size).map(|x| x.1).min().unwrap();
            FileRef { path: p.clone(), size, content }
        })
        .collect();
    let index = |p: &[u8]| files.iter().position(|f| f.path == p).unwrap() as u64;
    let place = |p: &PP| Place { file: index(&p.0), lo: p.1, hi: p.2 };

    let edits: Vec<Edit> = finals
        .iter()
        .map(|l| Edit {
            file: index(&l.site.0),
            lo: l.site.1,
            hi: l.site.2,
            pieces: l
                .pieces
                .iter()
                .map(|p| match p {
                    MP::L(b) => Piece::Literal(b.clone()),
                    MP::C(c) => Piece::Copy(place(c)),
                    MP::S(k, t) => Piece::Spelled { key: *k, text: t.clone() },
                })
                .collect(),
            subjects: l.j.subjects.iter().copied().collect(),
            tiers: l.j.tiers,
            macro_site: l.j.macro_site.clone(),
            notes: l.j.notes.iter().cloned().collect(),
            observers: l.j.obs.len() as u64,
            votes: l.j.votes,
            first: first(&l.j.obs, &observers),
        })
        .collect();
    let mut groups: Vec<Vec<u64>> = Vec::new();
    if !request.indivisible {
        let mut by: BTreeMap<(PP, Vec<MP>), Vec<u64>> = BTreeMap::new();
        for (i, l) in finals.iter().enumerate() {
            if let Some(r) = uf.root(&l.id) {
                by.entry(r).or_default().push(i as u64);
            }
        }
        groups = by.into_values().filter(|g| g.len() >= 2).collect();
        groups.sort();
    }
    let exempt: Vec<Exempt> = changed
        .iter()
        .map(|((key, p, reason), obs)| Exempt {
            key: *key,
            place: place(p),
            reason: reason.clone(),
            in_force: finals.iter().any(|l| same_file_share(&l.site, p)),
            observers: obs.len() as u64,
            first: first(obs, &observers),
        })
        .collect();

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
    let mut refusals = Vec::new();
    if no_observer {
        refusals.push(alone(Cause::NoObserver));
    }
    if rule_conflict {
        refusals.push(alone(Cause::RuleConflict));
    }
    if replaces_left == 0 {
        refusals.push(alone(Cause::NoEdit));
    }
    for (cause, subject, obs) in &subject_refusals {
        refusals.push(Refusal {
            subject: *subject,
            observers: obs.len() as u64,
            first: first(obs, &observers),
            ..alone(*cause)
        });
    }
    let mut rejected_sites = BTreeSet::new();
    for r in &site_refusals {
        rejected_sites.insert(r.site.clone());
        refusals.push(Refusal {
            cause: r.cause,
            file: Some(index(&r.site.0)),
            lo: r.site.1,
            hi: r.site.2,
            reason: r.reason,
            subject: r.subject,
            macro_site: r.macro_site.clone(),
            other: r.other.as_ref().map(place),
            observers: r.obs.len() as u64,
            first: first(&r.obs, &observers),
        });
    }
    refusals.sort_by(Refusal::cmp_key);
    let reported: Vec<Reported> = reports
        .iter()
        .map(|((p, severity, message, attached), obs)| Reported {
            report: Report {
                place: p.as_ref().map(place),
                severity: *severity,
                message: message.clone(),
                attached: *attached,
            },
            observers: obs.len() as u64,
            first: first(obs, &observers),
        })
        .collect();
    let failed: Vec<Failed> = failures
        .values()
        .map(|(obs, f, subject)| Failed {
            failure: Failure {
                refusal: f.refusal.as_ref().map(|r| HostRefusal {
                    subject: subject.clone().flatten().as_ref().map(place),
                    ..r.clone()
                }),
                ..f.clone()
            },
            observers: obs.len() as u64,
            first: first(obs, &observers),
        })
        .collect();

    // §6.3.
    let mut counters = shard_counters;
    let mut put = |n: &str, v: u64| {
        counters.insert(n.to_owned(), v);
    };
    put("reduce.shards", ok.len() as u64);
    put("reduce.unparsed", unparsed.len() as u64);
    put("reduce.failed_runs", failed_runs as u64);
    put("reduce.claims", n_claims);
    for (i, n) in ["replace", "keep", "refuse", "unresolved"].iter().enumerate() {
        put(&format!("reduce.claims.{n}"), by_stance[i]);
    }
    let set_subjects: Vec<Subject> = subjects
        .iter()
        .filter(|(k, _)| selected.contains(*k))
        .map(|(id, s)| Subject {
            id: *id,
            rule: s.rule.clone(),
            role: s.role,
            of: if s.role == Role::Follower { s.of.iter().copied().collect() } else { Vec::new() },
            flags: s.flags,
            display: s.display.clone(),
        })
        .collect();
    put("reduce.subjects", subjects.len() as u64);
    put("reduce.subjects.selected", set_subjects.len() as u64);
    put("reduce.rules", union.len() as u64);
    put("reduce.sites", claims.len() as u64);
    put("reduce.sites.live", n_live);
    put("reduce.sites.kept", n_kept);
    put("reduce.sites.unresolved", n_unres);
    put("reduce.sites.rejected", rejected_sites.len() as u64);
    let (g, gr) = if request.indivisible {
        let any = replaces_left > 0;
        (u64::from(any), u64::from(any && (set_refused || edits.is_empty())))
    } else {
        let live_ungrouped_rejected =
            lives.iter().filter(|l| uf.root(&l.id).is_none() && !l.alive()).count() as u64;
        let rr = if set_refused { roots.len() } else { rejected_roots.len() } as u64;
        let ur = if set_refused { ungrouped } else { ungrouped_rejected + live_ungrouped_rejected };
        (roots.len() as u64 + ungrouped, rr + ur)
    };
    put("reduce.groups", g);
    put("reduce.groups.rejected", gr);
    put("reduce.edits", edits.len() as u64);
    put("reduce.edits.shared", edits.iter().filter(|e| e.observers > 1).count() as u64);
    put("reduce.edits.max_observers", edits.iter().map(|e| e.observers).max().unwrap_or(0));
    let nested = finals
        .iter()
        .enumerate()
        .filter(|(i, l)| {
            finals.iter().enumerate().any(|(j, m)| j != *i && same_file_within(&l.site, &m.site))
        })
        .count();
    put("reduce.edits.nested", nested as u64);
    put("reduce.files", edits.iter().map(|e| e.file).collect::<BTreeSet<_>>().len() as u64);
    for (sev, n) in
        [(Severity::Note, "note"), (Severity::Review, "review"), (Severity::Error, "error")]
    {
        put(
            &format!("reduce.reports.{n}"),
            reported.iter().filter(|r| r.report.severity == sev).count() as u64,
        );
    }
    put("reduce.changed", exempt.len() as u64);
    put("reduce.changed.in_force", exempt.iter().filter(|c| c.in_force).count() as u64);

    // The set's rules: the renamed rules whose match no other has.
    let rules: Vec<Rule> = renamed
        .iter()
        .filter(|a| {
            !renamed.iter().any(|b| {
                b != *a
                    && (a.parent, a.kind, &a.name, a.disc) == (b.parent, b.kind, &b.name, b.disc)
            })
        })
        .cloned()
        .collect();
    let rules = slip_model::keys::RuleTable::new(rules).unwrap().rules().to_vec();

    EditSet {
        scheme: SCHEME,
        plan,
        round,
        transform: request_id,
        basis: basis_id(&basis),
        state: if accepted { SetState::Accepted } else { SetState::Refused },
        files: files.clone(),
        subjects: set_subjects,
        rules,
        edits,
        groups,
        changed: exempt,
        kept: kept.iter().map(place).collect(),
        unresolved: unresolved_sites.iter().map(place).collect(),
        refusals,
        reports: reported,
        failures: failed,
        unparsed,
        counters: counters.into_iter().map(|(name, value)| Counter { name, value }).collect(),
    }
}
