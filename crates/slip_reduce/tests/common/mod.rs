//! Builders of map shards and a `Shards` over a vector, for the tests of
//! `slip_reduce` (`docs/wp/WP-062.md` "Tests"). Every shard is written by
//! hand: no Kiln.

#![allow(dead_code, reason = "each test binary uses a part of the builders")]
#![allow(clippy::type_complexity, clippy::too_many_lines, reason = "test builders")]

pub mod model;

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use slip_model::container::{Reader, Writer};
use slip_model::edits::{
    Changed, Claim, Edits, Failure, FailureKind, HostRefusal, Link, MacroSite, Piece, Place,
    Reason, Report, Role, SCHEME, Severity, SourcePlace, Stance, Subject, tiers,
};
use slip_model::editset::{Cause, EditSet, Refusal};
use slip_model::formats::content_id;
use slip_model::hash::content_digest;
use slip_model::keys::{KeyKind, Name, Parent, Rule};
use slip_model::transform::{Ownership, Rename, Script, TargetKind, Transform, TransformKind};
use slip_model::{Counter, Digest, FileRef, Outcome, Phase, Shard, Status};
use slip_reduce::{Options, ReduceError, Reduced, Shards, reduce};

pub fn d(n: u8) -> Digest {
    Digest([n; 32])
}

/// The id of the request every test shard names.
pub const REQUEST: Digest = Digest([0x44; 32]);
pub const PLAN: Digest = Digest([0x50; 32]);

/// The observer at plan index `i`: a digest whose order is not the
/// index's, so that nothing can lean on it.
pub fn observer(i: usize) -> Digest {
    content_digest(format!("observer {i}").as_bytes())
}

/// The size every file has unless a test says otherwise.
pub const SIZE: u64 = 1000;

/// The content every file has unless a test says otherwise.
pub fn content(path: &[u8]) -> Digest {
    content_digest(path)
}

fn ownership() -> Ownership {
    Ownership { owned: vec![b"/s".to_vec()], readonly: vec![b"/s/ro".to_vec()] }
}

/// A programmable transform's request: owns `/s` but `/s/ro`, divisible.
pub fn script() -> Transform {
    Transform {
        kind: TransformKind::Script,
        ownership: ownership(),
        indivisible: false,
        rename: None,
        script: Some(Script {
            id: d(0x5c),
            file: b"/s/t.cc".to_vec(),
            options: Vec::new(),
            steps: 1 << 30,
            bytes: 1 << 30,
            depth: 512,
        }),
    }
}

/// A rename's request: indivisible.
pub fn rename() -> Transform {
    Transform {
        kind: TransformKind::Rename,
        ownership: ownership(),
        indivisible: true,
        rename: Some(Rename {
            target: vec![b"n".to_vec(), b"f".to_vec()],
            kind: TargetKind::Any,
            new_name: b"g".to_vec(),
        }),
        script: None,
    }
}

/// A piece by path.
#[derive(Clone, Debug)]
pub enum P {
    L(Vec<u8>),
    C(Vec<u8>, u64, u64),
    S(Digest, Vec<u8>),
    /// An identifier piece (WP-132).
    I(Vec<u8>, Option<Digest>),
}

pub fn lit(b: &str) -> P {
    P::L(b.as_bytes().to_vec())
}

pub fn copy(path: &str, lo: u64, hi: u64) -> P {
    P::C(path.as_bytes().to_vec(), lo, hi)
}

pub fn spelled(key: Digest, text: &str) -> P {
    P::S(key, text.as_bytes().to_vec())
}

/// A claim by path.
#[derive(Clone, Debug)]
pub struct C {
    pub path: Vec<u8>,
    pub lo: u64,
    pub hi: u64,
    pub stance: Stance,
    pub pieces: Vec<P>,
    pub subjects: Vec<Digest>,
    pub reason: Reason,
    pub tiers: u64,
    pub macro_site: Option<MacroSite>,
    pub votes: u64,
    pub notes: Vec<Vec<u8>>,
}

impl C {
    pub fn new(path: &str, lo: u64, hi: u64, stance: Stance) -> C {
        C {
            path: path.as_bytes().to_vec(),
            lo,
            hi,
            stance,
            pieces: Vec::new(),
            subjects: Vec::new(),
            reason: if stance == Stance::Refuse { Reason::Crossing } else { Reason::None },
            tiers: tiers::T0,
            macro_site: None,
            votes: 1,
            notes: Vec::new(),
        }
    }

    pub fn subjects(mut self, s: &[Digest]) -> C {
        let set: BTreeSet<Digest> = s.iter().copied().collect();
        self.subjects = set.into_iter().collect();
        self
    }

    pub fn notes(mut self, n: &[&str]) -> C {
        let set: BTreeSet<Vec<u8>> = n.iter().map(|x| x.as_bytes().to_vec()).collect();
        self.notes = set.into_iter().collect();
        self
    }

    pub fn reason(mut self, r: Reason) -> C {
        self.reason = r;
        self
    }

    pub fn in_macro(mut self, name_start: u64, name: &str) -> C {
        self.tiers = tiers::T0 | tiers::T1;
        self.macro_site = Some(MacroSite { name_start, name: name.as_bytes().to_vec() });
        self
    }

    pub fn votes(mut self, v: u64) -> C {
        self.votes = v;
        self
    }
}

pub fn replace(path: &str, lo: u64, hi: u64, pieces: Vec<P>) -> C {
    C { pieces, ..C::new(path, lo, hi, Stance::Replace) }
}

pub fn keep(path: &str, lo: u64, hi: u64) -> C {
    C::new(path, lo, hi, Stance::Keep)
}

pub fn refuse(path: &str, lo: u64, hi: u64) -> C {
    C::new(path, lo, hi, Stance::Refuse)
}

pub fn unresolved(path: &str, lo: u64, hi: u64) -> C {
    C::new(path, lo, hi, Stance::Unresolved)
}

pub fn function_rule(from: &str, to: &str) -> Rule {
    Rule {
        parent: Parent::Root,
        kind: KeyKind::Function,
        name: Name::Identifier(from.as_bytes().to_vec()),
        disc: None,
        to_parent: Parent::Root,
        to_name: Name::Identifier(to.as_bytes().to_vec()),
    }
}

pub fn namespace_rule(from: &str, to: &str) -> Rule {
    Rule { kind: KeyKind::Namespace, ..function_rule(from, to) }
}

/// A map shard, written by path; [`ShardB::build`] sorts and indexes.
#[derive(Clone, Debug)]
pub struct ShardB {
    pub observer: Digest,
    pub round: u64,
    pub phase: Phase,
    pub status: Status,
    pub files: BTreeMap<Vec<u8>, FileRef>,
    /// `None`: the shard has no `inputs`; else the extra entries beyond
    /// `files`.
    pub inputs: Option<BTreeMap<Vec<u8>, FileRef>>,
    pub subjects: BTreeMap<Digest, Subject>,
    pub links: BTreeSet<(Digest, Digest)>,
    pub claims: Vec<C>,
    pub groups: Vec<Vec<usize>>,
    pub reports: Vec<(Option<(Vec<u8>, u64, u64)>, Severity, Vec<u8>, bool)>,
    pub changed: Vec<(Digest, Vec<u8>, u64, u64, Vec<u8>)>,
    pub renamed: BTreeSet<Rule>,
    pub failure: Option<(FailureKind, String, String, Option<(Vec<u8>, u64, u64)>)>,
    pub counters: BTreeMap<String, u64>,
    pub edits: bool,
    pub scheme: u64,
    pub transform: Digest,
}

impl ShardB {
    /// The shard of plan index `i`.
    pub fn new(i: usize) -> ShardB {
        ShardB {
            observer: observer(i),
            round: 0,
            phase: Phase::Map,
            status: Status::Ok,
            files: BTreeMap::new(),
            inputs: Some(BTreeMap::new()),
            subjects: BTreeMap::new(),
            links: BTreeSet::new(),
            claims: Vec::new(),
            groups: Vec::new(),
            reports: Vec::new(),
            changed: Vec::new(),
            renamed: BTreeSet::new(),
            failure: None,
            counters: BTreeMap::new(),
            edits: true,
            scheme: SCHEME,
            transform: REQUEST,
        }
    }

    fn touch(&mut self, path: &[u8]) {
        self.files.entry(path.to_vec()).or_insert_with(|| FileRef {
            path: path.to_vec(),
            size: SIZE,
            content: content(path),
        });
    }

    /// Gives `path` a size and content of its own in this shard.
    pub fn file(mut self, path: &str, size: u64, content: Digest) -> ShardB {
        self.files.insert(
            path.as_bytes().to_vec(),
            FileRef { path: path.as_bytes().to_vec(), size, content },
        );
        self
    }

    /// An entry of `inputs` only.
    pub fn input(mut self, path: &str, size: u64, content: Digest) -> ShardB {
        let f = FileRef { path: path.as_bytes().to_vec(), size, content };
        self.inputs.get_or_insert_with(BTreeMap::new).insert(f.path.clone(), f);
        self
    }

    pub fn no_inputs(mut self) -> ShardB {
        self.inputs = None;
        self
    }

    pub fn subject(
        mut self,
        id: Digest,
        role: Role,
        of: &[Digest],
        flags: u64,
        display: &str,
    ) -> ShardB {
        let of: BTreeSet<Digest> = of.iter().copied().collect();
        self.subjects.insert(
            id,
            Subject {
                id,
                rule: None,
                role,
                of: of.into_iter().collect(),
                flags,
                display: display.to_owned(),
            },
        );
        self
    }

    pub fn seed(self, id: Digest) -> ShardB {
        self.subject(id, Role::Seed, &[], 0, "seed")
    }

    pub fn candidate(self, id: Digest) -> ShardB {
        self.subject(id, Role::Candidate, &[], 0, "candidate")
    }

    pub fn follower(self, id: Digest, of: &[Digest]) -> ShardB {
        self.subject(id, Role::Follower, of, 0, "follower")
    }

    pub fn flags(mut self, id: Digest, flags: u64) -> ShardB {
        self.subjects.get_mut(&id).expect("flags: no such subject").flags = flags;
        self
    }

    pub fn rule(mut self, id: Digest, rule: Rule) -> ShardB {
        self.subjects.get_mut(&id).expect("rule: no such subject").rule = Some(rule);
        self
    }

    pub fn link(mut self, a: Digest, b: Digest) -> ShardB {
        self.links.insert(if a < b { (a, b) } else { (b, a) });
        self
    }

    pub fn claim(mut self, c: C) -> ShardB {
        self.claims.push(c);
        self
    }

    /// Ties the claims of these builder indices (the order of `claim`).
    pub fn group(mut self, members: &[usize]) -> ShardB {
        self.groups.push(members.to_vec());
        self
    }

    pub fn report(
        mut self,
        place: Option<(&str, u64, u64)>,
        severity: Severity,
        message: &str,
    ) -> ShardB {
        self.reports.push((
            place.map(|(p, lo, hi)| (p.as_bytes().to_vec(), lo, hi)),
            severity,
            message.as_bytes().to_vec(),
            false,
        ));
        self
    }

    pub fn changed(mut self, key: Digest, path: &str, lo: u64, hi: u64, reason: &str) -> ShardB {
        self.changed.push((key, path.as_bytes().to_vec(), lo, hi, reason.as_bytes().to_vec()));
        self
    }

    pub fn renamed(mut self, rule: Rule) -> ShardB {
        self.renamed.insert(rule);
        self
    }

    pub fn failed(mut self, message: &str, subject: Option<(&str, u64, u64)>) -> ShardB {
        self.failure = Some((
            FailureKind::Uncaught,
            message.to_owned(),
            format!("rendered by {}", self.observer),
            subject.map(|(p, lo, hi)| (p.as_bytes().to_vec(), lo, hi)),
        ));
        self
    }

    pub fn counter(mut self, name: &str, value: u64) -> ShardB {
        self.counters.insert(name.to_owned(), value);
        self
    }

    pub fn status(mut self, status: Status) -> ShardB {
        self.status = status;
        self
    }

    pub fn build(mut self) -> Shard {
        let mut paths: Vec<Vec<u8>> = Vec::new();
        for c in &self.claims {
            paths.push(c.path.clone());
            for p in &c.pieces {
                if let P::C(path, ..) = p {
                    paths.push(path.clone());
                }
            }
        }
        for r in &self.reports {
            if let Some(p) = &r.0 {
                paths.push(p.0.clone());
            }
        }
        for c in &self.changed {
            paths.push(c.1.clone());
        }
        if let Some((.., Some(s))) = &self.failure {
            paths.push(s.0.clone());
        }
        for p in paths {
            self.touch(&p);
        }
        // §4.7: a failed run names at most the file of its refusal's subject.
        if let Some((.., subject)) = &self.failure {
            let keep = subject.as_ref().map(|s| s.0.clone());
            self.files.retain(|p, _| Some(p) == keep.as_ref());
        }
        let files: Vec<FileRef> = self.files.values().cloned().collect();
        let index = |path: &[u8]| files.iter().position(|f| f.path == path).unwrap() as u64;
        let place =
            |(path, lo, hi): &(Vec<u8>, u64, u64)| Place { file: index(path), lo: *lo, hi: *hi };

        let mut claims: Vec<(Claim, usize)> = self
            .claims
            .iter()
            .enumerate()
            .map(|(k, c)| {
                let pieces = c
                    .pieces
                    .iter()
                    .map(|p| match p {
                        P::L(b) => Piece::Literal(b.clone()),
                        P::C(path, lo, hi) => {
                            Piece::Copy(Place { file: index(path), lo: *lo, hi: *hi })
                        }
                        P::S(key, text) => Piece::Spelled { key: *key, text: text.clone() },
                        P::I(text, key) => Piece::Identifier { text: text.clone(), key: *key },
                    })
                    .collect();
                let claim = Claim {
                    file: index(&c.path),
                    lo: c.lo,
                    hi: c.hi,
                    stance: c.stance,
                    pieces,
                    subjects: c.subjects.clone(),
                    reason: c.reason,
                    tiers: c.tiers,
                    macro_site: c.macro_site.clone(),
                    votes: c.votes,
                    notes: c.notes.clone(),
                };
                (claim, k)
            })
            .collect();
        claims.sort_by(|a, b| a.0.cmp_key(&b.0));
        let mut at = vec![0u64; claims.len()];
        for (pos, (_, k)) in claims.iter().enumerate() {
            at[*k] = pos as u64;
        }
        let mut groups: Vec<Vec<u64>> = self
            .groups
            .iter()
            .map(|g| {
                let mut m: Vec<u64> = g.iter().map(|&k| at[k]).collect();
                m.sort_unstable();
                m
            })
            .collect();
        groups.sort();
        let mut reports: Vec<Report> = self
            .reports
            .iter()
            .map(|(p, severity, message, attached)| Report {
                place: p.as_ref().map(place),
                severity: *severity,
                message: message.clone(),
                attached: *attached,
            })
            .collect();
        reports.sort();
        reports.dedup();
        let mut changed: Vec<Changed> = self
            .changed
            .iter()
            .map(|(key, path, lo, hi, reason)| Changed {
                key: *key,
                place: place(&(path.clone(), *lo, *hi)),
                reason: reason.clone(),
            })
            .collect();
        changed.sort();
        changed.dedup();
        let renamed = slip_model::keys::RuleTable::new(self.renamed.iter().cloned().collect())
            .expect("renamed rules make a table")
            .rules()
            .to_vec();

        let failure = self.failure.as_ref().map(|(kind, message, rendered, subject)| Failure {
            kind: *kind,
            message: message.clone(),
            place: Some(SourcePlace { file: b"/s/t.cc".to_vec(), line: 1, column: 1 }),
            rendered: rendered.clone(),
            refusal: Some(HostRefusal {
                function: "slip::replace".to_owned(),
                errc: "not_editable".to_owned(),
                message: "refused".to_owned(),
                subject: subject.as_ref().map(place),
            }),
        });
        let failed = failure.is_some();
        let edits = Edits {
            scheme: self.scheme,
            transform: self.transform,
            failure,
            files: files.clone(),
            subjects: if failed { Vec::new() } else { self.subjects.values().cloned().collect() },
            links: if failed {
                Vec::new()
            } else {
                self.links.iter().map(|&(a, b)| Link { a, b }).collect()
            },
            claims: if failed { Vec::new() } else { claims.into_iter().map(|c| c.0).collect() },
            groups: if failed { Vec::new() } else { groups },
            reports: if failed { Vec::new() } else { reports },
            changed: if failed { Vec::new() } else { changed },
            renamed: if failed { Vec::new() } else { renamed },
        };
        let inputs = self.inputs.as_ref().map(|extra| {
            let mut all: BTreeMap<Vec<u8>, FileRef> = extra.clone();
            for f in &files {
                all.entry(f.path.clone()).or_insert_with(|| f.clone());
            }
            all.into_values().collect()
        });
        let ok = self.status == Status::Ok;
        Shard {
            observer: self.observer,
            phase: self.phase,
            round: self.round,
            outcome: Outcome {
                status: self.status,
                errors: 0,
                warnings: 0,
                message: String::new(),
            },
            inputs: if ok { inputs } else { None },
            counters: self
                .counters
                .iter()
                .map(|(name, &value)| Counter { name: name.clone(), value })
                .collect(),
            digests: None,
            verdict: None,
            edits: if ok && self.edits { Some(edits) } else { None },
            binding: None,
            rule_use: None,
            spellings: None,
        }
    }
}

/// One entry of a [`Store`].
#[derive(Clone, Debug)]
pub enum Entry {
    /// No shard: unparsed.
    Missing,
    /// `open` fails.
    Broken(String),
    /// The bytes of a stored shard: valid or not, reduce decodes them.
    Bytes(Vec<u8>),
}

/// `Shards` over a vector, in plan order.
#[derive(Debug)]
pub struct Store {
    pub observers: Vec<Digest>,
    pub entries: Vec<Entry>,
    /// Delays each `open` by a pseudo-random time drawn from this seed.
    pub delay: Option<u64>,
    /// Counts the calls of `open`.
    pub opens: AtomicU64,
}

pub fn id_of(s: &Shard) -> Digest {
    content_id(&s.to_bytes()).expect("a shard's bytes have a content id")
}

/// The bytes of a shard with section `id`'s payload replaced by what
/// `f` makes of it (`None`: the section removed): bytes `Shard::to_bytes`
/// cannot write, for the tests of what reduce refuses.
pub fn rewrite(bytes: &[u8], id: u64, f: impl FnOnce(&[u8]) -> Option<Vec<u8>>) -> Vec<u8> {
    let r = Reader::open(bytes, Shard::KIND, Shard::VERSION).expect("the shard reads");
    let ids: Vec<u64> = r.ids().collect();
    let mut f = Some(f);
    let mut w = Writer::new(Shard::KIND, Shard::VERSION);
    for i in ids {
        let payload = r.section(i).expect("a listed section").to_vec();
        if i == id {
            if let Some(p) = (f.take().expect("one section of an id"))(&payload) {
                w.section(i, &p);
            }
        } else {
            w.section(i, &payload);
        }
    }
    w.finish()
}

impl Store {
    /// The shards at plan indices `0..`; each shard's observer is its own.
    pub fn new(shards: Vec<Shard>) -> Store {
        let observers = shards.iter().map(|s| s.observer).collect();
        let entries = shards.into_iter().map(|s| Entry::Bytes(s.to_bytes())).collect();
        Store { observers, entries, delay: None, opens: AtomicU64::new(0) }
    }

    pub fn of(shards: Vec<ShardB>) -> Store {
        Store::new(shards.into_iter().map(ShardB::build).collect())
    }

    /// Adds an observer at the end whose stored shard is `bytes`.
    pub fn bytes(mut self, observer: Digest, bytes: Vec<u8>) -> Store {
        self.observers.push(observer);
        self.entries.push(Entry::Bytes(bytes));
        self
    }

    /// Adds an observer at the end with no shard.
    pub fn missing(mut self, i: usize) -> Store {
        self.observers.push(observer(i));
        self.entries.push(Entry::Missing);
        self
    }
}

/// A small, fixed generator (SplitMix64): the tests draw from it with a
/// seed they print.
#[derive(Clone, Debug)]
pub struct Rng(pub u64);

impl Rng {
    pub fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// Uniform in `0..n` (`n > 0`).
    pub fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }

    pub fn chance(&mut self, percent: u64) -> bool {
        self.below(100) < percent
    }
}

impl Shards for Store {
    fn open(&self, index: usize) -> Result<Option<Vec<u8>>, String> {
        self.opens.fetch_add(1, Ordering::Relaxed);
        if let Some(seed) = self.delay {
            let mut r = Rng(seed ^ (index as u64).wrapping_mul(0x2545_f491_4f6c_dd1d));
            std::thread::sleep(std::time::Duration::from_micros(r.below(400)));
        }
        match &self.entries[index] {
            Entry::Missing => Ok(None),
            Entry::Broken(m) => Err(m.clone()),
            Entry::Bytes(b) => Ok(Some(b.clone())),
        }
    }

    fn len(&self) -> usize {
        self.entries.len()
    }

    fn observer(&self, index: usize) -> Digest {
        self.observers[index]
    }
}

/// A scratch directory of its own for one call of reduce, under Cargo's
/// temporary directory for integration tests.
pub fn scratch(name: &str) -> PathBuf {
    static CALL: AtomicU64 = AtomicU64::new(0);
    let call = CALL.fetch_add(1, Ordering::Relaxed);
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join("slip-reduce")
        .join(format!("{name}-{}-{call}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Whether `dir` is empty: reduce left no temporary file.
pub fn assert_clean(dir: &std::path::Path) {
    let left: Vec<_> = std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().path()).collect();
    assert!(left.is_empty(), "reduce left temporary files: {left:?}");
}

pub fn options(name: &str) -> Options {
    Options { fan_in: 64, threads: 1, scratch: scratch(name) }
}

/// Reduces with the default options; the set validates (§6.1).
pub fn run(request: &Transform, store: &Store, name: &str) -> Result<Reduced, ReduceError> {
    let o = options(name);
    let r = reduce(request, REQUEST, PLAN, 0, store, &o);
    assert_clean(&o.scratch);
    if let Ok(r) = &r {
        r.edits.validate().expect("every set reduce makes validates (§6.1)");
    }
    r
}

pub fn ok(request: &Transform, shards: Vec<ShardB>, name: &str) -> EditSet {
    run(request, &Store::of(shards), name).expect("reduce succeeds").edits
}

/// The causes of a set's refusals, in order.
pub fn causes(set: &EditSet) -> Vec<Cause> {
    set.refusals.iter().map(|r| r.cause).collect()
}

/// The refusal of `cause` at `(path, lo, hi)`.
pub fn refusal_at<'a>(set: &'a EditSet, cause: Cause, path: &str, lo: u64, hi: u64) -> &'a Refusal {
    set.refusals
        .iter()
        .find(|r| {
            r.cause == cause
                && r.file.is_some_and(|f| set.files[f as usize].path == path.as_bytes())
                && (r.lo, r.hi) == (lo, hi)
        })
        .unwrap_or_else(|| panic!("no {cause:?} at {path} [{lo}, {hi}): {:#?}", set.refusals))
}

/// The ranges of the set's edits, as `(path, lo, hi)`.
pub fn edits(set: &EditSet) -> Vec<(String, u64, u64)> {
    set.edits
        .iter()
        .map(|e| {
            (String::from_utf8_lossy(&set.files[e.file as usize].path).into_owned(), e.lo, e.hi)
        })
        .collect()
}

pub fn e(path: &str, lo: u64, hi: u64) -> (String, u64, u64) {
    (path.to_owned(), lo, hi)
}

pub fn counter(set: &EditSet, name: &str) -> u64 {
    set.counters
        .iter()
        .find(|c| c.name == name)
        .map_or_else(|| panic!("no counter {name}"), |c| c.value)
}
