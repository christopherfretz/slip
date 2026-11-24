//! Shards, runs and overlays written by hand for `agreement.rs`,
//! `spelling.rs` and `conclude.rs` (WP-070): a [`Run`] gives the report
//! of a transform's run and shards that are its own, which a test then
//! breaks one field at a time.

#![allow(dead_code, reason = "each test file uses a part")]

use slip_model::agreement::{Binding, RuleUse, Use};
use slip_model::declared::Declared;
use slip_model::digests::{Digests, SCHEME, tu_digest};
use slip_model::edits::{
    Failure, FailureKind, Piece, Reason, Report as EditReport, Severity, tiers,
};
use slip_model::editset::{Cause, Edit, EditSet, Failed as SetFailed, Refusal, Reported, SetState};
use slip_model::formats::content_id;
use slip_model::keys::{KeyKind, Name, Parent, Rule, RuleTable};
use slip_model::overlay::{Content, Obligation, OverlayFile};
use slip_model::result::{Category, Residue, State, TransformResult};
use slip_model::spellings::{Failed, Span, Spellings};
use slip_model::verdict::{Finding, FindingKind, Verdict, VerdictKind};
use slip_model::{Counter, Digest, FileRef, Outcome, Overlay, Phase, Shard, Status};
use slip_verify::conclude::Concluded;
use slip_verify::run::{Header, MapShard, Report};

pub fn d(n: u8) -> Digest {
    Digest([n; 32])
}

/// A key distinct from every `d(n)`.
pub fn key(n: u8) -> Digest {
    let mut b = [0u8; 32];
    b[0] = 0x10;
    b[1] = n;
    Digest(b)
}

pub const PLAN: u8 = 0x01;
/// The run's round, which is also the round of its pre shards
/// (`spec/edits.md` §8.3).
pub const ROUND: u64 = 3;
pub const DECLARED: u8 = 0x02;
pub const OVERLAY: u8 = 0x07;

/// The map shard every verdict of [`Run::verdict`] names as its `pre`,
/// with `status` (`spec/verify.md` §9.3, WP-114).
pub const fn map_of(status: Status) -> Option<MapShard> {
    Some(MapShard { status, id: Digest([0x70; 32]) })
}

/// An `ok` map shard, the one the verdicts name.
pub const MAP_OK: Option<MapShard> = map_of(Status::Ok);

/// The six counters of `spec/verify.md` §7.2 by kind and
/// `verify.findings`, sorted.
pub fn counters(by_kind: [u64; 6]) -> Vec<Counter> {
    let names = [
        "verify.findings.changed",
        "verify.findings.removed",
        "verify.findings.added",
        "verify.findings.indeterminate",
        "verify.findings.derived_changed",
        "verify.findings.derived_indeterminate",
    ];
    let mut v: Vec<Counter> = names
        .iter()
        .zip(by_kind)
        .map(|(n, value)| Counter { name: (*n).to_owned(), value })
        .collect();
    v.push(Counter { name: "verify.findings".to_owned(), value: by_kind.iter().sum() });
    v.sort();
    v
}

pub fn file(path: &str, size: u64, n: u8) -> FileRef {
    FileRef { path: path.as_bytes().to_vec(), size, content: d(n) }
}

/// A rule of `kind` renaming the global `from` to `to`.
pub fn rule(kind: KeyKind, from: &str, to: &str) -> Rule {
    Rule {
        parent: Parent::Root,
        kind,
        name: Name::Identifier(from.as_bytes().to_vec()),
        disc: None,
        to_parent: Parent::Root,
        to_name: Name::Identifier(to.as_bytes().to_vec()),
    }
}

/// The declared changes of `rules`, sorted as a table.
pub fn declared(rules: Vec<Rule>) -> Declared {
    let t = RuleTable::new(rules).unwrap();
    Declared::new(t.rules().to_vec(), vec![], vec![], vec![]).unwrap()
}

/// Two overlay files, `/s/a.h` with obligations 0 and 1, `/s/b.h` with
/// obligation 2; and `/s/c.h`, unchanged (`post == pre`), with none.
/// An overlay of [`set`]`(SetFacts::default())` (its `edits` is that
/// set's content id) over three files, with three obligations.
pub fn overlay() -> Overlay {
    Overlay {
        edits: content_id(&set(SetFacts::default()).to_bytes()).unwrap(),
        files: vec![
            OverlayFile {
                path: b"/s/a.h".to_vec(),
                pre: Content { size: 10, content: d(0x51) },
                post: Content { size: 12, content: d(0x61) },
            },
            OverlayFile {
                path: b"/s/b.h".to_vec(),
                pre: Content { size: 20, content: d(0x52) },
                post: Content { size: 21, content: d(0x62) },
            },
            OverlayFile {
                path: b"/s/c.h".to_vec(),
                pre: Content { size: 30, content: d(0x53) },
                post: Content { size: 30, content: d(0x53) },
            },
        ],
        spellings: vec![
            Obligation { file: 0, lo: 1, hi: 2, key: key(0x40) },
            Obligation { file: 0, lo: 5, hi: 9, key: key(0x41) },
            Obligation { file: 1, lo: 0, hi: 3, key: key(0x42) },
        ],
    }
}

/// An overlay of `set` without obligations: the files of [`overlay`].
pub fn overlay_for(set: &EditSet) -> Overlay {
    Overlay { edits: content_id(&set.to_bytes()).unwrap(), spellings: Vec::new(), ..overlay() }
}

/// The `inputs` entry of overlay file `i` as read through the overlay.
pub fn post_input(o: &Overlay, i: usize) -> FileRef {
    let f = &o.files[i];
    FileRef { path: f.path.clone(), size: f.post.size, content: f.post.content }
}

/// The `spellings` section of a shard over `o` that read `inputs`, with
/// obligations `held` holding and `failed` failing (index, found, name).
pub fn section(o: &Overlay, inputs: &[FileRef], held: &[u64], failed: &[(u64, u8)]) -> Spellings {
    let seen = o
        .spellings
        .iter()
        .filter(|ob| {
            let p = &o.files[ob.file as usize].path;
            inputs.iter().any(|f| &f.path == p)
        })
        .count() as u64;
    let mut spans: Vec<Span> = Vec::new();
    for &i in held {
        match spans.last_mut() {
            Some(s) if s.first + s.count == i => s.count += 1,
            _ => spans.push(Span { first: i, count: 1 }),
        }
    }
    Spellings {
        seen,
        held: spans,
        failed: failed
            .iter()
            .map(|&(index, n)| Failed { index, found: Some(key(n)), name: format!("other{n}") })
            .collect(),
    }
}

/// A transform's run: its declared changes, its overlay (or a tree image
/// with none), and the shards of its observers.
pub struct Run {
    pub declared: Declared,
    pub overlay: Option<Overlay>,
    pub plan_observers: u64,
}

impl Run {
    pub fn new(rules: Vec<Rule>, overlay: Option<Overlay>) -> Run {
        Run { declared: declared(rules), overlay, plan_observers: 10 }
    }

    pub fn header(&self) -> Header {
        Header {
            plan: d(PLAN),
            round: ROUND,
            declared: d(DECLARED),
            rules: self.declared.rules().id(),
            image: vec![],
            limit: 64,
            locate: 16,
            plan_observers: self.plan_observers,
            run: b"/store/runs/0003.run".to_vec(),
        }
    }

    /// The report before any `add`: the overlay given, the pre round set.
    pub fn report(&self) -> Report {
        let mut r = Report::new(self.header(), &self.declared);
        if let Some(o) = &self.overlay {
            r.overlay(d(OVERLAY), o);
        }
        r.pre_round(ROUND);
        r
    }

    pub fn binding(&self) -> Binding {
        Binding {
            overlay: self.overlay.as_ref().map(|_| d(OVERLAY)),
            declared: d(DECLARED),
            pre: ROUND,
        }
    }

    fn base(&self, obs: u8, status: Status, message: &str) -> Shard {
        let ok = status == Status::Ok;
        Shard {
            observer: d(obs),
            phase: Phase::Verify,
            round: ROUND,
            outcome: Outcome {
                status,
                errors: u64::from(!ok),
                warnings: 0,
                message: message.to_owned(),
            },
            inputs: if status == Status::Failed {
                None
            } else {
                Some(vec![file("/s/x.cpp", 1, 0x99)])
            },
            counters: vec![],
            digests: None,
            verdict: None,
            edits: None,
            binding: if status == Status::Failed { None } else { Some(self.binding()) },
            rule_use: None,
            spellings: None,
            names: None,
            negatives: None,
            found: None,
        }
    }

    /// An `ok` shard of this run with `verdict`, reading `/s/x.cpp` and
    /// the overlay files `read` (through the overlay); a `rule_use` with
    /// `uses` if the run has rules; `spellings` over `read` with nothing
    /// held or failed if the overlay has obligations.
    pub fn ok(&self, obs: u8, verdict: Verdict, read: &[usize], uses: &[Use]) -> Shard {
        let mut by_kind = [0u64; 6];
        for f in &verdict.findings {
            by_kind[f.kind as usize] += 1;
        }
        let mut s = self.base(obs, Status::Ok, "");
        s.counters = counters(by_kind);
        s.digests = Some(Digests { scheme: SCHEME, tu: tu_digest(&[]), groups: vec![] });
        s.names = Some(slip_model::names::Names::default());
        s.verdict = Some(verdict);
        let mut inputs = s.inputs.take().unwrap();
        if let Some(o) = &self.overlay {
            inputs.extend(read.iter().map(|&i| post_input(o, i)));
        }
        inputs.sort_by(|a, b| a.path.cmp(&b.path));
        if !self.declared.rules().is_empty() {
            s.rule_use = Some(RuleUse {
                rules: self.declared.rules().id(),
                uses: uses.to_vec(),
                kept: vec![],
            });
        }
        if let Some(o) = self.overlay.as_ref().filter(|o| !o.spellings.is_empty()) {
            s.spellings = Some(section(o, &inputs, &[], &[]));
        }
        s.inputs = Some(inputs);
        s.validate().expect("a valid shard");
        s
    }

    pub fn verified(&self, obs: u8) -> Shard {
        self.ok(obs, self.verdict(VerdictKind::Verified), &[], &[])
    }

    pub fn mismatch(&self, obs: u8) -> Shard {
        let mut v = self.verdict(VerdictKind::Mismatch);
        v.total = 1;
        v.findings = vec![Finding {
            kind: FindingKind::Changed,
            key: Some(key(1)),
            file: b"/s/a.h".to_vec(),
            pre: Some(d(0xa1)),
            post: Some(d(0xb1)),
            name: "f".to_owned(),
            location: None,
        }];
        self.ok(obs, v, &[], &[])
    }

    pub fn unverifiable(&self, obs: u8, reason: &str) -> Shard {
        let mut v = self.verdict(VerdictKind::Unverifiable);
        reason.clone_into(&mut v.reason);
        v.pre = Digest([0; 32]);
        self.ok(obs, v, &[], &[])
    }

    /// A shard whose parse ended with `status` (not `ok`).
    pub fn unparsed(&self, obs: u8, status: Status, message: &str) -> Shard {
        let mut s = self.base(obs, status, message);
        if status != Status::Failed {
            s.counters = vec![Counter { name: "pp.tokens".to_owned(), value: 3 }];
        }
        s.validate().expect("a valid shard");
        s
    }

    pub fn verdict(&self, kind: VerdictKind) -> Verdict {
        Verdict {
            kind,
            pre: d(0x70),
            rules: self.declared.rules().id(),
            reason: String::new(),
            only_pre: 0,
            only_post: 0,
            used: vec![],
            total: 0,
            findings: vec![],
        }
    }
}

/// A use of rule `rule`.
pub fn used(rule: u64, via: u64, direct: u64) -> Use {
    Use { rule, via, direct, kept_via: 0 }
}

/// A use of namespace rule `rule` with its `kept_via` (§9.2).
pub fn used_kept(rule: u64, via: u64, direct: u64, kept_via: u64) -> Use {
    Use { rule, via, direct, kept_via }
}

/// Compares `actual` with `tests/golden/<dir>/<name>`; `SLIP_BLESS=1`
/// writes it instead (as `tests/run.rs` does).
pub fn golden(dir: &str, name: &str, actual: &str) {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden").join(dir).join(name);
    if std::env::var_os("SLIP_BLESS").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, actual).unwrap();
        eprintln!("blessed {}", path.display());
        return;
    }
    let expected = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{}: {e}; run with SLIP_BLESS=1 to create", path.display()));
    assert_eq!(actual, expected, "{name} differs from its golden; SLIP_BLESS=1 rewrites it");
}

// ---------------------------------------------------------------------------
// Edit sets and results (spec/edits.md §6, §8.5)

/// What an edit set has beyond one edit: the reasons of
/// `verified_with_residue` and the unparsed observers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SetFacts {
    pub refused: bool,
    pub refusal: bool,
    /// Observers of the one failure (0: none), at most 3.
    pub failed_runs: u64,
    pub error_report: bool,
    pub unparsed: u8,
}

/// An edit set of the run (plan `d(PLAN)`, round `ROUND`) with `f`.
pub fn set(f: SetFacts) -> EditSet {
    let first = |n: u64| (0..n).map(|i| d(0xa0 + i as u8)).collect::<Vec<_>>();
    let mut s = EditSet {
        scheme: 1,
        plan: d(PLAN),
        round: ROUND,
        transform: d(0x22),
        basis: d(0x23),
        state: SetState::Accepted,
        files: vec![file("/s/a.h", 10, 0x51)],
        subjects: vec![],
        rules: vec![],
        edits: vec![Edit {
            file: 0,
            lo: 1,
            hi: 2,
            pieces: vec![Piece::Literal(b"x".to_vec())],
            subjects: vec![],
            tiers: tiers::T0,
            macro_site: None,
            notes: vec![],
            observers: 1,
            votes: 1,
            first: vec![],
        }],
        groups: vec![],
        changed: vec![],
        kept: vec![],
        unresolved: vec![],
        refusals: vec![],
        reports: vec![],
        failures: vec![],
        unparsed: (0..f.unparsed).map(|i| d(0xb0 + i)).collect(),
        counters: vec![],
    };
    let refusal = |cause, file, subject| Refusal {
        cause,
        file,
        lo: if file.is_some() { 5 } else { 0 },
        hi: if file.is_some() { 6 } else { 0 },
        reason: Reason::None,
        subject,
        macro_site: None,
        other: None,
        observers: 0,
        first: vec![],
    };
    if f.refusal {
        s.refusals.push(refusal(Cause::MixedSet, Some(0), Some(d(0x12))));
    }
    if f.refused {
        s.state = SetState::Refused;
        s.edits.clear();
        s.refusals.push(refusal(Cause::NoEdit, None, None));
        if !f.refusal {
            // No record names the file now.
            s.files.clear();
        }
    }
    if f.error_report {
        s.reports.push(Reported {
            report: EditReport {
                place: None,
                severity: Severity::Error,
                message: b"e".to_vec(),
                attached: false,
            },
            observers: 1,
            first: first(1),
        });
    }
    if f.failed_runs != 0 {
        s.failures.push(SetFailed {
            failure: Failure {
                kind: FailureKind::Uncaught,
                message: "boom".to_owned(),
                place: None,
                rendered: String::new(),
                refusal: None,
            },
            observers: f.failed_runs,
            first: first(f.failed_runs),
        });
    }
    s.validate().expect("a valid edit set");
    s
}

/// The result conclude makes of `c` for `set`, with the fields the
/// caller fills.
pub fn result_of(
    c: &Concluded,
    set: &EditSet,
    partial: bool,
    allow_unparsed: bool,
    residue: &[Residue],
) -> TransformResult {
    let refused = c.state == State::Refused;
    TransformResult {
        scheme: 1,
        plan: set.plan,
        round: set.round,
        transform: set.transform,
        edits: content_id(&set.to_bytes()).unwrap(),
        declared: (!refused).then_some(d(DECLARED)),
        overlay: (!refused).then_some(d(OVERLAY)),
        state: c.state,
        partial,
        allow_unparsed,
        verdict: c.verdict,
        tally: c.tally,
        agreement: c.agreement.clone(),
        unused: c.unused.clone(),
        spellings: c.spellings.clone(),
        residue: residue.to_vec(),
        exempt: vec![],
    }
}

pub fn residue(category: Category, lo: u64) -> Residue {
    Residue { category, file: b"/s/z.h".to_vec(), lo, hi: lo + 1, line: 1, column: lo + 1 }
}

/// A small deterministic generator (xorshift64*), seeded from
/// `SLIP_VERIFY_SEED` when it is set.
pub struct Rng(pub u64);

impl Rng {
    pub fn from_env(default: u64) -> Rng {
        let seed =
            std::env::var("SLIP_VERIFY_SEED").ok().and_then(|s| s.parse().ok()).unwrap_or(default);
        eprintln!("seed {seed} (SLIP_VERIFY_SEED overrides)");
        Rng(seed.max(1))
    }

    pub fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    pub fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }

    pub fn chance(&mut self, one_in: u64) -> bool {
        self.below(one_in) == 0
    }
}
