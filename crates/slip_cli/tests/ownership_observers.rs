//! WP-146 stage 0 (`spec/landed/verify/WP-146.md` §2.2, §3.4, §6): every
//! shape S1 to S23 of the proposal as a run over two observers (three for
//! S12; both orders for S1 and S10), with real shards made from sources by
//! the map task and the verify task, and the run's report over them
//! (`spec/verify.md` §7, §9), as the review of WP-140 ran its probes.
//!
//! Each test pins what the run says **today** (before stages 2 and 3):
//! per observer its verdict and its rule use, then the run's verdict, its
//! stale pairs and its unused rules, as one text (`summary`). Where today's
//! answer is a wrong `verified` (the proposal's **WV**), a second test,
//! ignored as this repository marks pending tests (`#[ignore = "pending:
//! ..."]`), states the answer the shape must have once stage 3 (a later
//! package: the fold and the verdict) is built: the run not verified. The
//! limits of §3.6 (S15, S16, S17, S22) are pinned with today's answer and
//! the reason, and have no pending test here: stage 3 does not turn them.
//!
//! Throughout, as in the proposal: `relocate old => :: fresh` (the rule
//! `fresh -> old`, kind namespace) unless a test says otherwise; files
//! under `dep/` are read-only; `u1` is the observer `X` (it reads the
//! entity as owned), `u0` the observer `Y` (it reads it as a dependency's).
//!
//! A parse changes the process's current directory: every test that
//! parses holds [`LOCK`], and every path is absolute.

mod common;

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};

use slip_cli::image::{Image, RootMap};
use slip_cli::verify_job::{Params, verify};
use slip_cli::worker::MapTask;
use slip_exec::store::{Access, Store};
use slip_exec::worker::{Job, Task};
use slip_model::declared::Declared;
use slip_model::formats::content_id;
use slip_model::keys::{KeyKind, Name, Parent, Rule, RuleTable};
use slip_model::transform::Ownership;
use slip_model::{Command, Digest, FileRef, Language, Observer, Origin, Phase, Shard};
use slip_verify::run::{Header, MapShard, Report, RunVerdict};

static LOCK: Mutex<()> = Mutex::new(());

/// A subject tree `s/`, a post tree `post-tree/` and a store, in a
/// directory of their own that is removed on drop; the lock is held while
/// it lives.
struct Tree {
    base: PathBuf,
    subject: PathBuf,
    post: PathBuf,
    store: Store,
    _lock: MutexGuard<'static, ()>,
}

impl Drop for Tree {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.base);
    }
}

fn bytes(p: &Path) -> Vec<u8> {
    p.to_str().unwrap().as_bytes().to_vec()
}

fn write(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

impl Tree {
    /// `pre` in both trees, then `post` over the post tree.
    fn new(name: &str, pre: &[(&str, &str)], post: &[(&str, &str)]) -> Tree {
        static N: AtomicUsize = AtomicUsize::new(0);
        let lock = LOCK.lock().unwrap_or_else(PoisonError::into_inner);
        let n = N.fetch_add(1, Ordering::SeqCst);
        let base = common::work_root().join(format!("wp146-{name}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        let base = std::fs::canonicalize(&base).unwrap();
        let subject = base.join("s");
        let post_dir = base.join("post-tree");
        std::fs::create_dir_all(&subject).unwrap();
        std::fs::create_dir_all(&post_dir).unwrap();
        for (rel, text) in pre {
            write(&subject.join(rel), text);
            write(&post_dir.join(rel), text);
        }
        for (rel, text) in post {
            write(&post_dir.join(rel), text);
        }
        let store = Store::open(&base.join("store"), Access::Shared).unwrap();
        Tree { base, subject, post: post_dir, store, _lock: lock }
    }

    fn observer_of(&self, main: &str, flags: &[&str]) -> Observer {
        let mut argv: Vec<Vec<u8>> = vec![b"c++".to_vec(), b"-std=c++20".to_vec()];
        argv.extend(flags.iter().map(|f| f.as_bytes().to_vec()));
        argv.push(bytes(&self.subject.join(main)));
        let command = Command { directory: bytes(&self.subject), argv, env: Vec::new() };
        Observer {
            id: command.observer_id(),
            command,
            language: Language::Cxx,
            main: FileRef {
                path: bytes(&self.subject.join(main)),
                size: 0,
                content: Digest([0; 32]),
            },
            cost_hint: 1,
            origins: vec![Origin { configuration: 0, entry: 0 }],
        }
    }

    fn job(&self, observer: &Observer, phase: Phase, round: u64) -> Job {
        Job {
            store: bytes(self.store.root()),
            plan: Digest([7; 32]),
            phase,
            round,
            observer: observer.clone(),
            transform: None,
            post: None,
        }
    }

    /// The map shard of `observer`, made by the map task and published.
    fn map(&self, observer: &Observer) -> Shard {
        let job = self.job(observer, Phase::Map, 0);
        let shard = MapTask::new().run(&job).unwrap();
        self.store
            .publish_shard(&job.plan, Phase::Map, 0, &observer.id, &shard.to_bytes())
            .unwrap();
        shard
    }

    fn image(&self) -> Image {
        Image::new(vec![RootMap { subject: bytes(&self.subject), post: bytes(&self.post) }])
            .unwrap()
    }

    /// The run's files: the subject owned, `readonly` of it read-only.
    fn files(&self, readonly: &[&str]) -> Ownership {
        let readonly = readonly.iter().map(|r| bytes(&self.subject.join(r))).collect();
        Ownership { owned: vec![bytes(&self.subject)], readonly, rebuilt: Vec::new() }
    }

    fn params(&self, declared: Declared) -> Params {
        let declared_id = content_id(&declared.to_bytes()).unwrap();
        Params { pre: 0, declared, declared_id, image: self.image(), limit: 64, locate: 16 }
    }

    /// The verify task's shard of `observer`, round 1, validated.
    fn verify_with(&self, observer: &Observer, params: &Params) -> Shard {
        let job = self.job(observer, Phase::Verify, 1);
        let shard = verify(&self.store, &job, params).unwrap();
        shard.validate().unwrap();
        shard
    }

    /// The map shard as the run reads it: its status and content id.
    fn stored_map(&self, observer: &Observer) -> MapShard {
        let bytes =
            self.store.read_shard(&Digest([7; 32]), Phase::Map, 0, &observer.id).unwrap().unwrap();
        let shard = Shard::from_bytes(&bytes).unwrap();
        MapShard::Read { status: shard.outcome.status, id: content_id(&bytes).unwrap() }
    }
}

fn ident(s: &str) -> Name {
    Name::Identifier(s.as_bytes().to_vec())
}

/// `relocate old => :: fresh`: the rule `fresh -> old`, kind namespace.
fn old_to_fresh() -> Rule {
    Rule {
        parent: Parent::Root,
        kind: KeyKind::Namespace,
        name: ident("fresh"),
        disc: None,
        to_parent: Parent::Root,
        to_name: ident("old"),
    }
}

/// The declared changes of `slip declare` text `text` (`spec/verify.md`
/// §2.3), resolved over the records of the one-file snippets `pre` and
/// `post` (every file owned there: the keys of the entities named do not
/// depend on ownership in them). For rules on a function itself and for
/// `kept` lines, whose keys hold a signature.
fn declared_text(text: &str, pre: &str, post: &str) -> Declared {
    use slip_cli::declare::{finish, read, resolve};
    let lines = read(text).unwrap();
    let empty = RuleTable::empty();
    let resolved = resolve(&lines, &records_of(pre, &empty), &records_of(post, &empty))
        .unwrap_or_else(|e| panic!("{text}: {e}"));
    let mapped = records_of(post, &resolved.rules);
    finish(&lines, resolved, &mapped).unwrap_or_else(|e| panic!("{text}: {e}"))
}

fn records_of(source: &str, rules: &RuleTable) -> Vec<slip_digest::records::Record> {
    let files = [("main.cpp", source)];
    let snippet = slip_kiln::snippet::Snippet {
        files: &files,
        main: "main.cpp",
        include: &[""],
        std: "c++20",
    };
    let extra = slip_kiln::snippet::Extra { args: &[], target: None };
    let (r, errors) = slip_kiln::snippet::parse_with_tables(&snippet, &extra, |p| {
        let paths: Vec<Vec<u8>> =
            p.sm.file_ids()
                .map(|f| p.path(f).map(|s: &str| s.as_bytes().to_vec()).unwrap_or_default())
                .collect();
        let (ast, sm) = (p.ast, p.sm);
        let r = std::thread::scope(|scope| {
            std::thread::Builder::new()
                .stack_size(slip_digest::STACK_SIZE)
                .spawn_scoped(scope, || {
                    let input = slip_digest::Input {
                        ast,
                        sm,
                        subject_path: &|file| paths.get(file.index()).cloned().unwrap_or_default(),
                    };
                    slip_digest::records::records(&input, rules, slip_digest::ReadOnly::NO_FILE)
                })
                .expect("a thread for the walk")
                .join()
                .unwrap_or_else(|payload| std::panic::resume_unwind(payload))
        });
        (r, p.errors)
    })
    .expect("the snippet parses");
    assert_eq!(errors, 0, "{source}");
    r.expect("records")
}

/// One shape: the trees, the declared changes (`None`: `relocate old =>
/// :: fresh`), the read-only files and the observers in plan order (a
/// main file and its flags).
struct Shape<'a> {
    name: &'a str,
    pre: &'a [(&'a str, &'a str)],
    post: &'a [(&'a str, &'a str)],
    declared: Option<Declared>,
    readonly: &'a [&'a str],
    observers: &'a [(&'a str, &'a [&'a str])],
}

/// What the run said, one line per observer in plan order (its main
/// file, its verdict, its findings by kind and name, its rule use) and
/// one for the run (verdict, stale pairs, unused rules).
struct Ran {
    summary: String,
    verdict: RunVerdict,
    /// The run's report as text (`Report::to_text`).
    text: String,
}

fn run(shape: Shape<'_>) -> Ran {
    let tree = Tree::new(shape.name, shape.pre, shape.post);
    let declared = shape
        .declared
        .unwrap_or_else(|| Declared::new(vec![old_to_fresh()], vec![], vec![], vec![]).unwrap());
    let params = tree.params(declared.with_files(Some(tree.files(shape.readonly))));
    let header = Header {
        plan: Digest([7; 32]),
        round: 1,
        declared: params.declared_id,
        rules: params.declared.rules_id(),
        image: vec![(bytes(&tree.subject), bytes(&tree.post))],
        limit: params.limit as u64,
        locate: params.locate as u64,
        plan_observers: shape.observers.len() as u64,
        run: b"runs/0001.run".to_vec(),
    };
    let mut report = Report::new(header, &params.declared);
    let mut summary = String::new();
    for (main, flags) in shape.observers {
        let o = tree.observer_of(main, flags);
        tree.map(&o);
        let v = tree.verify_with(&o, &params);
        let verdict = v.verdict.as_ref().expect("a verdict");
        let mut line = format!("{main}: {:?}", verdict.kind);
        if !verdict.reason.is_empty() {
            let _ = write!(line, " ({})", verdict.reason);
        }
        for f in &verdict.findings {
            let _ = write!(line, " [{:?} {}]", f.kind, f.name);
        }
        if let Some(u) = &v.rule_use {
            for x in &u.uses {
                let _ = write!(
                    line,
                    " use {} via {} direct {} kept_via {}",
                    x.rule, x.via, x.direct, x.kept_via
                );
            }
        }
        summary.push_str(line.trim_end());
        summary.push('\n');
        report.add(&o.id, &o.main.path, Some(&v), Some(tree.stored_map(&o)));
    }
    let verdict = report.verdict();
    let _ = writeln!(
        summary,
        "run: {verdict:?} stale {} unused {}",
        report.stale().len(),
        report.unused().len()
    );
    let text = report.to_text();
    Ran { summary, verdict, text }
}

/// Pins today's summary of `shape`.
fn pin(shape: Shape<'_>, want: &str) {
    let ran = run(shape);
    assert_eq!(ran.summary, want, "\n{}", ran.summary);
}

/// The answer stage 3 must give a shape whose lists join (the package's
/// "Done when", WP-146 ruling 2 F4): the run is not verified, and its
/// report has an `ownership` finding: one line of `Report::to_text` that
/// says `ownership`, names the entity by `key` (its key as the report
/// names a key: the entity's name) and names each of `files` (the files
/// of the two entries, or where both are one file, that file and the two
/// observers' main files). The text's format for it is stage 3's; this is
/// its specification.
fn ownership_finding(shape: Shape<'_>, key: &str, files: &[&str]) {
    let ran = run(shape);
    assert_eq!(ran.verdict, RunVerdict::NotVerified, "\n{}", ran.summary);
    let found = ran.text.lines().any(|l| {
        l.to_lowercase().contains("ownership")
            && l.contains(key)
            && files.iter().all(|f| l.contains(f))
    });
    assert!(found, "no ownership finding for {key} naming {files:?}:\n{}", ran.text);
}

const RO: &[&str] = &["dep"];

// ---------------------------------------------------------------------------
// S1: the hook pair (the package's). dep/d.h declares old::hook; u1
// defines it, does not include d.h, and moves it with `old`; u0 includes
// d.h and calls the dependency's old::hook, unchanged. §3.4: WV today;
// stage 3: a finding (T), the run not verified. Both orders (§3.1
// "Attacks": the answer does not depend on the order of observers).

const S1_D: &str = "namespace old { int hook(int); }\n";
const S1_U1: &str = "namespace old { int hook(int x) { return x; } }\n";
const S1_U1_POST: &str = "namespace fresh { int hook(int x) { return x; } }\n";
const S1_U0: &str = "#include \"dep/d.h\"\nint r = old::hook(1);\n";

fn s1(observers: &'static [(&'static str, &'static [&'static str])]) -> Shape<'static> {
    Shape {
        name: "s1",
        pre: &[("dep/d.h", S1_D), ("u1.cpp", S1_U1), ("u0.cpp", S1_U0)],
        post: &[("u1.cpp", S1_U1_POST)],
        declared: None,
        readonly: RO,
        observers,
    }
}

const U0_U1: &[(&str, &[&str])] = &[("u0.cpp", &[]), ("u1.cpp", &[])];
const U1_U0: &[(&str, &[&str])] = &[("u1.cpp", &[]), ("u0.cpp", &[])];

#[test]
fn s1_hook_pair_today() {
    // WV: each observer is verified alone, the run is `Verified`, and the
    // program no longer links (old::hook has no definition).
    pin(
        s1(U0_U1),
        "u0.cpp: Verified\nu1.cpp: Verified use 0 via 1 direct 0 kept_via 0\n\
         run: Verified stale 0 unused 0\n",
    );
    pin(
        s1(U1_U0),
        "u1.cpp: Verified use 0 via 1 direct 0 kept_via 0\nu0.cpp: Verified\n\
         run: Verified stale 0 unused 0\n",
    );
}

#[test]
#[ignore = "pending: WP-146 stage 3 (S1: an ownership finding, T, the run not verified)"]
fn s1_hook_pair_stage3() {
    for order in [U0_U1, U1_U0] {
        ownership_finding(s1(order), "old::hook(int)", &["dep/d.h", "u1.cpp"]);
    }
}

// ---------------------------------------------------------------------------
// S10: the mirror (§0.1). dep/e.h already declares fresh::g; u1 defines
// old::g, does not include e.h, and moves it to fresh; u0 includes e.h and
// calls the dependency's fresh::g. After the edit the two are one
// function. §3.4: WV today, by reading; stage 3: a finding (M). Both
// orders; and under a rule on the function itself (`relocate old::g(int)
// => fresh :: =`), WV too by reading (§0.3).

const S10_E: &str = "namespace fresh { int g(int); }\n";
const S10_U1: &str = "namespace old { int g(int x) { return x; } }\n";
const S10_U1_POST: &str = "namespace fresh { int g(int x) { return x; } }\n";
const S10_U0: &str = "#include \"dep/e.h\"\nint r = fresh::g(1);\n";
const S10_U1_OWN: &str =
    "namespace old { int g(int x) { return x; } int o; }\nnamespace fresh { int m; }\n";
const S10_U1_OWN_POST: &str =
    "namespace old { int o; }\nnamespace fresh { int m; int g(int x) { return x; } }\n";

fn s10(
    observers: &'static [(&'static str, &'static [&'static str])],
    own_key: bool,
) -> Shape<'static> {
    // Under a rule on the function alone the namespaces stay: each keeps
    // a member of its own.
    let (u1, u1_post) = if own_key { (S10_U1_OWN, S10_U1_OWN_POST) } else { (S10_U1, S10_U1_POST) };
    let declared =
        own_key.then(|| declared_text("relocate old::g(int) => fresh :: =\n", u1, u1_post));
    let pre: &'static [(&str, &str)] =
        Box::leak(Box::new([("dep/e.h", S10_E), ("u1.cpp", u1), ("u0.cpp", S10_U0)]));
    let post: &'static [(&str, &str)] = Box::leak(Box::new([("u1.cpp", u1_post)]));
    Shape { name: "s10", pre, post, declared, readonly: RO, observers }
}

#[test]
fn s10_mirror_today() {
    // WV, as §0.1 says by reading.
    pin(
        s10(U0_U1, false),
        "u0.cpp: Verified\nu1.cpp: Verified use 0 via 1 direct 0 kept_via 0\n\
         run: Verified stale 0 unused 0\n",
    );
    pin(
        s10(U1_U0, false),
        "u1.cpp: Verified use 0 via 1 direct 0 kept_via 0\nu0.cpp: Verified\n\
         run: Verified stale 0 unused 0\n",
    );
    // Under the rule on the function itself: WV too (§0.3).
    pin(
        s10(U0_U1, true),
        "u0.cpp: Verified\nu1.cpp: Verified use 0 via 1 direct 0 kept_via 0\n\
         run: Verified stale 0 unused 0\n",
    );
}

#[test]
#[ignore = "pending: WP-146 stage 3 (S10, the mirror: an ownership finding, M, the run not verified)"]
fn s10_mirror_stage3() {
    for (order, own_key) in [(U0_U1, false), (U1_U0, false), (U0_U1, true)] {
        ownership_finding(s10(order, own_key), "fresh::g(int)", &["dep/e.h", "u1.cpp"]);
    }
}

// ---------------------------------------------------------------------------
// S23: S1 under a rule on the function itself, `relocate old::hook(int)
// => fresh :: =`. §0.3, §3.4: by reading, u0's dependency entity has the
// key the rule produces (`rule_use::unmapped`: direct), so u0 is stale and
// the run is not verified today (H); stage 3 adds the finding.

fn s23() -> Shape<'static> {
    // The namespaces stay: each keeps a member of its own.
    const U1: &str =
        "namespace old { int hook(int x) { return x; } int o; }\nnamespace fresh { int m; }\n";
    const U1_POST: &str =
        "namespace old { int o; }\nnamespace fresh { int m; int hook(int x) { return x; } }\n";
    Shape {
        pre: &[("dep/d.h", S1_D), ("u1.cpp", U1), ("u0.cpp", S1_U0)],
        post: &[("u1.cpp", U1_POST)],
        declared: Some(declared_text("relocate old::hook(int) => fresh :: =\n", U1, U1_POST)),
        ..s1(U0_U1)
    }
}

#[test]
fn s23_own_key_rule_pair_today() {
    pin(
        s23(),
        "u0.cpp: Verified use 0 via 0 direct 1 kept_via 0\n\
         u1.cpp: Verified use 0 via 1 direct 0 kept_via 0\n\
         run: NotVerified stale 1 unused 0\n",
    );
}

#[test]
#[ignore = "pending: WP-146 stage 3 (S23: not verified today; plus an ownership finding, T)"]
fn s23_own_key_rule_pair_stage3() {
    ownership_finding(s23(), "old::hook(int)", &["dep/d.h", "u1.cpp"]);
}

// ---------------------------------------------------------------------------
// S2: a class the dependency forward-declares and owned code defines.
// §3.4: WV today, by reading; stage 3: a finding at old::W.

fn s2() -> Shape<'static> {
    Shape {
        name: "s2",
        pre: &[
            ("dep/d.h", "namespace old { struct W; void take(W*); }\n"),
            ("own/w.h", "namespace old { struct W { int x; }; }\n"),
            ("u1.cpp", "#include \"own/w.h\"\nint a = sizeof(old::W);\n"),
            ("u0.cpp", "#include \"dep/d.h\"\nvoid f(old::W* p) { old::take(p); }\n"),
        ],
        post: &[
            ("own/w.h", "namespace fresh { struct W { int x; }; }\n"),
            ("u1.cpp", "#include \"own/w.h\"\nint a = sizeof(fresh::W);\n"),
        ],
        declared: None,
        readonly: RO,
        observers: U0_U1,
    }
}

#[test]
fn s2_class_forward_declared_today() {
    pin(
        s2(),
        "u0.cpp: Verified\nu1.cpp: Verified use 0 via 1 direct 0 kept_via 0\n\
         run: Verified stale 0 unused 0\n",
    );
}

#[test]
#[ignore = "pending: WP-146 stage 3 (S2: an ownership finding at old::W, the run not verified)"]
fn s2_class_forward_declared_stage3() {
    ownership_finding(s2(), "old::W", &["dep/d.h", "own/w.h"]);
}

// ---------------------------------------------------------------------------
// S3: a variable a dependency declares `extern`. WV today; stage 3: a
// finding.

fn s3() -> Shape<'static> {
    Shape {
        name: "s3",
        pre: &[
            ("dep/d.h", "namespace old { extern int level; }\n"),
            ("u1.cpp", "namespace old { int level = 0; }\n"),
            ("u0.cpp", "#include \"dep/d.h\"\nint r = old::level;\n"),
        ],
        post: &[("u1.cpp", "namespace fresh { int level = 0; }\n")],
        declared: None,
        readonly: RO,
        observers: U0_U1,
    }
}

#[test]
fn s3_extern_variable_today() {
    pin(
        s3(),
        "u0.cpp: Verified\nu1.cpp: Verified use 0 via 1 direct 0 kept_via 0\n\
         run: Verified stale 0 unused 0\n",
    );
}

#[test]
#[ignore = "pending: WP-146 stage 3 (S3: an ownership finding, the run not verified)"]
fn s3_extern_variable_stage3() {
    ownership_finding(s3(), "old::level", &["dep/d.h", "u1.cpp"]);
}

// ---------------------------------------------------------------------------
// S4: a function template owned code defines and a dependency declares.
// WV today; stage 3: a finding.

fn s4() -> Shape<'static> {
    Shape {
        name: "s4",
        pre: &[
            ("dep/d.h", "namespace old { template <class T> T conv(T); }\n"),
            ("u1.cpp", "namespace old { template <class T> T conv(T t) { return t; } }\n"),
            ("u0.cpp", "#include \"dep/d.h\"\nint r = old::conv(1);\n"),
        ],
        post: &[("u1.cpp", "namespace fresh { template <class T> T conv(T t) { return t; } }\n")],
        declared: None,
        readonly: RO,
        observers: U0_U1,
    }
}

#[test]
fn s4_function_template_today() {
    pin(
        s4(),
        "u0.cpp: Verified\nu1.cpp: Verified use 0 via 1 direct 0 kept_via 0\n\
         run: Verified stale 0 unused 0\n",
    );
}

#[test]
#[ignore = "pending: WP-146 stage 3 (S4: an ownership finding, the run not verified)"]
fn s4_function_template_stage3() {
    ownership_finding(s4(), "old::conv", &["dep/d.h", "u1.cpp"]);
}

// ---------------------------------------------------------------------------
// S5: a namespace. dep/d.h opens old; u1 moves its own old to fresh. The
// unit `old` merges by rule (`spec/verify.md` §2.1): verified, rightly
// (H), today and after stage 3 (no entry for a namespace).

#[test]
fn s5_namespace_today() {
    pin(
        Shape {
            name: "s5",
            pre: &[
                ("dep/d.h", "namespace old { struct D; }\n"),
                ("u1.cpp", "namespace old { struct A {}; }\nint a = sizeof(old::A);\n"),
                ("u0.cpp", "#include \"dep/d.h\"\nold::D* p = nullptr;\n"),
            ],
            post: &[("u1.cpp", "namespace fresh { struct A {}; }\nint a = sizeof(fresh::A);\n")],
            declared: None,
            readonly: RO,
            observers: U0_U1,
        },
        "u0.cpp: Verified\nu1.cpp: Verified use 0 via 1 direct 0 kept_via 0\n\
         run: Verified stale 0 unused 0\n",
    );
}

// ---------------------------------------------------------------------------
// S6: an inline function defined in an owned header that a dependency
// header also declares. WV where u0 includes only the dependency's header;
// a collision in u0 (H) where it includes both.

fn s6(u0_both: bool) -> Shape<'static> {
    let u0 = if u0_both {
        "#include \"dep/d.h\"\n#include \"own/t.h\"\nint r = old::twice(1);\n"
    } else {
        "#include \"dep/d.h\"\nint r = old::twice(1);\n"
    };
    let u0_post = if u0_both {
        "#include \"dep/d.h\"\n#include \"own/t.h\"\nint r = fresh::twice(1);\n"
    } else {
        u0
    };
    let pre: &'static [(&str, &str)] = Box::leak(Box::new([
        ("dep/d.h", "namespace old { int twice(int); }\n"),
        ("own/t.h", "namespace old { inline int twice(int x) { return 2 * x; } }\n"),
        ("u1.cpp", "#include \"own/t.h\"\nint a = old::twice(1);\n"),
        ("u0.cpp", u0),
    ]));
    let post: &'static [(&str, &str)] = Box::leak(Box::new([
        ("own/t.h", "namespace fresh { inline int twice(int x) { return 2 * x; } }\n"),
        ("u1.cpp", "#include \"own/t.h\"\nint a = fresh::twice(1);\n"),
        ("u0.cpp", u0_post),
    ]));
    Shape { name: "s6", pre, post, declared: None, readonly: RO, observers: U0_U1 }
}

#[test]
fn s6_inline_function_today() {
    pin(
        s6(false),
        "u0.cpp: Verified\nu1.cpp: Verified use 0 via 1 direct 0 kept_via 0\n\
         run: Verified stale 0 unused 0\n",
    );
    // u0 includes both: the collision of one observer (H).
    pin(
        s6(true),
        "u0.cpp: Mismatch [Indeterminate old::twice(int)] use 0 via 1 direct 0 kept_via 0\n\
         u1.cpp: Verified use 0 via 1 direct 0 kept_via 0\n\
         run: NotVerified stale 0 unused 0\n",
    );
}

#[test]
#[ignore = "pending: WP-146 stage 3 (S6: an ownership finding, the run not verified)"]
fn s6_inline_function_stage3() {
    ownership_finding(s6(false), "old::twice(int)", &["dep/d.h", "own/t.h"]);
}

// ---------------------------------------------------------------------------
// S7: a specialisation of a dependency's template. Where owned code
// redeclares the template (own/fwd.h), u1 reads the template as owned and
// moves it with its specialisation: WV (S2 for the template); stage 3: a
// finding at old::box. Where every observer that sees the specialisation
// sees the dependency's template (own/spec.h includes d.h), the
// specialisation follows the template's ownership: H.

fn s7() -> Shape<'static> {
    Shape {
        name: "s7",
        pre: &[
            ("dep/d.h", "namespace old { template <class T> struct box {}; }\n"),
            (
                "own/fwd.h",
                "namespace old { template <class T> struct box; struct Mine {};\n\
                 template <> struct box<Mine> { int v; }; }\n",
            ),
            ("u1.cpp", "#include \"own/fwd.h\"\nint a = sizeof(old::box<old::Mine>);\n"),
            ("u0.cpp", "#include \"dep/d.h\"\nold::box<int> b;\n"),
        ],
        post: &[
            (
                "own/fwd.h",
                "namespace fresh { template <class T> struct box; struct Mine {};\n\
                 template <> struct box<Mine> { int v; }; }\n",
            ),
            ("u1.cpp", "#include \"own/fwd.h\"\nint a = sizeof(fresh::box<fresh::Mine>);\n"),
        ],
        declared: None,
        readonly: RO,
        observers: U0_U1,
    }
}

#[test]
fn s7_specialisation_today() {
    pin(
        s7(),
        "u0.cpp: Verified\nu1.cpp: Verified use 0 via 1 direct 0 kept_via 0\n\
         run: Verified stale 0 unused 0\n",
    );
    // Every observer that sees the specialisation sees the dependency's
    // template: Mine moves, the specialisation stays the template's.
    pin(
        Shape {
            name: "s7-h",
            pre: &[
                ("dep/d.h", "namespace old { template <class T> struct box {}; }\n"),
                (
                    "own/spec.h",
                    "#include \"../dep/d.h\"\nnamespace old { struct Mine {}; }\n\
                     template <> struct old::box<old::Mine> { int v; };\n",
                ),
                ("u1.cpp", "#include \"own/spec.h\"\nint a = sizeof(old::box<old::Mine>);\n"),
                ("u0.cpp", "#include \"dep/d.h\"\nold::box<int> b;\n"),
            ],
            post: &[
                (
                    "own/spec.h",
                    "#include \"../dep/d.h\"\nnamespace fresh { struct Mine {}; }\n\
                     template <> struct old::box<fresh::Mine> { int v; };\n",
                ),
                ("u1.cpp", "#include \"own/spec.h\"\nint a = sizeof(old::box<fresh::Mine>);\n"),
            ],
            declared: None,
            readonly: RO,
            observers: U0_U1,
        },
        "u0.cpp: Verified\nu1.cpp: Verified use 0 via 1 direct 0 kept_via 0\n\
         run: Verified stale 0 unused 0\n",
    );
}

#[test]
#[ignore = "pending: WP-146 stage 3 (S7: an ownership finding at old::box, the run not verified)"]
fn s7_specialisation_stage3() {
    ownership_finding(s7(), "old::box", &["dep/d.h", "own/fwd.h"]);
}

// ---------------------------------------------------------------------------
// S8: a friend a dependency's class declares and owned code defines. u1
// sees only the dependency's forward declaration of D. §3.4: WV today, by
// reading; stage 3: a finding at old::peek(const old::D &).

fn s8() -> Shape<'static> {
    Shape {
        name: "s8",
        pre: &[
            ("dep/d.h", "namespace old { struct D { friend int peek(const D&); int v; }; }\n"),
            ("dep/fwd.h", "namespace old { struct D; }\n"),
            (
                "u1.cpp",
                "#include \"dep/fwd.h\"\nnamespace old { int peek(const D&) { return 0; } }\n",
            ),
            ("u0.cpp", "#include \"dep/d.h\"\nint r(const old::D& d) { return peek(d); }\n"),
        ],
        post: &[(
            "u1.cpp",
            "#include \"dep/fwd.h\"\nnamespace fresh { int peek(const old::D&) { return 0; } }\n",
        )],
        declared: None,
        readonly: RO,
        observers: U0_U1,
    }
}

#[test]
fn s8_friend_of_dependency_class_today() {
    pin(
        s8(),
        "u0.cpp: Verified\nu1.cpp: Verified use 0 via 1 direct 0 kept_via 0\n\
         run: Verified stale 0 unused 0\n",
    );
}

#[test]
#[ignore = "pending: WP-146 stage 3 (S8: an ownership finding at old::peek, the run not verified)"]
fn s8_friend_of_dependency_class_stage3() {
    ownership_finding(s8(), "old::peek(const old::D &)", &["dep/d.h", "u1.cpp"]);
}

// ---------------------------------------------------------------------------
// S9: `a4-owned-header-both-ways` across observers. own/own.h is entered
// through the read-only dep/ro_inc.h in u0 (read-only there by the
// include-chain rule) and directly in u1. The entity moved: u0 is a
// mismatch (H for the verdict); stage 3 adds the finding (M).

fn s9() -> Shape<'static> {
    Shape {
        name: "s9",
        pre: &[
            ("dep/ro_inc.h", "#include \"../own/own.h\"\n"),
            ("own/own.h", "namespace old { int g(int); }\n"),
            ("u1.cpp", "#include \"own/own.h\"\nint a = old::g(1);\n"),
            ("u0.cpp", "#include \"dep/ro_inc.h\"\nint r = old::g(1);\n"),
        ],
        post: &[
            ("own/own.h", "namespace fresh { int g(int); }\n"),
            ("u1.cpp", "#include \"own/own.h\"\nint a = fresh::g(1);\n"),
            ("u0.cpp", "#include \"dep/ro_inc.h\"\nint r = fresh::g(1);\n"),
        ],
        declared: None,
        readonly: RO,
        observers: U0_U1,
    }
}

#[test]
fn s9_owned_header_both_ways_today() {
    pin(
        s9(),
        "u0.cpp: Mismatch [Changed r] [Removed old] [Removed old::g(int)] [Added fresh] \
         [Added fresh::g(int)]\n\
         u1.cpp: Verified use 0 via 1 direct 0 kept_via 0\n\
         run: NotVerified stale 0 unused 0\n",
    );
}

#[test]
#[ignore = "pending: WP-146 stage 3 (S9: not verified today; plus an ownership finding, M)"]
fn s9_owned_header_both_ways_stage3() {
    // Both entries are in own/own.h: the finding names it and the two
    // observers.
    ownership_finding(s9(), "fresh::g(int)", &["own/own.h", "u0.cpp", "u1.cpp"]);
}

// ---------------------------------------------------------------------------
// S11: S1 with the hook not moved and `kept old::hook(int)`; u0 moves
// something of its own so that the rule is used. Verified, rightly (H).

#[test]
fn s11_hook_kept_today() {
    let u0 = "#include \"dep/d.h\"\nnamespace old { inline int f0() { return 0; } }\n\
              int r = old::f0() + old::hook(1);\n";
    let u0_post = "#include \"dep/d.h\"\nnamespace fresh { inline int f0() { return 0; } }\n\
                   int r = fresh::f0() + old::hook(1);\n";
    let snippet_pre =
        "namespace old { int hook(int x) { return x; } inline int f0() { return 0; } }\n";
    let snippet_post = "namespace old { int hook(int x) { return x; } }\n\
                        namespace fresh { inline int f0() { return 0; } }\n";
    pin(
        Shape {
            name: "s11",
            pre: &[("dep/d.h", S1_D), ("u1.cpp", S1_U1), ("u0.cpp", u0)],
            post: &[("u0.cpp", u0_post)],
            declared: Some(declared_text(
                "relocate old => :: fresh\nkept old::hook(int)\n",
                snippet_pre,
                snippet_post,
            )),
            readonly: RO,
            observers: U0_U1,
        },
        "u0.cpp: Verified use 0 via 1 direct 0 kept_via 0\nu1.cpp: Verified\n\
         run: Verified stale 0 unused 0\n",
    );
}

// ---------------------------------------------------------------------------
// S12: S1 with a third observer that sees neither declaration. As S1: WV
// today; stage 3: the finding; the third changes nothing.

fn s12() -> Shape<'static> {
    Shape {
        name: "s12",
        pre: &[("dep/d.h", S1_D), ("u1.cpp", S1_U1), ("u0.cpp", S1_U0), ("u2.cpp", "int z;\n")],
        observers: &[("u0.cpp", &[]), ("u1.cpp", &[]), ("u2.cpp", &[])],
        ..s1(U0_U1)
    }
}

#[test]
fn s12_third_observer_today() {
    pin(
        s12(),
        "u0.cpp: Verified\nu1.cpp: Verified use 0 via 1 direct 0 kept_via 0\nu2.cpp: Verified\n\
         run: Verified stale 0 unused 0\n",
    );
}

#[test]
#[ignore = "pending: WP-146 stage 3 (S12: as S1 with a third observer, the run not verified)"]
fn s12_third_observer_stage3() {
    ownership_finding(s12(), "old::hook(int)", &["dep/d.h", "u1.cpp"]);
}

// ---------------------------------------------------------------------------
// S13: WP-140's two views of a namespace: dep/d.h opens fresh; u0 moves
// old into it beside the dependency's members, u1 does not include d.h.
// By rule (the owned view): verified, rightly (H).

#[test]
fn s13_two_view_namespace_today() {
    let u0 = "#include \"dep/d.h\"\nnamespace old { inline int f0() { return 0; } }\n\
              int u0() { return old::f0() + sizeof(fresh::D); }\n";
    let u1 = "namespace old { inline int f1() { return 1; } }\nint u1() { return old::f1(); }\n";
    pin(
        Shape {
            name: "s13",
            pre: &[
                ("dep/d.h", "namespace fresh { struct D {}; }\n"),
                ("u0.cpp", u0),
                ("u1.cpp", u1),
            ],
            post: &[
                ("u0.cpp", &*Box::leak(u0.replace("old", "fresh").into_boxed_str())),
                ("u1.cpp", &*Box::leak(u1.replace("old", "fresh").into_boxed_str())),
            ],
            declared: None,
            readonly: RO,
            observers: U0_U1,
        },
        "u0.cpp: Verified use 0 via 1 direct 0 kept_via 0\n\
         u1.cpp: Verified use 0 via 1 direct 0 kept_via 0\n\
         run: Verified stale 0 unused 0\n",
    );
}

// ---------------------------------------------------------------------------
// S14: an entity a dependency declares that the edit does not move:
// dep/d.h declares old::other, u1 defines it and leaves it (and moves f).
// u1 is stale by `direct` unless `kept old::other(int)`; with it, verified,
// rightly (H).

#[test]
fn s14_dependency_declares_not_moved_today() {
    const U1: &str = "namespace old { int other(int x) { return x; } int f() { return 1; } }\n";
    const U1_POST: &str = "namespace old { int other(int x) { return x; } }\n\
                           namespace fresh { int f() { return 1; } }\n";
    let (u1, u1_post) = (U1, U1_POST);
    let shape = |declared| Shape {
        name: "s14",
        pre: &[
            ("dep/d.h", "namespace old { int other(int); }\n"),
            ("u1.cpp", U1),
            ("u0.cpp", "#include \"dep/d.h\"\nint r = old::other(1);\n"),
        ],
        post: &[("u1.cpp", U1_POST)],
        declared,
        readonly: RO,
        observers: U0_U1,
    };
    pin(
        shape(None),
        "u0.cpp: Verified\nu1.cpp: Verified use 0 via 1 direct 1 kept_via 0\n\
         run: NotVerified stale 1 unused 0\n",
    );
    pin(
        shape(Some(declared_text("relocate old => :: fresh\nkept old::other(int)\n", u1, u1_post))),
        "u0.cpp: Verified\nu1.cpp: Verified use 0 via 1 direct 0 kept_via 0\n\
         run: Verified stale 0 unused 0\n",
    );
}

// ---------------------------------------------------------------------------
// S15, a limit (§3.6): no observer of the run includes the dependency's
// header (`tests/rules/owned/observer-includes-owned-only`). dep/d.h
// forward-declares old::Both; two owned observers define and use it and
// move it. No observer shows the dependency's reading: WV today, and
// still after stage 3 (no fixed entry anywhere); `spec/verify.md` §8 and
// "Open questions" ("A dependency's header no observer includes").

#[test]
fn s15_no_observer_includes_the_header_today() {
    pin(
        Shape {
            name: "s15",
            pre: &[
                ("dep/d.h", "namespace old { struct Both; }\n"),
                ("own/both.h", "namespace old { struct Both { int v; }; }\n"),
                ("u1.cpp", "#include \"own/both.h\"\nint a = sizeof(old::Both);\n"),
                ("u0.cpp", "#include \"own/both.h\"\nint r(old::Both b) { return b.v; }\n"),
            ],
            post: &[
                ("own/both.h", "namespace fresh { struct Both { int v; }; }\n"),
                ("u1.cpp", "#include \"own/both.h\"\nint a = sizeof(fresh::Both);\n"),
                ("u0.cpp", "#include \"own/both.h\"\nint r(fresh::Both b) { return b.v; }\n"),
            ],
            declared: None,
            readonly: RO,
            observers: U0_U1,
        },
        "u0.cpp: Verified use 0 via 1 direct 0 kept_via 0\n\
         u1.cpp: Verified use 0 via 1 direct 0 kept_via 0\n\
         run: Verified stale 0 unused 0\n",
    );
}

// ---------------------------------------------------------------------------
// S16, a limit (§3.6): a name pasted by a dependency's macro
// (`tests/rules/owned/dependency-macro-name-pasted`). The name is spelled
// in scratch space and decided by its expansion location: owned in every
// observer, no fixed entry. WV today and after stage 3; WP-116 is to
// close it.

#[test]
fn s16_pasted_name_today() {
    let d = "#define DECL_INIT() void legacy_##init();\n";
    let u0 = "#include \"dep/d.h\"\nnamespace old { DECL_INIT() }\nvoid user() { old::legacy_init(); }\n";
    let u1 = "#include \"dep/d.h\"\nnamespace old { DECL_INIT() }\nvoid other() { old::legacy_init(); }\n";
    pin(
        Shape {
            name: "s16",
            pre: &[("dep/d.h", d), ("u0.cpp", u0), ("u1.cpp", u1)],
            post: &[
                ("u0.cpp", &*Box::leak(u0.replace("old", "fresh").into_boxed_str())),
                ("u1.cpp", &*Box::leak(u1.replace("old", "fresh").into_boxed_str())),
            ],
            declared: None,
            readonly: RO,
            observers: U0_U1,
        },
        "u0.cpp: Verified use 0 via 1 direct 0 kept_via 0\n\
         u1.cpp: Verified use 0 via 1 direct 0 kept_via 0\n\
         run: Verified stale 0 unused 0\n",
    );
}

// ---------------------------------------------------------------------------
// S17, a limit (§3.6): an inclusion the walk cannot see
// (`dependency-built-against-owned-type-direct-first`). own/user_types.h
// is owned; the read-only dep/dep.h includes it; every observer enters it
// directly first and dep.h's `#include` is skipped by `#pragma once`: owned
// everywhere, WV today and after stage 3 (Kiln's WP-2111). Where one
// observer enters it through the dependency first, that observer is a
// mismatch (H; `-through-dependency-first`).

fn s17(through_first: bool) -> Shape<'static> {
    let pre_types = "#pragma once\nnamespace old { struct UserData { int v; }; }\n\
                     namespace old_cfg { typedef old::UserData data_t; }\n";
    let post_types = "#pragma once\nnamespace fresh { struct UserData { int v; }; }\n\
                      namespace old_cfg { typedef fresh::UserData data_t; }\n";
    let dep =
        "#include \"../own/user_types.h\"\nnamespace old { void consume(old_cfg::data_t*); }\n";
    let u = |name: &str, ns: &str, first: bool| -> &'static str {
        let includes = if first {
            "#include \"dep/dep.h\"\n#include \"own/user_types.h\"\n"
        } else {
            "#include \"own/user_types.h\"\n#include \"dep/dep.h\"\n"
        };
        Box::leak(
            format!("{includes}void {name}({ns}::UserData* p) {{ old::consume(p); }}\n")
                .into_boxed_str(),
        )
    };
    let pre: &'static [(&str, &str)] = Box::leak(Box::new([
        ("own/user_types.h", pre_types),
        ("dep/dep.h", dep),
        ("u0.cpp", u("user0", "old", through_first)),
        ("u1.cpp", u("user1", "old", false)),
    ]));
    let post: &'static [(&str, &str)] = Box::leak(Box::new([
        ("own/user_types.h", post_types),
        ("u0.cpp", u("user0", "fresh", through_first)),
        ("u1.cpp", u("user1", "fresh", false)),
    ]));
    Shape { name: "s17", pre, post, declared: None, readonly: RO, observers: U0_U1 }
}

#[test]
fn s17_guard_skipped_inclusion_today() {
    pin(
        s17(false),
        "u0.cpp: Verified use 0 via 1 direct 0 kept_via 0\n\
         u1.cpp: Verified use 0 via 1 direct 0 kept_via 0\n\
         run: Verified stale 0 unused 0\n",
    );
    pin(
        s17(true),
        "u0.cpp: Mismatch [Changed old_cfg::data_t] [Removed old::consume(old::UserData *)] \
         [Removed old::UserData] [Removed old::UserData::v] [Removed user0(old::UserData *)] \
         [Added fresh] [Added old::consume(fresh::UserData *)] [Added fresh::UserData::v] \
         [Added fresh::UserData] [Added user0(fresh::UserData *)]\n\
         u1.cpp: Verified use 0 via 1 direct 0 kept_via 0\n\
         run: NotVerified stale 0 unused 0\n",
    );
}

#[test]
#[ignore = "pending: WP-146 stage 3 (S17 through the dependency first: not verified today; \
            plus an ownership finding, M)"]
fn s17_through_dependency_first_stage3() {
    // Both entries are in own/user_types.h: the finding names it and the
    // two observers.
    ownership_finding(s17(true), "fresh::UserData", &["own/user_types.h", "u0.cpp", "u1.cpp"]);
}

// ---------------------------------------------------------------------------
// S18: a declaration whose name a dependency macro spells in u0 (-DUSE_DEP
// includes dep/m.h) and an owned macro in u1 (own/m.h); the text of both
// is `struct Thing {};`. In u0 Thing is the dependency's and moved: a
// mismatch (H).

fn s18() -> Shape<'static> {
    let m = "#define THING struct Thing { int v; };\n";
    let u = |ns: &str| -> &'static str {
        Box::leak(
            format!(
                "#ifdef USE_DEP\n#include \"dep/m.h\"\n#else\n#include \"own/m.h\"\n#endif\n\
                 namespace {ns} {{ THING }}\nint a = sizeof({ns}::Thing);\n"
            )
            .into_boxed_str(),
        )
    };
    Shape {
        name: "s18",
        pre: Box::leak(Box::new([("dep/m.h", m), ("own/m.h", m), ("u.cpp", u("old"))])),
        post: Box::leak(Box::new([("u.cpp", u("fresh"))])),
        declared: None,
        readonly: RO,
        observers: &[("u.cpp", &["-DUSE_DEP"]), ("u.cpp", &[])],
    }
}

#[test]
fn s18_name_token_both_ways_today() {
    pin(
        s18(),
        "u.cpp: Mismatch [Changed a] [Removed old::Thing::v] [Removed old::Thing] \
         [Added fresh (the dependency's part of fresh)] [Added fresh::Thing::v] \
         [Added fresh::Thing] use 0 via 1 direct 0 kept_via 0\n\
         u.cpp: Verified use 0 via 1 direct 0 kept_via 0\n\
         run: NotVerified stale 0 unused 0\n",
    );
}

#[test]
#[ignore = "pending: WP-146 stage 3 (S18: not verified today; plus an ownership finding, M)"]
fn s18_name_token_both_ways_stage3() {
    ownership_finding(s18(), "fresh::Thing", &["dep/m.h", "own/m.h"]);
}

// ---------------------------------------------------------------------------
// S19: internal linkage. dep/d.h has its own `static` old::helper; u1 has
// another and moves it. Two entities by the language, alike by key: not to
// be reported. Verified, rightly (H).

#[test]
fn s19_internal_linkage_today() {
    pin(
        Shape {
            name: "s19",
            pre: &[
                ("dep/d.h", "namespace old { static int helper() { return 0; } }\n"),
                (
                    "u1.cpp",
                    "namespace old { static int helper() { return 1; } }\nint a = old::helper();\n",
                ),
                ("u0.cpp", "#include \"dep/d.h\"\nint r = old::helper();\n"),
            ],
            post: &[(
                "u1.cpp",
                "namespace fresh { static int helper() { return 1; } }\nint a = fresh::helper();\n",
            )],
            declared: None,
            readonly: RO,
            observers: U0_U1,
        },
        "u0.cpp: Verified\nu1.cpp: Verified use 0 via 1 direct 0 kept_via 0\n\
         run: Verified stale 0 unused 0\n",
    );
}

// ---------------------------------------------------------------------------
// S20: `extern "C"`. The key's parent is `c_linkage`; no namespace rule
// changes it and the symbol is the same: verified, rightly (H).

#[test]
fn s20_extern_c_today() {
    pin(
        Shape {
            name: "s20",
            pre: &[
                ("dep/d.h", "extern \"C\" int c_hook(int);\n"),
                ("u1.cpp", "namespace old { extern \"C\" { int c_hook(int x) { return x; } } }\n"),
                ("u0.cpp", "#include \"dep/d.h\"\nint r = c_hook(1);\n"),
            ],
            post: &[(
                "u1.cpp",
                "namespace fresh { extern \"C\" { int c_hook(int x) { return x; } } }\n",
            )],
            declared: None,
            readonly: RO,
            observers: U0_U1,
        },
        "u0.cpp: Verified\nu1.cpp: Verified use 0 via 1 direct 0 kept_via 0\n\
         run: Verified stale 0 unused 0\n",
    );
}

// ---------------------------------------------------------------------------
// S21: an alias a dependency declares and owned code redeclares and
// moves. Verified today; no binary holds an alias. By ruling 4 (every kind
// that collides in one observer, less internal linkage) stage 3 reports
// it: pending, the run not verified.

fn s21() -> Shape<'static> {
    Shape {
        name: "s21",
        pre: &[
            ("dep/d.h", "namespace old { typedef int id_t; }\n"),
            ("u1.cpp", "namespace old { typedef int id_t; }\nold::id_t a = 0;\n"),
            ("u0.cpp", "#include \"dep/d.h\"\nold::id_t r = 1;\n"),
        ],
        post: &[("u1.cpp", "namespace fresh { typedef int id_t; }\nfresh::id_t a = 0;\n")],
        declared: None,
        readonly: RO,
        observers: U0_U1,
    }
}

#[test]
fn s21_alias_today() {
    pin(
        s21(),
        "u0.cpp: Verified\nu1.cpp: Verified use 0 via 1 direct 0 kept_via 0\n\
         run: Verified stale 0 unused 0\n",
    );
}

#[test]
#[ignore = "pending: WP-146 stage 3 (S21: an ownership finding by ruling 4, the run not verified)"]
fn s21_alias_stage3() {
    ownership_finding(s21(), "old::id_t", &["dep/d.h", "u1.cpp"]);
}

// ---------------------------------------------------------------------------
// S22, a limit (§3.6): a friend only a dependency's class template
// declares. u1 defines old::peek(old::D<int>&) and moves it; u0
// instantiates old::D<int> from d.h and calls peek. §3.6, by reading:
// a finding after stage 3 only where an observer instantiates the class
// and so declares the friend at namespace scope.

fn s22() -> Shape<'static> {
    const D: &str =
        "namespace old { template <class T> struct D { friend int peek(D<int>&); T v; }; }\n";
    Shape {
        name: "s22",
        pre: &[
            ("dep/d.h", D),
            (
                "u1.cpp",
                "#include \"dep/d.h\"\nnamespace old { int peek(D<int>& d) { return 0; } }\n",
            ),
            ("u0.cpp", "#include \"dep/d.h\"\nold::D<int> x;\nint r = peek(x);\n"),
        ],
        post: &[(
            "u1.cpp",
            "#include \"dep/d.h\"\nnamespace fresh { int peek(old::D<int>& d) { return 0; } }\n",
        )],
        declared: None,
        readonly: RO,
        observers: U0_U1,
    }
}

#[test]
fn s22_friend_of_class_template_today() {
    pin(
        s22(),
        "u0.cpp: Verified\nu1.cpp: Verified use 0 via 1 direct 0 kept_via 0\n\
         run: Verified stale 0 unused 0\n",
    );
}

#[test]
#[ignore = "pending: WP-146 stage 3 (S22: u0 instantiates old::D<int>, so the lists join, T)"]
fn s22_friend_of_class_template_stage3() {
    ownership_finding(s22(), "old::peek(old::D<int> &)", &["dep/d.h", "u1.cpp"]);
}
