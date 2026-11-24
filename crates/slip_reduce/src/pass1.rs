//! The first pass (`spec/edits.md` §5.1, §5.3, §5.6): each shard checked,
//! its subjects, links, rules and group ties joined in memory, its claims,
//! reports, `changed` entries, failures and files collapsed into the run's
//! records.

use std::collections::{BTreeMap, BTreeSet};

use slip_model::edits::{Edits, Piece, Place, Stance};
use slip_model::keys::Rule;
use slip_model::transform::{Transform, is_file_path};
use slip_model::{Digest, Phase, Shard, Status};

use crate::Shards;
use crate::groups::Ties;
use crate::meter::{Held, Meter};
use crate::records::{
    ClaimVal, Entry, FailVal, KPiece, Key, NOTES_MAX, RefusalK, Sample, Site, Val, identity,
    variant,
};
use crate::runs::BasisEntry;
use crate::subjects::SubjectJoin;

/// What the first pass reads of the request and the shards.
pub(crate) struct Ctx<'a> {
    pub request: &'a Transform,
    pub request_id: Digest,
    pub round: u64,
    pub shards: &'a dyn Shards,
    pub meter: &'a Meter,
}

/// Counts of the first pass (§6.3).
#[derive(Clone, Debug, Default)]
pub(crate) struct Stats {
    pub ok: u64,
    pub failed_runs: u64,
    pub claims: u64,
    /// By stance: replace, keep, refuse, unresolved.
    pub by_stance: [u64; 4],
}

/// What is held in memory across shards (§5.6): joined subjects, links,
/// rules and group ties, and the shard counters' sums.
#[derive(Clone, Debug, Default)]
pub(crate) struct Joined {
    pub subjects: BTreeMap<Digest, SubjectJoin>,
    pub links: BTreeSet<(Digest, Digest)>,
    pub rules: BTreeSet<Rule>,
    pub ties: Ties,
    pub counters: BTreeMap<String, u64>,
    pub stats: Stats,
}

/// §6.3: a shard counter named `api.*` or `edits.*` is summed; for
/// `api.peak_bytes` the largest is kept.
fn add_counter(counters: &mut BTreeMap<String, u64>, name: &str, value: u64) {
    if !(name.starts_with("api.") || name.starts_with("edits.")) {
        return;
    }
    let slot = counters.entry(name.to_owned()).or_insert(0);
    *slot = if name == "api.peak_bytes" { (*slot).max(value) } else { slot.saturating_add(value) };
}

impl Joined {
    pub fn join(&mut self, other: Joined) {
        for (id, s) in other.subjects {
            match self.subjects.get_mut(&id) {
                Some(mine) => mine.join(&s),
                None => {
                    self.subjects.insert(id, s);
                }
            }
        }
        self.links.extend(other.links);
        self.rules.extend(other.rules);
        self.ties.join(other.ties);
        for (name, v) in other.counters {
            add_counter(&mut self.counters, &name, v);
        }
        let (a, b) = (&mut self.stats, other.stats);
        a.ok += b.ok;
        a.failed_runs += b.failed_runs;
        a.claims += b.claims;
        for (x, y) in a.by_stance.iter_mut().zip(b.by_stance) {
            *x += y;
        }
    }

    /// The least observer index whose subject's rule disagrees with an
    /// earlier observer's, and the subject (§5.2 rule 1).
    pub fn rule_conflict(&self) -> Option<(u64, Digest)> {
        self.subjects.iter().filter_map(|(id, s)| s.conflict.map(|c| (c, *id))).min()
    }
}

/// The site of a place of `edits`, by path.
fn site_of(edits: &Edits, p: &Place) -> Site {
    // `Shard::validate` made every place's file an index of `files`.
    Site { path: edits.files[p.file as usize].path.clone(), lo: p.lo, hi: p.hi }
}

fn kpieces(edits: &Edits, pieces: &[Piece]) -> Vec<KPiece> {
    pieces
        .iter()
        .map(|p| match p {
            Piece::Literal(b) => KPiece::Literal(b.clone()),
            Piece::Copy(place) => KPiece::Copy(site_of(edits, place)),
            Piece::Spelled { key, text } => KPiece::Spelled { key: *key, text: text.clone() },
            Piece::Identifier { text, key } => KPiece::Identifier { text: text.clone(), key: *key },
        })
        .collect()
}

/// A shard read and checked: its basis entry, and the shard if it is `ok`,
/// counted as held (`reduce.peak.sections`) while it is.
pub(crate) struct Read<'a> {
    pub entry: BasisEntry,
    pub shard: Option<Shard>,
    pub _held: Option<Held<'a>>,
}

/// Decodes the bytes a store gave for a shard, and their content id: the
/// trailer, the digest of every byte before it, which the container's
/// reader has checked (`spec/core.md` §5). One read gives both, so the
/// id is of the very bytes decoded (landed WP-062 §1).
pub(crate) fn decode(bytes: &[u8]) -> Result<(Shard, Digest), String> {
    let shard = Shard::from_bytes(bytes).map_err(|e| format!("the shard is not valid: {e}"))?;
    let mut id = [0u8; 32];
    // A file the reader accepts ends in its 32-byte trailer.
    id.copy_from_slice(&bytes[bytes.len() - 32..]);
    Ok((shard, Digest(id)))
}

/// Opens and checks the shard at `index` (§5.1). `Err` is the message of
/// an `Input` error naming the shard's observer.
pub(crate) fn read<'a>(ctx: &Ctx<'a>, index: usize) -> Result<Read<'a>, String> {
    let observer = ctx.shards.observer(index);
    let unparsed = Read { entry: BasisEntry { observer, shard: None }, shard: None, _held: None };
    let Some(bytes) =
        ctx.shards.open(index).map_err(|e| format!("the shard cannot be read: {e}"))?
    else {
        return Ok(unparsed);
    };
    let (shard, id) = decode(&bytes)?;
    let held = ctx.meter.sections.hold();
    drop(bytes);
    if shard.observer != observer {
        return Err("the shard is of another observer".into());
    }
    if shard.phase != Phase::Map {
        return Err("the shard is not of phase map".into());
    }
    if shard.round != ctx.round {
        return Err(format!("the shard is of round {}, not {}", shard.round, ctx.round));
    }
    if shard.outcome.status != Status::Ok {
        return Ok(unparsed);
    }
    let Some(edits) = &shard.edits else {
        return Err("an ok shard has no edits section".into());
    };
    if edits.transform != ctx.request_id {
        return Err("the edits section is of another transform request".into());
    }
    // The shard's reader checks that `inputs` is sorted, not that its paths
    // are canonical (landed WP-062 §2): reduce checks them, as it checks
    // `edits.files`, before comparing the two.
    for (j, f) in shard.inputs.iter().flatten().enumerate() {
        if !is_file_path(&f.path) {
            return Err(format!("inputs[{j}] is not a canonical absolute path of a file"));
        }
    }
    for (j, f) in edits.files.iter().enumerate() {
        if !is_file_path(&f.path) {
            return Err(format!("edits.files[{j}] is not a canonical absolute path of a file"));
        }
        if let Some(inputs) = &shard.inputs {
            match inputs.binary_search_by(|i| i.path.cmp(&f.path)) {
                Ok(k) if inputs[k] == *f => {}
                Ok(_) => return Err(format!("edits.files[{j}] disagrees with the shard's inputs")),
                Err(_) => return Err(format!("edits.files[{j}] is not in the shard's inputs")),
            }
        }
    }
    // §5.1: a rule that no rule table may hold (`spec/verify.md` §2.1,
    // `Rule::refusal`) is not an observer's to give.
    for (j, r) in edits.renamed.iter().enumerate() {
        if let Some(what) = r.refusal() {
            return Err(format!("edits.renamed[{j}] is a rule no table may hold: {what}"));
        }
    }
    for (j, s) in edits.subjects.iter().enumerate() {
        if let Some(what) = s.rule.as_ref().and_then(Rule::refusal) {
            return Err(format!("edits.subjects[{j}].rule is a rule no table may hold: {what}"));
        }
    }
    let is_subject = |k: &Digest| edits.subjects.binary_search_by(|s| s.id.cmp(k)).is_ok();
    for (j, l) in edits.links.iter().enumerate() {
        if !is_subject(&l.a) || !is_subject(&l.b) {
            return Err(format!("edits.links[{j}] names a key that is no subject of the shard"));
        }
    }
    for (j, c) in edits.claims.iter().enumerate() {
        if !c.subjects.iter().all(is_subject) {
            return Err(format!("edits.claims[{j}] names a key that is no subject of the shard"));
        }
        if c.stance == Stance::Replace
            && !ctx.request.ownership.is_owned(&edits.files[c.file as usize].path)
        {
            return Err(format!(
                "edits.claims[{j}] replaces bytes of a file the request does not own"
            ));
        }
    }
    Ok(Read { entry: BasisEntry { observer, shard: Some(id) }, shard: Some(shard), _held: Some(held) })
}

fn put(records: &mut BTreeMap<Key, Val>, key: Key, val: Val) -> Result<(), String> {
    if let Some(v) = records.get_mut(&key) {
        v.join(&val)
    } else {
        records.insert(key, val);
        Ok(())
    }
}

/// Folds an `ok` shard, checked by [`read`], into a run.
#[allow(clippy::too_many_lines)]
pub(crate) fn fold(
    ctx: &Ctx<'_>,
    index: u64,
    shard: &Shard,
    records: &mut BTreeMap<Key, Val>,
    joined: &mut Joined,
) -> Result<(), String> {
    let Some(edits) = &shard.edits else { return Ok(()) };
    joined.stats.ok += 1;
    for c in &shard.counters {
        add_counter(&mut joined.counters, &c.name, c.value);
    }
    for f in &edits.files {
        put(
            records,
            Key::File { path: f.path.clone(), size: f.size, content: f.content },
            Val::File,
        )?;
    }
    if let Some(f) = &edits.failure {
        joined.stats.failed_runs += 1;
        let key = Key::Failure {
            kind: f.kind,
            message: f.message.clone(),
            place: f.place.as_ref().map(|p| (p.file.clone(), p.line, p.column)),
        };
        let refusal = f.refusal.as_ref().map(|r| RefusalK {
            function: r.function.clone(),
            errc: r.errc.clone(),
            message: r.message.clone(),
            subject: r.subject.as_ref().map(|p| site_of(edits, p)),
        });
        let val = Val::Failure(FailVal {
            sample: Sample::one(index),
            from: index,
            rendered: f.rendered.clone(),
            refusal,
        });
        return put(records, key, val);
    }
    // Claims are sorted by site first (§4.7): the claims of one site are
    // adjacent.
    let mut at = 0;
    while at < edits.claims.len() {
        let c0 = &edits.claims[at];
        let end = at
            + edits.claims[at..]
                .iter()
                .take_while(|c| (c.file, c.lo, c.hi) == (c0.file, c0.lo, c0.hi))
                .count();
        let site = Site { path: edits.files[c0.file as usize].path.clone(), lo: c0.lo, hi: c0.hi };
        let alone = end - at == 1;
        let mut variants = Vec::new();
        for c in &edits.claims[at..end] {
            joined.stats.claims += 1;
            joined.stats.by_stance[c.stance as usize] += 1;
            let pieces = kpieces(edits, &c.pieces);
            if !alone {
                variants.push(variant(&site, c.stance, &pieces));
            }
            let key =
                Key::Claim { site: site.clone(), entry: Entry::Claim { stance: c.stance, pieces } };
            let val = Val::Claim(ClaimVal {
                subjects: c.subjects.clone(),
                tiers: c.tiers,
                macro_site: c.macro_site.clone(),
                reason: c.reason,
                notes: c.notes.iter().take(NOTES_MAX).cloned().collect(),
                votes: c.votes,
                sample: Sample::one(index),
                single: if alone { Sample::one(index) } else { Sample::default() },
            });
            put(records, key, val)?;
        }
        if !alone {
            variants.sort_unstable();
            variants.dedup();
            put(
                records,
                Key::Claim { site, entry: Entry::Observed(variants) },
                Val::Sample(Sample::one(index)),
            )?;
        }
        at = end;
    }
    for r in &edits.reports {
        let key = Key::Report {
            place: r.place.as_ref().map(|p| site_of(edits, p)),
            severity: r.severity,
            message: r.message.clone(),
            attached: r.attached,
        };
        put(records, key, Val::Sample(Sample::one(index)))?;
    }
    for c in &edits.changed {
        let key =
            Key::Changed { key: c.key, place: site_of(edits, &c.place), reason: c.reason.clone() };
        put(records, key, Val::Sample(Sample::one(index)))?;
    }
    joined.rules.extend(edits.renamed.iter().cloned());
    for s in &edits.subjects {
        let mine = SubjectJoin::of_record(s, index);
        match joined.subjects.get_mut(&s.id) {
            Some(j) => j.join(&mine),
            None => {
                joined.subjects.insert(s.id, mine);
            }
        }
    }
    joined.links.extend(edits.links.iter().map(|l| (l.a, l.b)));
    if !ctx.request.indivisible {
        for g in &edits.groups {
            let members: Vec<Digest> = g
                .iter()
                .map(|&m| {
                    let c = &edits.claims[m as usize];
                    let site = Site {
                        path: edits.files[c.file as usize].path.clone(),
                        lo: c.lo,
                        hi: c.hi,
                    };
                    identity(&site, &kpieces(edits, &c.pieces))
                })
                .collect();
            joined.ties.tie(&members);
        }
    }
    Ok(())
}
