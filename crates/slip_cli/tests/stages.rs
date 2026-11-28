//! `spec/exec.md` §8.12, §8.14 and `spec/edits.md` §7.2, §7.3, §10.2 and
//! §10.5 through the built executable: `slip reduce` and `slip apply`
//! (`docs/wp/WP-075.md` "Tests") over a store each test fills by hand with
//! a plan, a request, map shards and results made with `slip_model`. No
//! Kiln: nothing here parses.
//!
//! Each test has its own directory (a small tree `src/` and a store),
//! under the work root (`common::work_root()`), removed on drop. Every wait on a child is
//! bounded.

mod common;

use std::fs;
use std::io::Read;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use slip_apply::journal::{Journal, JournalEntry, staged_name};
use slip_cli::transform::reduce::reduce_round;
use slip_exec::store::{Access, Object, Store};
use slip_model::declared::Declared;
use slip_model::edits::{
    Claim, Edits, Failure, FailureKind, Instance, Piece, Place, Reason, Report, Role, SCHEME,
    Severity, SourcePlace, Stance, Subject, tiers,
};
use slip_model::editset::EditSet;
use slip_model::formats::content_id;
use slip_model::hash::content_digest;
use slip_model::overlay::{Content, Overlay, OverlayFile};
use slip_model::result::{Category, Residue, RunVerdict, Spellings, State, Tally, TransformResult};
use slip_model::transform::{Ownership, Rename, Script, TargetKind, Transform, TransformKind};
use slip_model::transform_run::TransformRun;

/// A report of a test shard: its place (file, offset), severity and text.
type ShardReport = (Option<(&'static str, u64)>, Severity, Vec<u8>);
use slip_model::{
    Command as Cmd, Configuration, Digest, End, FileRef, Job, Language, Observer, Origin, Outcome,
    Phase, Plan, Run, RunRecord, Shard, Status,
};

const SLIP: &str = env!("CARGO_BIN_EXE_slip");

/// The bound on one `slip` command.
const BOUND: Duration = Duration::from_mins(2);

/// The tree every test starts from: a header both observers read, and
/// each observer's main file. The name `f` is at the offsets below.
const H: &str = "int f();\n";
const A: &str = "#include \"h.h\"\nint x = f();\n";
const B: &str = "#include \"h.h\"\nint y = f() + f();\n";
/// `f` in `h.h`, `a.cc` (line 2, column 9) and `b.cc` (twice).
const F_H: u64 = 4;
const F_A: u64 = 23;
const F_B: [u64; 2] = [23, 29];

/// The subject every shard names.
const K: Digest = Digest([0x4b; 32]);

const ROUND: u64 = 1;

// ---------------------------------------------------------------------------
// The fixture

struct Fx {
    root: PathBuf,
}

impl Drop for Fx {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

/// What a finished `slip` did.
#[derive(Debug)]
struct Out {
    status: ExitStatus,
    stdout: String,
    stderr: String,
}

impl Out {
    fn code(&self) -> Option<i32> {
        self.status.code()
    }

    fn lines(&self) -> Vec<&str> {
        self.stdout.lines().collect()
    }

    /// The value of the stdout line `<key> <value>`.
    fn value(&self, key: &str) -> Option<&str> {
        self.stdout.lines().find_map(|l| l.strip_prefix(key)?.strip_prefix(' '))
    }

    fn digest(&self, key: &str) -> Digest {
        let hex = self.value(key).unwrap_or_else(|| panic!("no `{key}` line: {self:?}"));
        let hex = hex.split(':').next().unwrap();
        Digest::from_hex(hex).unwrap_or_else(|| panic!("`{key}` {hex} is not a digest"))
    }

    #[track_caller]
    fn expect(&self, code: i32) -> &Out {
        assert_eq!(self.code(), Some(code), "{self:#?}");
        self
    }
}

fn run_bounded(mut cmd: Command) -> Out {
    cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn().expect("start slip");
    let stdout = reader(child.stdout.take().unwrap());
    let stderr = reader(child.stderr.take().unwrap());
    let status = wait_bounded(&mut child);
    Out { status, stdout: stdout.join().unwrap(), stderr: stderr.join().unwrap() }
}

fn reader(mut r: impl Read + Send + 'static) -> std::thread::JoinHandle<String> {
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = r.read_to_end(&mut bytes);
        String::from_utf8_lossy(&bytes).into_owned()
    })
}

fn wait_bounded(child: &mut Child) -> ExitStatus {
    let deadline = Instant::now() + BOUND;
    let mut pause = Duration::from_millis(1);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            return status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("slip did not finish within {BOUND:?}");
        }
        std::thread::sleep(pause);
        pause = (pause * 2).min(Duration::from_millis(25));
    }
}

/// `slip <args>` outside any fixture (usage errors, `--help`).
fn slip(args: &[&str]) -> Out {
    let mut cmd = Command::new(SLIP);
    cmd.args(args);
    run_bounded(cmd)
}

fn file_ref(path: &[u8], bytes: &[u8]) -> FileRef {
    FileRef { path: path.to_vec(), size: bytes.len() as u64, content: content_digest(bytes) }
}

fn content(bytes: &[u8]) -> Content {
    Content { size: bytes.len() as u64, content: content_digest(bytes) }
}

/// The pre-image on disk, for `slip_apply::build`.
struct Disk;

impl slip_apply::Files for Disk {
    fn read(&self, path: &[u8]) -> std::io::Result<Vec<u8>> {
        fs::read(std::ffi::OsStr::from_bytes(path))
    }

    fn identity(&self, path: &[u8]) -> std::io::Result<Option<(u64, u64)>> {
        slip_apply::Files::identity(&slip_cli::transform::store::DiskFiles, path)
    }
}

/// A claim of one observer: `path`, `[lo, lo + 1)`, its stance, the
/// replacement `text`.
#[derive(Clone)]
struct C {
    path: &'static str,
    lo: u64,
    stance: Stance,
    text: &'static str,
    reason: Reason,
}

fn put(path: &'static str, lo: u64, text: &'static str) -> C {
    C { path, lo, stance: Stance::Replace, text, reason: Reason::None }
}

fn keep(path: &'static str, lo: u64) -> C {
    C { path, lo, stance: Stance::Keep, text: "", reason: Reason::None }
}

/// One observer's map shard, by path.
#[derive(Clone)]
struct S {
    claims: Vec<C>,
    reports: Vec<ShardReport>,
    /// WP-139: `(path, lo, spec, display, binds the subject)`.
    instances: Vec<(&'static str, u64, Option<Digest>, &'static str, bool)>,
    extra_subjects: Vec<Subject>,
    failed: bool,
    transform: Option<Digest>,
    edits: bool,
}

impl S {
    fn new(claims: Vec<C>) -> S {
        S {
            claims,
            reports: Vec::new(),
            instances: Vec::new(),
            extra_subjects: Vec::new(),
            failed: false,
            transform: None,
            edits: true,
        }
    }
}

impl Fx {
    fn new(name: &str) -> Fx {
        Fx::new_in(&common::work_root(), name)
    }

    /// A fixture under `base`: the work root, or the folding root for a
    /// test about spelling on a volume that folds (`common::folding_root`).
    fn new_in(base: &Path, name: &str) -> Fx {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let root = base.join(format!("wp075-{name}-{}-{n}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("src")).unwrap();
        // Real paths: the store, the shards and the overlay name them.
        let fx = Fx { root: fs::canonicalize(&root).unwrap() };
        fx.reset_tree();
        fx
    }

    /// Writes the tree of [`H`], [`A`], [`B`].
    fn reset_tree(&self) {
        for (name, text) in [("h.h", H), ("a.cc", A), ("b.cc", B)] {
            fs::write(self.src(name), text).unwrap();
        }
    }

    fn src(&self, name: &str) -> PathBuf {
        self.root.join("src").join(name)
    }

    fn real(&self, name: &str) -> Vec<u8> {
        self.src(name).as_os_str().as_bytes().to_vec()
    }

    fn read(&self, name: &str) -> String {
        fs::read_to_string(self.src(name)).unwrap()
    }

    fn store_dir(&self) -> PathBuf {
        self.root.join("store")
    }

    /// The store, opened shared (no lock, no journal check).
    fn store(&self) -> Store {
        Store::open(&self.store_dir(), Access::Shared).unwrap()
    }

    /// `slip <args> --store <store>`, run in the fixture's directory.
    fn slip(&self, args: &[&str]) -> Out {
        let mut cmd = Command::new(SLIP);
        cmd.args(args).arg("--store").arg(self.store_dir()).current_dir(&self.root);
        run_bounded(cmd)
    }

    fn owned(&self) -> Ownership {
        Ownership {
            owned: vec![self.root.join("src").as_os_str().as_bytes().to_vec()],
            readonly: vec![],
            rebuilt: Vec::new(),
        }
    }

    /// A rename's request (indivisible) owning `src/`.
    fn rename(&self) -> Transform {
        Transform {
            kind: TransformKind::Rename,
            ownership: self.owned(),
            indivisible: true,
            rename: Some(Rename {
                target: vec![b"f".to_vec()],
                kind: TargetKind::Any,
                new_name: b"g".to_vec(),
                dependent: slip_model::transform::Dependent::Offer,
            }),
            script: None,
        }
    }

    /// A programmable transform's request (divisible) owning `src/`.
    fn script(&self) -> Transform {
        Transform {
            kind: TransformKind::Script,
            ownership: self.owned(),
            indivisible: false,
            rename: None,
            script: Some(Script {
                id: Digest([0x5c; 32]),
                file: self.root.join("t.cc").as_os_str().as_bytes().to_vec(),
                options: Vec::new(),
                steps: 1 << 30,
                bytes: 1 << 30,
                depth: 512,
            }),
        }
    }

    /// The plan: one observer per main file, `a.cc` then `b.cc`.
    fn plan(&self) -> (Plan, Digest) {
        let mut observers: Vec<Observer> = [("a.cc", A, 0), ("b.cc", B, 1)]
            .into_iter()
            .map(|(name, text, entry)| {
                let command = Cmd {
                    directory: self.root.as_os_str().as_bytes().to_vec(),
                    argv: vec![b"c++".to_vec(), b"-c".to_vec(), self.real(name)],
                    env: vec![],
                };
                Observer {
                    id: command.observer_id(),
                    command,
                    language: Language::Cxx,
                    main: file_ref(&self.real(name), text.as_bytes()),
                    cost_hint: 1,
                    origins: vec![Origin { configuration: 0, entry }],
                }
            })
            .collect();
        observers.sort_by_key(|o| o.id);
        let plan = Plan {
            tool_slip: "s".into(),
            tool_kiln: slip_kiln::kiln_version().into(),
            configurations: vec![Configuration {
                name: "default".into(),
                database: FileRef {
                    path: b"/cdb.json".to_vec(),
                    size: 1,
                    content: Digest([1; 32]),
                },
            }],
            observers,
            excluded: vec![],
        };
        let id = self.store().publish_plan(&plan.to_bytes()).unwrap();
        (plan, id)
    }

    /// The observer whose main file is `name`.
    fn observer(plan: &Plan, name: &str) -> Digest {
        plan.observers
            .iter()
            .find(|o| o.main.path.ends_with(format!("/{name}").as_bytes()))
            .unwrap()
            .id
    }

    /// The shard of `observer` for round `round` of the request `request`.
    #[allow(clippy::too_many_lines, reason = "one record, field by field")]
    fn shard(&self, observer: Digest, round: u64, request: Digest, s: &S) -> Shard {
        let mut names: Vec<&str> = s.claims.iter().map(|c| c.path).collect();
        names.extend(s.reports.iter().filter_map(|r| r.0.map(|p| p.0)));
        names.extend(s.instances.iter().map(|i| i.0));
        names.sort_unstable();
        names.dedup();
        if s.failed {
            names.clear();
        }
        let text = |n: &str| match n {
            "h.h" => H,
            "a.cc" => A,
            "b.cc" => B,
            _ => unreachable!(),
        };
        let files: Vec<FileRef> =
            names.iter().map(|n| file_ref(&self.real(n), text(n).as_bytes())).collect();
        let index = |n: &str| names.iter().position(|m| *m == n).unwrap() as u64;
        let mut claims: Vec<Claim> = s
            .claims
            .iter()
            .map(|c| Claim {
                file: index(c.path),
                lo: c.lo,
                hi: c.lo + 1,
                stance: c.stance,
                pieces: if c.stance == Stance::Replace {
                    vec![Piece::Literal(c.text.as_bytes().to_vec())]
                } else {
                    vec![]
                },
                subjects: vec![K],
                reason: c.reason,
                tiers: tiers::T0,
                macro_site: None,
                votes: 1,
                notes: vec![],
            })
            .collect();
        claims.sort_by(Claim::cmp_key);
        let mut reports: Vec<Report> = s
            .reports
            .iter()
            .map(|(place, severity, message)| Report {
                place: place.map(|(n, lo)| Place { file: index(n), lo, hi: lo + 1 }),
                severity: *severity,
                message: message.clone(),
                attached: false,
            })
            .collect();
        reports.sort();
        let failure = s.failed.then(|| Failure {
            kind: FailureKind::Uncaught,
            message: "the transform threw".to_owned(),
            place: Some(SourcePlace { file: b"/t.cc".to_vec(), line: 1, column: 1 }),
            rendered: "the transform threw".to_owned(),
            refusal: None,
        });
        let edits = Edits {
            scheme: SCHEME,
            transform: s.transform.unwrap_or(request),
            failure,
            files,
            subjects: if s.failed {
                vec![]
            } else {
                let mut subjects = vec![Subject {
                    id: K,
                    rule: None,
                    role: Role::Seed,
                    of: vec![],
                    flags: 0,
                    display: "f".to_owned(),
                }];
                subjects.extend(s.extra_subjects.iter().cloned());
                subjects.sort_by_key(|s| s.id);
                subjects
            },
            links: vec![],
            claims: if s.failed { vec![] } else { claims },
            groups: vec![],
            reports: if s.failed { vec![] } else { reports },
            changed: vec![],
            renamed: vec![],
            instances: if s.failed {
                vec![]
            } else {
                let mut v: Vec<Instance> = s
                    .instances
                    .iter()
                    .map(|&(n, lo, spec, display, subject)| Instance {
                        place: Place { file: index(n), lo, hi: lo + 1 },
                        spec,
                        display: display.to_owned(),
                        subjects: if subject { vec![K] } else { vec![] },
                    })
                    .collect();
                v.sort();
                v
            },
        };
        let mut inputs: Vec<FileRef> = ["h.h", "a.cc", "b.cc"]
            .into_iter()
            .map(|n| file_ref(&self.real(n), text(n).as_bytes()))
            .collect();
        inputs.sort_by(|a, b| a.path.cmp(&b.path));
        Shard {
            observer,
            phase: Phase::Map,
            round,
            outcome: Outcome { status: Status::Ok, errors: 0, warnings: 0, message: String::new() },
            inputs: Some(inputs),
            counters: vec![],
            digests: None,
            verdict: None,
            edits: s.edits.then_some(edits),
            binding: None,
            rule_use: None,
            spellings: None,
            names: None,
            negatives: None,
            found: None,
            rebuilt_use: None,
        }
    }

    /// Publishes round `round` of the plan: the request (unless round 0),
    /// one map shard per `(main file, shard)`, the map run record naming
    /// those observers and, for a transform's round, the transform run
    /// record. Returns the request id.
    fn round(
        &self,
        plan: &(Plan, Digest),
        round: u64,
        request: &Transform,
        shards: &[(&str, S)],
    ) -> Digest {
        let store = self.store();
        let request_id = store.publish_object(Object::Transform, &request.to_bytes()).unwrap();
        let mut jobs = Vec::new();
        for (name, s) in shards {
            let o = Fx::observer(&plan.0, name);
            let shard = self.shard(o, round, request_id, s);
            store.publish_shard(&plan.1, Phase::Map, round, &o, &shard.to_bytes()).unwrap();
            jobs.push(Job {
                observer: o,
                attempt: 1,
                end: End::Completed,
                shard: None,
                queued_ns: 0,
                wall_ns: 0,
                cpu_ns: 0,
                peak_rss: 0,
                estimate: 0,
            });
        }
        let started = 1_000 + round;
        let record = RunRecord {
            run: Run {
                tool_slip: "s".into(),
                tool_kiln: slip_kiln::kiln_version().into(),
                plan: plan.1,
                phase: Phase::Map,
                round,
                started_unix_ns: started,
                wall_ns: 1,
                cores: 1,
                workers_max: 1,
                memory_budget: 0,
                peak_rss_sum: 0,
            },
            jobs,
        };
        store.publish_run(&plan.1, Phase::Map, round, started, &record.to_bytes()).unwrap();
        if round != 0 {
            let runt = TransformRun {
                tool_slip: "s".into(),
                tool_kiln: slip_kiln::kiln_version().into(),
                plan: plan.1,
                round,
                transform: request_id,
                edits: None,
                declared: None,
                overlay: None,
                result: None,
                started_unix_ns: started,
                wall_ns: 1,
                steps: vec![],
            };
            store.publish_transform_run(&plan.1, round, started, &runt.to_bytes()).unwrap();
        }
        request_id
    }

    fn reduce(&self, plan: Digest, round: u64, extra: &[&str]) -> Out {
        let (p, r) = (plan.to_string(), round.to_string());
        let mut args = vec!["reduce", "--plan", &p, "--round", &r];
        args.extend_from_slice(extra);
        self.slip(&args)
    }

    fn edit_set(&self, id: Digest) -> EditSet {
        EditSet::from_bytes(&fs::read(self.store().object_path(Object::EditSet, &id)).unwrap())
            .unwrap()
    }

    /// The overlay of edit set `id` (§7.1), published with its blobs and
    /// the pre-image's.
    fn overlay(&self, set_id: Digest) -> (Digest, Overlay) {
        let set = self.edit_set(set_id);
        let built = slip_apply::build(&set, set_id, &Disk).unwrap();
        let store = self.store();
        for (_, bytes) in &built.blobs {
            store.publish_blob(bytes).unwrap();
        }
        let id = store.publish_object(Object::Overlay, &built.overlay.to_bytes()).unwrap();
        (id, built.overlay)
    }

    /// A result of `state` for edit set `set_id` and its overlay, made as
    /// conclude would make it: every selected observer verified, the
    /// tally's map counts the set's.
    fn result(&self, set_id: Digest, overlay: Option<Digest>, state: State) -> TransformResult {
        let set = self.edit_set(set_id);
        let declared = Declared::none();
        let decl = self.store().publish_declared(&declared.to_bytes()).unwrap();
        let unparsed = set.unparsed.len() as u64;
        let refused = state == State::Refused;
        let verdict = match state {
            State::Verified | State::VerifiedWithResidue => Some(RunVerdict::Verified),
            State::NotVerified => Some(RunVerdict::NotVerified),
            State::Incomplete | State::Refused => None,
        };
        let parsed = 2 - unparsed;
        let tally = Tally {
            selected: 2,
            unparsed,
            failed_runs: set.failures.iter().map(|f| f.observers).sum(),
            error_reports: set
                .reports
                .iter()
                .filter(|r| r.report.severity == Severity::Error)
                .count() as u64,
            verified: match verdict {
                Some(RunVerdict::Verified) => parsed,
                Some(_) => parsed - 1,
                None => 0,
            },
            mismatch: u64::from(verdict == Some(RunVerdict::NotVerified)),
            ..Tally::default()
        };
        TransformResult {
            scheme: slip_model::result::SCHEME,
            plan: set.plan,
            round: set.round,
            transform: set.transform,
            edits: set_id,
            declared: (!refused).then_some(decl),
            overlay: if refused { None } else { overlay },
            state,
            partial: false,
            allow_unparsed: false,
            verdict,
            tally,
            agreement: vec![],
            unused: vec![],
            spellings: Spellings::default(),
            residue: vec![],
            exempt: vec![],
        }
    }

    fn publish_result(&self, r: &TransformResult) -> String {
        r.validate().unwrap();
        self.store().publish_object(Object::Result, &r.to_bytes()).unwrap().to_string()
    }

    /// Publishes the plan and round 1 of `request` over `shards`, runs
    /// `reduce`, and returns the plan id, the edit set id and the output.
    fn reduced(&self, request: &Transform, shards: &[(&str, S)]) -> (Digest, Digest, Out) {
        let plan = self.plan();
        self.round(&plan, ROUND, request, shards);
        let out = self.reduce(plan.1, ROUND, &[]);
        let set = out.digest("edits");
        (plan.1, set, out)
    }

    /// The post-image of the agreed rename: every `f` is `g`.
    fn post(text: &str) -> String {
        text.replace("f()", "g()").replace("int f", "int g")
    }

    fn tree_is_pre(&self) -> bool {
        self.read("h.h") == H && self.read("a.cc") == A && self.read("b.cc") == B
    }

    fn tree_is_post(&self) -> bool {
        self.read("h.h") == Fx::post(H)
            && self.read("a.cc") == Fx::post(A)
            && self.read("b.cc") == Fx::post(B)
    }

    fn put_journal(&self, bytes: &[u8]) {
        fs::write(self.store().journal_path(), bytes).unwrap();
    }
}

/// The shards of two observers that agree on every `f`: both edit the
/// header, each its main file.
fn agreeing() -> Vec<(&'static str, S)> {
    vec![
        ("a.cc", S::new(vec![put("h.h", F_H, "g"), put("a.cc", F_A, "g")])),
        (
            "b.cc",
            S::new(vec![put("h.h", F_H, "g"), put("b.cc", F_B[0], "g"), put("b.cc", F_B[1], "g")]),
        ),
    ]
}

// ---------------------------------------------------------------------------
// reduce (§10.2, §10.5)

/// §10.5: two observers that agree: the summary lines, the edit set and
/// its declared changes published, status 0.
#[test]
fn s10_5_reduce_two_observers_agree() {
    let fx = Fx::new("agree");
    let (_, set_id, out) = fx.reduced(&fx.rename(), &agreeing());
    out.expect(0);
    let decl = fx.store().read_declared(&out.digest("declared")).unwrap().expect("DECL published");
    let decl_id = content_id(&decl).unwrap();
    assert_eq!(
        out.lines(),
        vec![
            format!("edits {set_id}"),
            "state accepted".to_owned(),
            "edit 4 in 3 file(s); shared 1; most observers 2; nested 0".to_owned(),
            "kept 0; unresolved 0".to_owned(),
            format!("declared {decl_id}: rules 0, exempt 0"),
        ],
        "{out:#?}"
    );
    let set = fx.edit_set(set_id);
    assert_eq!(set.edits.len(), 4);
    assert!(out.stderr.is_empty(), "{out:#?}");
}

/// §10.5: a divisible request prints the `groups` line.
#[test]
fn s10_5_reduce_groups_line_of_a_divisible_request() {
    let fx = Fx::new("groups");
    let (_, _, out) = fx.reduced(&fx.script(), &agreeing());
    out.expect(0);
    assert!(out.lines().contains(&"groups 4, rejected 0"), "{out:#?}");
}

/// §10.5: a contested site under an indivisible request: the set is
/// refused (status 1), the `refused` count and the `refusal` line with
/// its line and column computed from the file on disk.
#[test]
fn s10_5_reduce_contested_site() {
    let fx = Fx::new("contested");
    let shards = vec![
        ("a.cc", S::new(vec![put("h.h", F_H, "g"), put("a.cc", F_A, "g")])),
        ("b.cc", S::new(vec![keep("h.h", F_H), put("b.cc", F_B[0], "g")])),
    ];
    let (plan, _, out) = fx.reduced(&fx.rename(), &shards);
    out.expect(1);
    assert_eq!(out.value("state"), Some("refused"));
    assert!(out.lines().contains(&"refused contested 1"), "{out:#?}");
    assert!(out.value("declared").is_none(), "no declared changes for a refused set");
    let store = fx.store();
    let plan = Plan::from_bytes(&store.read_plan(&plan).unwrap().unwrap()).unwrap();
    // The refusal counts the observers of the claim that refuses (the
    // `keep` of `b.cc`'s observer), as reduce records it.
    let set = fx.edit_set(out.digest("edits"));
    assert_eq!(set.refusals.len(), 1);
    assert_eq!(set.refusals[0].first, vec![Fx::observer(&plan, "b.cc")]);
    let (h, b) = (fx.src("h.h"), fx.src("b.cc"));
    let want =
        format!("refusal contested: {}:1:5: f (1 observer(s), first {})", h.display(), b.display());
    assert!(out.lines().contains(&want.as_str()), "want {want:?}: {out:#?}");
}

/// §10.5 (WP-139 item 3): the names edited, or refused, on the evidence of
/// instantiations, under their heading, with each specialisation and its
/// observers; a contested site keeps the instances both observers made. In
/// a golden, the source directory written `<src>`.
#[test]
fn s10_5_reduce_instance_lines() {
    let fx = Fx::new("instances");
    let d = |n: u8| Digest([n; 32]);
    let mut a = S::new(vec![put("h.h", F_H, "g"), put("a.cc", F_A, "g")]);
    a.instances = vec![
        ("h.h", F_H, Some(d(0x30)), "call<n::S<int>>", true),
        ("a.cc", F_A, Some(d(0x31)), "call<n::S<long>>", true),
        ("a.cc", F_A, None, "call<(lambda)>", true),
    ];
    let mut b = S::new(vec![keep("h.h", F_H), put("b.cc", F_B[0], "g")]);
    b.instances = vec![("h.h", F_H, Some(d(0x32)), "call<n::R>", false)];
    let (_, set_id, out) = fx.reduced(&fx.script(), &[("a.cc", a), ("b.cc", b)]);
    out.expect(0);
    let set = fx.edit_set(set_id);
    assert_eq!(set.instances.len(), 4, "{:?}", set.instances);
    let src = fx.src("h.h");
    let src = src.parent().unwrap().display().to_string();
    let lines: Vec<String> = out
        .lines()
        .into_iter()
        .filter(|l| l.starts_with("instance"))
        .map(|l| l.replace(&src, "<src>"))
        .collect();
    golden("reduce-instances.txt", &(lines.join("\n") + "\n"));
}

/// A golden of `crates/slip_cli/tests/golden/`; `SLIP_BLESS=1` rewrites it.
fn golden(name: &str, actual: &str) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden").join(name);
    if std::env::var_os("SLIP_BLESS").is_some() {
        fs::write(&path, actual).unwrap();
        return;
    }
    let expected = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{}: {e}; run with SLIP_BLESS=1 to create", path.display()));
    assert_eq!(actual, expected, "{name} differs from its golden; SLIP_BLESS=1 rewrites it");
}

/// §10.5: a file changed since the shards were made: the detail line
/// carries the byte offset instead of a line and column.
#[test]
fn s10_5_reduce_offset_when_the_file_changed() {
    let fx = Fx::new("offset");
    let shards = vec![
        ("a.cc", S::new(vec![put("h.h", F_H, "g")])),
        ("b.cc", S::new(vec![keep("h.h", F_H)])),
    ];
    let plan = fx.plan();
    fx.round(&plan, ROUND, &fx.rename(), &shards);
    fs::write(fx.src("h.h"), "changed").unwrap();
    let out = fx.reduce(plan.1, ROUND, &[]);
    out.expect(1);
    let want = format!(":@{F_H}: f (1 observer(s)");
    assert!(
        out.lines().iter().any(|l| l.starts_with("refusal contested: ") && l.contains(&want)),
        "{out:#?}"
    );
}

/// §5.7, §10.5: a shard of another request is an input error (status 3)
/// and nothing is published.
#[test]
fn s10_5_reduce_shard_of_another_request() {
    let fx = Fx::new("other-request");
    let plan = fx.plan();
    // The second shard in plan order names another request: the first
    // names the round's.
    let second = plan.0.observers[1].id;
    let mut shards = agreeing();
    for (name, s) in &mut shards {
        if Fx::observer(&plan.0, name) == second {
            s.transform = Some(Digest([0x77; 32]));
        }
    }
    fx.round(&plan, ROUND, &fx.rename(), &shards);
    let out = fx.reduce(plan.1, ROUND, &[]);
    out.expect(3);
    assert!(out.stdout.is_empty(), "{out:#?}");
    assert!(
        out.stderr.starts_with("slip: ") && out.stderr.contains("map shard of observer"),
        "{out:#?}"
    );
    assert!(fx.store().find_object(Object::EditSet, "").unwrap().is_empty());
}

/// §10.2: round 0 is `parse`'s, not a transform's: status 3.
#[test]
fn s10_2_reduce_round_of_parse() {
    let fx = Fx::new("parse-round");
    let mut shards = agreeing();
    for s in &mut shards {
        s.1.edits = false;
    }
    let plan = fx.plan();
    fx.round(&plan, 0, &fx.rename(), &shards);
    let out = fx.reduce(plan.1, 0, &[]);
    out.expect(3);
    assert!(out.stderr.contains("not a transform's round"), "{out:#?}");
    // A round with no map run record at all.
    let out = fx.reduce(plan.1, 7, &[]);
    out.expect(3);
    assert!(out.stderr.contains("no map run record"), "{out:#?}");
}

/// §5.3, §5.6: the edit set depends on neither `-j` nor the run: two runs,
/// with `-j 1` and `-j 4`, give the same ids and the same bytes on
/// standard output.
#[test]
fn s5_3_reduce_is_deterministic_across_j() {
    let fx = Fx::new("determinism");
    let plan = fx.plan();
    let mut shards = agreeing();
    shards[0].1.reports.push((Some(("a.cc", F_A)), Severity::Note, b"a note".to_vec()));
    fx.round(&plan, ROUND, &fx.script(), &shards);
    let one = fx.reduce(plan.1, ROUND, &["-j", "1"]);
    let four = fx.reduce(plan.1, ROUND, &["-j4"]);
    one.expect(0);
    four.expect(0);
    assert_eq!(one.stdout, four.stdout);
    assert_eq!(one.digest("edits"), four.digest("edits"));
}

/// §10.5: a report's message is printed with control characters as
/// spaces, at its line and column; at most 200 lines of a kind, then
/// `<kind>: <n> more (slip dump <file>)`.
#[test]
fn s10_5_reduce_report_lines_and_the_cut() {
    let fx = Fx::new("reports");
    let mut shards = agreeing();
    shards[0].1.reports.push((Some(("a.cc", F_A)), Severity::Error, b"bad\x01thing".to_vec()));
    // After it in the record's order (by place: `a.cc` before `h.h`).
    for i in 0..201 {
        shards[1].1.reports.push((
            Some(("h.h", F_H)),
            Severity::Note,
            format!("note {i:03}").into_bytes(),
        ));
    }
    let (_, set_id, out) = fx.reduced(&fx.script(), &shards);
    out.expect(0);
    assert!(out.lines().contains(&"reports: note 201, review 0, error 1"), "{out:#?}");
    let reports: Vec<&str> = out.lines().into_iter().filter(|l| l.starts_with("report ")).collect();
    assert_eq!(reports.len(), 200);
    let a = fx.src("a.cc");
    let want = format!(
        "report error: {}:2:9: bad thing (1 observer(s), first {})",
        a.display(),
        a.display()
    );
    assert!(reports.contains(&want.as_str()), "want {want:?}: {reports:#?}");
    let record = fx.store().object_path(Object::EditSet, &set_id);
    assert_eq!(
        out.lines().last().copied(),
        Some(format!("report: 2 more (slip dump {})", record.display()).as_str())
    );
}

// ---------------------------------------------------------------------------
// apply --result (§7.2, §7.3, §8.5 permits, §10.5)

/// A fixture with the agreed rename reduced, its overlay published.
fn agreed(name: &str, request: fn(&Fx) -> Transform) -> (Fx, Digest, Digest) {
    agreed_in(&common::work_root(), name, request)
}

/// [`agreed`] under `base`.
fn agreed_in(base: &Path, name: &str, request: fn(&Fx) -> Transform) -> (Fx, Digest, Digest) {
    let fx = Fx::new_in(base, name);
    let (_, set_id, out) = fx.reduced(&request(&fx), &agreeing());
    out.expect(0);
    let (overlay, _) = fx.overlay(set_id);
    (fx, set_id, overlay)
}

fn apply_write(fx: &Fx, result: &str, allow: &[&str]) -> Out {
    let mut args = vec!["apply", "--result", result, "--write"];
    args.extend_from_slice(allow);
    fx.slip(&args)
}

fn apply_patch(fx: &Fx, result: &str, patch: &Path, allow: &[&str]) -> Out {
    let p = patch.to_str().unwrap();
    let mut args = vec!["apply", "--result", result, "--patch", p];
    args.extend_from_slice(allow);
    fx.slip(&args)
}

/// §7.3, §10.5: a verified result written: the tree is the post-image,
/// `written 3 file(s)`, status 0.
#[test]
fn s7_3_apply_verified_write() {
    let (fx, _, overlay) = agreed("write", Fx::rename);
    let r = fx.publish_result(&fx.result(fx.edit_set_id(), Some(overlay), State::Verified));
    let out = apply_write(&fx, &r[..12], &[]);
    out.expect(0);
    assert_eq!(
        out.lines(),
        vec![format!("result {r}"), "state verified".to_owned(), "written 3 file(s)".to_owned()]
    );
    assert!(fx.tree_is_post());
    assert!(!fx.store().journal_path().exists());
}

impl Fx {
    /// The one edit set of the store.
    fn edit_set_id(&self) -> Digest {
        let ids = self.store().find_object(Object::EditSet, "").unwrap();
        assert_eq!(ids.len(), 1);
        ids[0]
    }
}

/// §7.2: the patch of a verified result applies with `patch -p0` to a
/// copy of the pre-image and gives the post-image; `--relative-to`
/// names the files relative to it; the tree is untouched.
#[test]
fn s7_2_apply_verified_patch() {
    let (fx, set, overlay) = agreed("patch", Fx::rename);
    let r = fx.publish_result(&fx.result(set, Some(overlay), State::Verified));
    let patch = fx.root.join("out.patch");
    let root = fx.root.to_str().unwrap().to_owned();
    let out = apply_patch(&fx, &r, &patch, &["--relative-to", &root]);
    out.expect(0);
    assert_eq!(out.value("patch"), Some(patch.to_str().unwrap()));
    assert!(fx.tree_is_pre(), "a patch does not touch the tree");
    let text = fs::read_to_string(&patch).unwrap();
    assert!(text.starts_with("--- src/a.cc\n+++ src/a.cc\n"), "{text}");
    // `patch -p0` from a copy of the tree.
    let copy = fx.root.join("copy");
    fs::create_dir_all(copy.join("src")).unwrap();
    for (n, t) in [("h.h", H), ("a.cc", A), ("b.cc", B)] {
        fs::write(copy.join("src").join(n), t).unwrap();
    }
    let status = Command::new("patch")
        .args(["-p0", "-s", "-i"])
        .arg(&patch)
        .current_dir(&copy)
        .status()
        .expect("run patch");
    assert!(status.success());
    for (n, t) in [("h.h", H), ("a.cc", A), ("b.cc", B)] {
        assert_eq!(fs::read_to_string(copy.join("src").join(n)).unwrap(), Fx::post(t), "{n}");
    }
    // Without `--relative-to`: real paths.
    let out = apply_patch(&fx, &r, &patch, &[]);
    out.expect(0);
    let text = fs::read_to_string(&patch).unwrap();
    let a = fx.src("a.cc");
    assert!(text.contains(&format!("--- {}\n", a.display())), "{text}");
    // §7.2: an overlay file outside `--relative-to` is a usage error.
    let elsewhere = fx.root.join("elsewhere");
    fs::create_dir_all(&elsewhere).unwrap();
    let out = apply_patch(&fx, &r, &patch, &["--relative-to", elsewhere.to_str().unwrap()]);
    out.expect(2);
    assert!(out.stderr.contains("not inside `--relative-to`"), "{out:#?}");
}

/// §8.5 through `apply`: each reason of a `verified_with_residue` needs
/// its own option; a `partial` result needs `--allow-partial`; no option
/// permits another state. Refused means status 1, nothing written, no
/// patch file; permitted means written, status 0 whatever the state the
/// options permitted (§10.5: the status says what apply did).
#[test]
#[allow(clippy::too_many_lines, reason = "one table of cases")]
fn s8_5_apply_permits_table() {
    // (what, request, shards, how the result is made, options that are
    // not enough, options that are).
    type Shape = fn(&mut TransformResult, &Fx);
    struct Case {
        what: &'static str,
        request: fn(&Fx) -> Transform,
        shards: fn() -> Vec<(&'static str, S)>,
        state: State,
        shape: Shape,
        short: &'static [&'static str],
        enough: Option<&'static [&'static str]>,
    }
    fn none(_: &mut TransformResult, _: &Fx) {}
    fn residue(r: &mut TransformResult, fx: &Fx) {
        let file = fx.real("a.cc");
        r.residue.push(Residue {
            category: Category::Dependent,
            file,
            lo: 0,
            hi: 1,
            line: 1,
            column: 1,
        });
    }
    fn partial(r: &mut TransformResult, _: &Fx) {
        r.partial = true;
    }
    fn rejected_group() -> Vec<(&'static str, S)> {
        // Divisible: the contested edit in `b.cc` is a group of its own,
        // rejected; the others stand.
        let mut s = agreeing();
        s[0].1.claims.push(keep("b.cc", F_B[1]));
        s
    }
    fn failed_run() -> Vec<(&'static str, S)> {
        let mut s = agreeing();
        s[1].1.failed = true;
        s[1].1.claims.clear();
        s
    }
    fn error_report() -> Vec<(&'static str, S)> {
        let mut s = agreeing();
        s[0].1.reports.push((Some(("a.cc", F_A)), Severity::Error, b"an error".to_vec()));
        s
    }
    let all: &[&str] = &["--allow-residue", "--allow-errors", "--allow-partial"];
    let cases = [
        Case {
            what: "residue",
            request: Fx::rename,
            shards: agreeing,
            state: State::VerifiedWithResidue,
            shape: residue,
            short: &["--allow-errors", "--allow-partial"],
            enough: Some(&["--allow-residue"]),
        },
        Case {
            what: "rejected group",
            request: Fx::script,
            shards: rejected_group,
            state: State::VerifiedWithResidue,
            shape: none,
            short: &["--allow-errors"],
            enough: Some(&["--allow-residue"]),
        },
        Case {
            what: "failed run",
            request: Fx::script,
            shards: failed_run,
            state: State::VerifiedWithResidue,
            shape: none,
            short: &["--allow-residue", "--allow-partial"],
            enough: Some(&["--allow-errors"]),
        },
        Case {
            what: "error report",
            request: Fx::script,
            shards: error_report,
            state: State::VerifiedWithResidue,
            shape: none,
            short: &["--allow-residue"],
            enough: Some(&["--allow-errors"]),
        },
        Case {
            what: "partial",
            request: Fx::rename,
            shards: agreeing,
            state: State::Verified,
            shape: partial,
            short: &["--allow-residue", "--allow-errors"],
            enough: Some(&["--allow-partial"]),
        },
        Case {
            what: "not verified",
            request: Fx::rename,
            shards: agreeing,
            state: State::NotVerified,
            shape: none,
            short: all,
            enough: None,
        },
        Case {
            what: "incomplete",
            request: Fx::rename,
            shards: agreeing,
            state: State::Incomplete,
            shape: none,
            short: all,
            enough: None,
        },
        Case {
            what: "refused",
            request: Fx::rename,
            shards: agreeing,
            state: State::Refused,
            shape: none,
            short: all,
            enough: None,
        },
    ];
    for case in cases {
        let fx = Fx::new("permits");
        let (_, set, out) = fx.reduced(&(case.request)(&fx), &(case.shards)());
        out.expect(0);
        let (overlay, _) = fx.overlay(set);
        let mut r = fx.result(set, Some(overlay), case.state);
        (case.shape)(&mut r, &fx);
        let id = fx.publish_result(&r);
        let patch = fx.root.join("p.patch");
        for refused in [&[][..], case.short] {
            let out = apply_write(&fx, &id, refused);
            out.expect(1);
            assert!(out.stderr.contains("does not permit `--write`"), "{}: {out:#?}", case.what);
            assert_eq!(out.value("state"), Some(case.state.name()), "{}", case.what);
            assert!(fx.tree_is_pre(), "{}", case.what);
            let out = apply_patch(&fx, &id, &patch, refused);
            out.expect(1);
            assert!(!patch.exists(), "{}", case.what);
        }
        if let Some(enough) = case.enough {
            let out = apply_patch(&fx, &id, &patch, enough);
            out.expect(0);
            assert!(patch.exists(), "{}", case.what);
            let out = apply_write(&fx, &id, enough);
            out.expect(0);
            assert!(out.value("written").is_some(), "{}: {out:#?}", case.what);
            assert!(!fx.tree_is_pre(), "{}", case.what);
        }
    }
}

/// §7.3 step 1, §7.2, §10.5: a file changed since the result: the write
/// and the patch are refused (`stale_file`, status 1), the tree is left
/// as it is and no patch is written.
#[test]
fn s7_3_apply_stale_file_refused() {
    let (fx, set, overlay) = agreed("stale", Fx::rename);
    let r = fx.publish_result(&fx.result(set, Some(overlay), State::Verified));
    fs::write(fx.src("b.cc"), "changed\n").unwrap();
    let out = apply_write(&fx, &r, &[]);
    out.expect(1);
    assert!(out.stderr.contains("refused stale_file: "), "{out:#?}");
    assert_eq!((fx.read("h.h").as_str(), fx.read("a.cc").as_str()), (H, A), "nothing replaced");
    assert_eq!(fx.read("b.cc"), "changed\n");
    assert!(!fx.store().journal_path().exists());
    let patch = fx.root.join("p.patch");
    let out = apply_patch(&fx, &r, &patch, &[]);
    out.expect(1);
    assert!(out.stderr.contains("refused stale_file: "), "{out:#?}");
    assert!(!patch.exists());
}

/// A result made by hand whose overlay has a file outside the request's
/// ownership, and one whose `post` is its `pre`: the write is refused
/// (`not_owned`) with nothing replaced; without the foreign file the
/// unchanged entry is neither written nor in the patch (§7.1).
#[test]
fn s7_1_apply_not_owned_and_unchanged_entries() {
    let (fx, set, _) = agreed("hand-made", Fx::rename);
    let store = fx.store();
    let built = slip_apply::build(&fx.edit_set(set), set, &Disk).unwrap();
    let mut overlay = built.overlay.clone();
    // `t.cc` beside `src/`: not owned. Its post-image is published.
    let outside = fx.root.join("t.cc");
    fs::write(&outside, "int t;\n").unwrap();
    let post = store.publish_blob(b"int u;\n").unwrap();
    let pre_t = content(b"int t;\n");
    let mut foreign = overlay.clone();
    foreign.files.push(OverlayFile {
        path: outside.as_os_str().as_bytes().to_vec(),
        pre: pre_t,
        post: Content { size: 7, content: post },
    });
    foreign.files.sort_by(|a, b| a.path.cmp(&b.path));
    let id = store.publish_object(Object::Overlay, &foreign.to_bytes()).unwrap();
    let r = fx.publish_result(&fx.result(set, Some(id), State::Verified));
    let out = apply_write(&fx, &r, &[]);
    out.expect(1);
    assert!(out.stderr.contains(&format!("refused not_owned: {}", outside.display())), "{out:#?}");
    assert!(fx.tree_is_pre());
    assert_eq!(fs::read_to_string(&outside).unwrap(), "int t;\n");

    // An entry whose post is its pre (`b.cc` is dropped from the edits'
    // files by giving it its own pre as post).
    let b = overlay.files.iter_mut().find(|f| f.path == fx.real("b.cc")).unwrap();
    b.post = b.pre;
    let id = store.publish_object(Object::Overlay, &overlay.to_bytes()).unwrap();
    let r = fx.publish_result(&fx.result(set, Some(id), State::Verified));
    let patch = fx.root.join("p.patch");
    apply_patch(&fx, &r, &patch, &[]).expect(0);
    let text = fs::read_to_string(&patch).unwrap();
    assert!(!text.contains("b.cc"), "{text}");
    assert!(text.contains("a.cc") && text.contains("h.h"), "{text}");
    let before = fs::metadata(fx.src("b.cc")).unwrap();
    let out = apply_write(&fx, &r, &[]);
    out.expect(0);
    assert!(out.lines().contains(&"written 2 file(s)"), "{out:#?}");
    let after = fs::metadata(fx.src("b.cc")).unwrap();
    assert_eq!(
        (before.ino(), before.mtime_nsec()),
        (after.ino(), after.mtime_nsec()),
        "b.cc not rewritten"
    );
    assert_eq!(fx.read("b.cc"), B);
    assert_eq!(fx.read("a.cc"), Fx::post(A));
}

/// §10.5: an id prefix that names no result, and one that is not hex.
#[test]
fn s10_5_apply_unknown_result() {
    let fx = Fx::new("unknown");
    fx.plan();
    let out = fx.slip(&["apply", "--result", "abcd", "--write"]);
    out.expect(3);
    assert!(out.stderr.contains("no result in the store"), "{out:#?}");
    slip(&["apply", "--result", "xyz1", "--write"]).expect(2);
}

// ---------------------------------------------------------------------------
// The journal (§7.3, §10.5)

/// §10.5: a store with a journal is refused with status 3 by `reduce` and
/// `apply --result`, with the message of `StoreError::UnfinishedWrite`.
#[test]
fn s10_5_journal_refused_by_reduce_and_apply() {
    let (fx, set, overlay) = agreed("journal-refused", Fx::rename);
    let r = fx.publish_result(&fx.result(set, Some(overlay), State::Verified));
    fx.put_journal(b"anything");
    let plan = fx.store().find_plan("").unwrap()[0];
    for out in [
        fx.reduce(plan, ROUND, &[]),
        apply_write(&fx, &r, &[]),
        apply_patch(&fx, &r, &fx.root.join("p"), &[]),
    ] {
        out.expect(3);
        assert!(out.stderr.contains("unfinished write of the working tree"), "{out:#?}");
        assert!(out.stderr.contains("run `slip apply --recover`"), "{out:#?}");
    }
    assert!(fx.tree_is_pre());
}

/// Review 2 of WP-161, finding 1: a store of version 1 (an earlier
/// Slip's) is refused, status 3, by `apply --recover` and by `reduce`;
/// when it holds `write.journal` the message says an unfinished write is
/// recorded there and to finish it with the earlier Slip's `apply
/// --recover`, then use a new store. Nothing in it is changed.
#[test]
fn s2_an_earlier_store_with_an_unfinished_write() {
    let (fx, _set, _overlay) = agreed("earlier-journal", Fx::rename);
    let plan = fx.store().find_plan("").unwrap()[0];
    fs::write(fx.store_dir().join("VERSION"), "1\n").unwrap();
    let journal = fx.store_dir().join("write.journal");
    for with_journal in [false, true] {
        if with_journal {
            fs::write(&journal, b"an earlier Slip's journal").unwrap();
        }
        for out in [fx.slip(&["apply", "--recover"]), fx.reduce(plan, ROUND, &[])] {
            out.expect(3);
            assert!(out.stderr.contains("store version 1: written by an earlier Slip"), "{out:#?}");
            let told =
                out.stderr.contains("an unfinished write of the working tree is recorded there")
                    && out.stderr.contains("`slip apply --recover` of the Slip that wrote it")
                    && out.stderr.contains(&journal.display().to_string());
            assert_eq!(told, with_journal, "{out:#?}");
            assert!(out.stderr.contains("use a new store"), "{out:#?}");
            assert!(!out.stdout.contains("recovered"), "{out:#?}");
        }
        assert_eq!(fs::read_to_string(fx.store_dir().join("VERSION")).unwrap(), "1\n");
        assert_eq!(journal.exists(), with_journal);
        assert!(fx.tree_is_pre());
    }
    assert_eq!(fs::read(&journal).unwrap(), b"an earlier Slip's journal");
}

/// The journal of a write of the agreed overlay, and the post-image
/// blobs, as `write` publishes them before it stages anything.
fn journal_of(fx: &Fx, overlay_id: Digest, overlay: &Overlay) -> Journal {
    let store = fx.store();
    for f in &overlay.files {
        store.publish_blob(&fs::read(std::ffi::OsStr::from_bytes(&f.path)).unwrap()).unwrap();
    }
    Journal {
        overlay: overlay_id,
        files: overlay
            .files
            .iter()
            .filter(|f| f.pre != f.post)
            .map(|f| JournalEntry {
                path: f.path.clone(),
                staged: staged_name(&f.path, overlay_id),
                pre: f.pre,
                post: f.post,
            })
            .collect(),
    }
}

/// §7.3: a tree left half written (one file replaced, one staged, one
/// untouched) is restored by `apply --recover`, which exits 0 and removes
/// the journal; a second run finds nothing to do.
#[test]
fn s7_3_recover_restores_a_half_written_tree() {
    let (fx, set, overlay_id) = agreed("recover", Fx::rename);
    let overlay = fx.store().read_object(Object::Overlay, &overlay_id).unwrap().unwrap();
    let overlay = Overlay::from_bytes(&overlay).unwrap();
    let j = journal_of(&fx, overlay_id, &overlay);
    fx.put_journal(&j.to_bytes());
    let _ = set;
    // `a.cc` replaced by its post-image, `b.cc`'s post-image staged.
    fs::write(fx.src("a.cc"), Fx::post(A)).unwrap();
    let staged_b = staged_name(&fx.real("b.cc"), overlay_id);
    fs::write(std::ffi::OsStr::from_bytes(&staged_b), Fx::post(B)).unwrap();
    let out = fx.slip(&["apply", "--recover"]);
    out.expect(0);
    assert_eq!(out.lines(), vec!["recovered: restored 1, left 2, reported 0"], "{out:#?}");
    assert!(fx.tree_is_pre());
    assert!(!Path::new(std::ffi::OsStr::from_bytes(&staged_b)).exists());
    assert!(!fx.store().journal_path().exists());
    let out = fx.slip(&["apply", "--recover"]);
    out.expect(0);
    assert_eq!(out.lines(), vec!["recovered: no unfinished write"]);
}

/// §7.3, §10.5: a file that is neither image is reported: status 3, the
/// file left as it is, the journal kept (and the store still refused).
#[test]
fn s7_3_recover_reports_and_keeps_the_journal() {
    let (fx, _, overlay_id) = agreed("recover-report", Fx::rename);
    let overlay = Overlay::from_bytes(
        &fx.store().read_object(Object::Overlay, &overlay_id).unwrap().unwrap(),
    )
    .unwrap();
    let j = journal_of(&fx, overlay_id, &overlay);
    fx.put_journal(&j.to_bytes());
    fs::write(fx.src("a.cc"), "someone else's save\n").unwrap();
    let out = fx.slip(&["apply", "--recover"]);
    out.expect(3);
    assert_eq!(out.lines(), vec!["recovered: restored 0, left 2, reported 1"], "{out:#?}");
    assert!(
        out.stderr.contains(&format!("slip: {}: neither", fx.src("a.cc").display())),
        "{out:#?}"
    );
    assert_eq!(fx.read("a.cc"), "someone else's save\n");
    assert!(fx.store().journal_path().exists());
    let plan = fx.store().find_plan("").unwrap()[0];
    fx.reduce(plan, ROUND, &[]).expect(3);
}

/// §7.3, landed WP-063 §7: the ownership `--recover` checks is the one of
/// the request the journal's overlay leads to; a journal that names a file
/// it does not own is refused (status 3) with nothing changed.
#[test]
fn s7_3_recover_refuses_a_journal_not_owned() {
    let (fx, set, overlay_id) = agreed("recover-not-owned", Fx::rename);
    // The journal of a write of a hand-made overlay with a file outside
    // the request's ownership (`write` would refuse it; `recover` must
    // too).
    let outside = fx.root.join("t.cc");
    fs::write(&outside, "int t;\n").unwrap();
    let (_, foreign) = hand(&fx, set, &[(bytes_of(&outside), b"int t;\n", b"int u;\n")]);
    let j = Journal::of_write(&fx.overlay_of(foreign), foreign);
    fx.put_journal(&j.to_bytes());
    let out = fx.slip(&["apply", "--recover"]);
    out.expect(3);
    assert!(out.stderr.contains("journal_not_owned"), "{out:#?}");
    assert!(fx.store().journal_path().exists());
    assert_eq!(fs::read_to_string(&outside).unwrap(), "int t;\n");
    // A journal whose overlay the store does not have: the ownership
    // cannot be found, the journal is kept.
    let j = Journal {
        overlay: Digest([9; 32]),
        ..journal_of(
            &fx,
            overlay_id,
            &Overlay::from_bytes(
                &fx.store().read_object(Object::Overlay, &overlay_id).unwrap().unwrap(),
            )
            .unwrap(),
        )
    };
    let j = Journal {
        files: j
            .files
            .iter()
            .map(|f| JournalEntry { staged: staged_name(&f.path, j.overlay), ..f.clone() })
            .collect(),
        ..j.clone()
    };
    fx.put_journal(&j.to_bytes());
    let out = fx.slip(&["apply", "--recover"]);
    out.expect(3);
    assert!(out.stderr.contains("the journal is kept"), "{out:#?}");
    assert!(fx.tree_is_pre());
}

/// A path as bytes.
fn bytes_of(p: &Path) -> Vec<u8> {
    p.as_os_str().as_bytes().to_vec()
}

/// A hand-made overlay of edit set `set` over `entries` (path, pre, post),
/// its blobs published, and a verified result on it: the result id and
/// the overlay id. `write` and `--patch` must refuse what such an overlay
/// carries that the edit set's could not.
fn hand(fx: &Fx, set: Digest, entries: &[(Vec<u8>, &[u8], &[u8])]) -> (String, Digest) {
    let store = fx.store();
    let mut files: Vec<OverlayFile> = entries
        .iter()
        .map(|(p, pre, post)| {
            store.publish_blob(pre).unwrap();
            store.publish_blob(post).unwrap();
            OverlayFile { path: p.clone(), pre: content(pre), post: content(post) }
        })
        .collect();
    files.sort_by(|a, b| a.path.cmp(&b.path));
    let overlay = Overlay { edits: set, files, spellings: vec![] };
    let id = store.publish_object(Object::Overlay, &overlay.to_bytes()).unwrap();
    (fx.publish_result(&fx.result(set, Some(id), State::Verified)), id)
}

impl Fx {
    fn overlay_of(&self, id: Digest) -> Overlay {
        Overlay::from_bytes(&self.store().read_object(Object::Overlay, &id).unwrap().unwrap())
            .unwrap()
    }

    /// The fixture's volume finds a name in another case (this workspace's
    /// host mount does; most Linux file systems do not).
    fn case_insensitive(&self) -> bool {
        let p = self.root.join("CaseProbe");
        fs::write(&p, "x").unwrap();
        let found = self.root.join("caseprobe").exists();
        fs::remove_file(&p).unwrap();
        found
    }

    /// The names of `src/` that are staged files of a write.
    fn staged_left(&self) -> Vec<String> {
        fs::read_dir(self.root.join("src"))
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|n| n.contains(".slip-"))
            .collect()
    }
}

/// Says which kind of volume the one-spelling tests ran on (the folding
/// root's; the work root's too); with `SLIP_TEST_VOLUME` set
/// (`case-insensitive` or `case-sensitive`), fails unless the folding root
/// is that kind, so that a run can require the tests that need a
/// case-insensitive volume to have run rather than skipped.
#[test]
fn s2_1_the_volume_of_the_tests() {
    let work = Fx::new("volume");
    let kind = if work.case_insensitive() { "case-insensitive" } else { "case-sensitive" };
    eprintln!("the volume of the work root {} is {kind}", work.root.display());
    let fx = Fx::new_in(&common::folding_root(), "volume");
    let kind = if fx.case_insensitive() { "case-insensitive" } else { "case-sensitive" };
    eprintln!("the volume of the folding root {} is {kind}", fx.root.display());
    if let Ok(want) = std::env::var("SLIP_TEST_VOLUME") {
        assert_eq!(kind, want, "the volume is not the kind `SLIP_TEST_VOLUME` asks for");
    }
}

/// The skip of a test that needs a volume that finds a name in another
/// case: `true` (and a line saying so) when the fixture's volume does not.
/// Through `common::folding_volume`, which panics on the gate's host when
/// the folding root does not fold; `fx` must be under it.
fn skip_unless_case_insensitive(fx: &Fx, test: &str) -> bool {
    let folds = common::folding_volume(test).is_some();
    assert!(!folds || fx.case_insensitive(), "{test}: {} does not fold", fx.root.display());
    !folds
}

/// §2.1 "one spelling", §7.3 step 1, §7.2: on a volume that finds names
/// in any case, a directory spelled in another case than on disk (the
/// request owns `src/` and carves out `src/vendor` read-only; the overlay
/// names `src/Vendor/x.h`) is refused, `not_canonical`, naming the
/// spelling on disk, by `--write` and `--patch`: status 1, nothing
/// written. The control: the exact spelling is `not_owned`.
#[test]
fn s2_1_another_case_of_a_readonly_directory_refused() {
    let fx = Fx::new_in(&common::folding_root(), "case-readonly");
    if skip_unless_case_insensitive(&fx, "s2_1_another_case_of_a_readonly_directory_refused") {
        return;
    }
    let mut request = fx.rename();
    request.ownership.readonly = vec![bytes_of(&fx.root.join("src/vendor"))];
    fs::create_dir_all(fx.root.join("src/vendor")).unwrap();
    let x = fx.root.join("src/vendor/x.h");
    fs::write(&x, "int v;\n").unwrap();
    let (_, set, out) = fx.reduced(&request, &agreeing());
    out.expect(0);
    let (r, _) = hand(&fx, set, &[(bytes_of(&x), b"int v;\n", b"int w;\n")]);
    let out = apply_write(&fx, &r, &[]);
    out.expect(1);
    assert!(out.stderr.contains(&format!("refused not_owned: {}", x.display())), "{out:#?}");

    let other = fx.root.join("src/Vendor/x.h");
    let (r, _) = hand(&fx, set, &[(bytes_of(&other), b"int v;\n", b"int w;\n")]);
    let out = apply_write(&fx, &r, &[]);
    out.expect(1);
    let want = format!(
        "refused not_canonical: {} is not spelled as on disk ({})",
        other.display(),
        x.display()
    );
    assert!(out.stderr.contains(&want), "{out:#?}");
    assert_eq!(out.value("written"), None);
    assert_eq!(fs::read_to_string(&x).unwrap(), "int v;\n");
    assert!(!fx.store().journal_path().exists());
    let patch = fx.root.join("p.patch");
    let out = apply_patch(&fx, &r, &patch, &[]);
    out.expect(1);
    assert!(out.stderr.contains(&want), "{out:#?}");
    assert!(!patch.exists());
}

/// §2.1 "one spelling": a file name in another case (`src/A.cc` for
/// `src/a.cc`) is refused, `not_canonical`; the tree is untouched.
#[test]
fn s2_1_a_file_name_in_another_case_refused() {
    let (fx, set, _) = agreed_in(&common::folding_root(), "case-file", Fx::rename);
    if skip_unless_case_insensitive(&fx, "s2_1_a_file_name_in_another_case_refused") {
        return;
    }
    let (r, _) = hand(&fx, set, &[(bytes_of(&fx.src("A.cc")), A.as_bytes(), b"int z;\n")]);
    let out = apply_write(&fx, &r, &[]);
    out.expect(1);
    let want = format!(
        "refused not_canonical: {} is not spelled as on disk ({})",
        fx.src("A.cc").display(),
        fx.src("a.cc").display()
    );
    assert!(out.stderr.contains(&want), "{out:#?}");
    assert!(fx.tree_is_pre());
}

/// §2.1 "one spelling", §7.3 step 1: one file named by two spellings in
/// one overlay is refused by the spelling check, before anything is
/// staged: no staged file, no journal, the file its pre-image.
#[test]
fn s2_1_two_spellings_of_one_file_refused_before_staging() {
    let (fx, set, _) = agreed_in(&common::folding_root(), "case-two", Fx::rename);
    if skip_unless_case_insensitive(&fx, "s2_1_two_spellings_of_one_file_refused_before_staging") {
        return;
    }
    let a = A.as_bytes();
    let (r, _) =
        hand(&fx, set, &[(fx.real("a.cc"), a, b"one\n"), (bytes_of(&fx.src("A.cc")), a, b"two\n")]);
    let out = apply_write(&fx, &r, &[]);
    out.expect(1);
    assert!(
        out.stderr.contains(&format!("refused not_canonical: {}", fx.src("A.cc").display())),
        "{out:#?}"
    );
    assert!(fx.tree_is_pre());
    assert_eq!(fx.staged_left(), Vec::<String>::new());
    assert!(!fx.store().journal_path().exists());
}

/// §7.2: `--patch` refuses what `--write` refuses by path: a hand-made
/// overlay with a file outside the request's ownership gives no patch,
/// status 1, `not_owned`.
#[test]
fn s7_2_patch_refuses_a_file_not_owned() {
    let (fx, set, _) = agreed("patch-not-owned", Fx::rename);
    let outside = fx.root.join("t.cc");
    fs::write(&outside, "int t;\n").unwrap();
    let (r, _) = hand(&fx, set, &[(bytes_of(&outside), b"int t;\n", b"int u;\n")]);
    let patch = fx.root.join("p.patch");
    let out = apply_patch(&fx, &r, &patch, &[]);
    out.expect(1);
    assert!(out.stderr.contains(&format!("refused not_owned: {}", outside.display())), "{out:#?}");
    assert!(!patch.exists());
    assert_eq!(out.value("patch"), None);
}

/// §7.2: a temporary of a patch that another run left beside the patch
/// (a run killed by SIGKILL, which no cleanup can follow) is reported and
/// left; this run's patch is written all the same.
#[test]
fn s7_2_patch_reports_a_temporary_left_by_another_run() {
    let (fx, set, overlay) = agreed("patch-leftover", Fx::rename);
    let r = fx.publish_result(&fx.result(set, Some(overlay), State::Verified));
    let patch = fx.root.join("p.patch");
    let left = fx.root.join(".p.patch.slip-tmp.1");
    fs::write(&left, "partial").unwrap();
    let out = apply_patch(&fx, &r, &patch, &[]);
    out.expect(0);
    assert!(
        out.stderr.contains(&format!("slip: {}: the temporary of a patch", left.display())),
        "{out:#?}"
    );
    assert!(left.exists());
    assert!(patch.exists());
}

/// §7.3, `--recover`: a journal whose overlay is real but whose files are
/// not the ones a write of that overlay journals (a file the overlay does
/// not change; an entry's `pre` altered) is refused, status 3, nothing
/// changed, the journal kept.
#[test]
fn s7_3_recover_refuses_a_journal_not_of_its_overlay() {
    let (fx, _, overlay_id) = agreed("recover-not-bound", Fx::rename);
    let c = fx.src("c.cc");
    fs::write(&c, "new\n").unwrap();
    fx.store().publish_blob(b"old\n").unwrap();
    let path = bytes_of(&c);
    let j = Journal {
        overlay: overlay_id,
        files: vec![JournalEntry {
            staged: staged_name(&path, overlay_id),
            path,
            pre: content(b"old\n"),
            post: content(b"new\n"),
        }],
    };
    fx.put_journal(&j.to_bytes());
    let out = fx.slip(&["apply", "--recover"]);
    out.expect(3);
    assert!(out.stderr.contains("the journal is not the one a write of its overlay"), "{out:#?}");
    assert_eq!(fs::read_to_string(&c).unwrap(), "new\n");
    assert!(fx.store().journal_path().exists());

    let mut j = journal_of(&fx, overlay_id, &fx.overlay_of(overlay_id));
    fs::write(fx.src("a.cc"), Fx::post(A)).unwrap();
    j.files[0].pre = content(b"int other;\n");
    fx.store().publish_blob(b"int other;\n").unwrap();
    fx.put_journal(&j.to_bytes());
    let out = fx.slip(&["apply", "--recover"]);
    out.expect(3);
    assert!(out.stderr.contains("the journal is not the one a write of its overlay"), "{out:#?}");
    assert_eq!(fx.read("a.cc"), Fx::post(A), "nothing changed");
    assert!(fx.store().journal_path().exists());
}

/// §2.1 "one spelling", §7.3: `--recover` checks the spelling of every
/// path its journal names before anything: a journal (of a hand-made
/// overlay) naming `src/A.cc` is refused, status 3, the journal kept.
#[test]
fn s7_3_recover_refuses_a_path_not_spelled_as_on_disk() {
    let (fx, set, _) = agreed_in(&common::folding_root(), "recover-case", Fx::rename);
    if skip_unless_case_insensitive(&fx, "s7_3_recover_refuses_a_path_not_spelled_as_on_disk") {
        return;
    }
    let other = fx.src("A.cc");
    fs::write(fx.src("a.cc"), "int z;\n").unwrap();
    let (_, id) = hand(&fx, set, &[(bytes_of(&other), A.as_bytes(), b"int z;\n")]);
    fx.put_journal(&Journal::of_write(&fx.overlay_of(id), id).to_bytes());
    let out = fx.slip(&["apply", "--recover"]);
    out.expect(3);
    assert!(out.stderr.contains("refused not_canonical: "), "{out:#?}");
    assert_eq!(fx.read("a.cc"), "int z;\n", "nothing changed");
    assert!(fx.store().journal_path().exists());
}

/// §10.5, `exec.md` §8.5: a `--result` prefix that two results share is
/// an input error naming both; nothing is written.
#[test]
fn s10_5_apply_result_prefix_of_two() {
    let (fx, set, overlay) = agreed("prefix", Fx::rename);
    let mut seen = std::collections::HashMap::new();
    let mut hit = None;
    for i in 0..5000u64 {
        let mut r = fx.result(set, Some(overlay), State::VerifiedWithResidue);
        r.residue.push(Residue {
            category: Category::Dependent,
            file: fx.real("a.cc"),
            lo: 0,
            hi: 1,
            line: 1 + i,
            column: 1,
        });
        let id = fx.publish_result(&r);
        if let Some(other) = seen.insert(id[..4].to_owned(), id.clone()) {
            hit = Some((id[..4].to_owned(), other, id));
            break;
        }
    }
    let (prefix, a, b) = hit.expect("two results with one 4-digit prefix");
    let out = apply_write(&fx, &prefix, &["--allow-residue"]);
    out.expect(3);
    assert!(out.stderr.contains("2 results have an id starting with it"), "{out:#?}");
    assert!(out.stderr.contains(&a) && out.stderr.contains(&b), "{out:#?}");
    assert!(fx.tree_is_pre());
}

/// `exec.md` §2.2, decision 1 of the landed file: reduce reads the shards
/// of the observers the round's map phase selected, no others. Here the
/// map run record selects `a.cc`'s observer only; `b.cc`'s has a shard of
/// an older round and none of this one: the edit set is `a.cc`'s alone,
/// with nothing unparsed.
#[test]
fn s2_2_reduce_reads_the_selected_observers_only() {
    let fx = Fx::new("selected");
    let plan = fx.plan();
    let request = fx.rename();
    let mut shards = agreeing();
    let b = shards.pop().unwrap();
    fx.round(&plan, 0, &request, &[(b.0, S { edits: false, ..b.1 })]);
    fx.round(&plan, ROUND, &request, &shards);
    let out = fx.reduce(plan.1, ROUND, &[]);
    out.expect(0);
    let set = fx.edit_set(out.digest("edits"));
    assert_eq!(set.unparsed, Vec::<Digest>::new(), "{out:#?}");
    assert!(
        out.lines().contains(&"edit 2 in 2 file(s); shared 0; most observers 1; nested 0"),
        "{out:#?}"
    );
}

// ---------------------------------------------------------------------------
// Usage (spec/exec.md §8, §8.5 status 2)

/// §8: each option without its value, `--write` with `--patch`,
/// `--recover` with `--result`, and the other combinations that are not
/// commands: status 2, one line and the usage text.
#[test]
fn s8_usage_errors() {
    for (args, message) in [
        (&["reduce", "--plan"][..], "`--plan` needs a value"),
        (&["reduce", "--plan", "abcd", "--round"], "`--round` needs a value"),
        (&["reduce", "--plan", "abcd", "--round", "1", "--store"], "`--store` needs a value"),
        (&["reduce", "--plan", "abcd", "--round", "1", "-j"], "`-j` needs a value"),
        (&["reduce", "--plan", "abcd"], "`reduce` needs `--round`"),
        (&["reduce", "--round", "1"], "`reduce` needs `--plan`"),
        (&["reduce", "--plan", "abcd", "--round", "1", "-j", "0"], "`-j` must be at least 1"),
        (&["apply", "--result"], "`--result` needs a value"),
        (&["apply", "--result", "abcd", "--patch"], "`--patch` needs a value"),
        (
            &["apply", "--result", "abcd", "--write", "--relative-to"],
            "`--relative-to` needs a value",
        ),
        (&["apply", "--recover", "--store"], "`--store` needs a value"),
        (
            &["apply", "--result", "abcd", "--write", "--patch", "p"],
            "`--write` and `--patch` cannot both be given",
        ),
        (
            &["apply", "--recover", "--result", "abcd"],
            "`--recover` cannot be given with `--result`",
        ),
        (&["apply", "--recover", "--write"], "`--recover` cannot be given with `--write`"),
        (&["apply", "--result", "abcd"], "`apply --result` needs `--write` or `--patch`"),
        (&["apply", "--write"], "`apply` needs `--result` or `--recover`"),
        (&["apply", "--result", "abc", "--write"], "`--result`: `abc` is not an id"),
        (&["apply", "--result", "abcd", "--write=1"], "`--write` takes no value"),
        (
            &["apply", "--result", "abcd", "--write", "--allow-unparsed"],
            "unknown option `--allow-unparsed`",
        ),
    ] {
        let out = slip(args);
        out.expect(2);
        let first = out.stderr.lines().next().unwrap_or("");
        assert!(first.starts_with("slip: ") && first.contains(message), "{args:?}: {out:#?}");
        assert!(out.stderr.contains("usage: slip plan"), "{args:?}");
    }
}

/// §8: `--help` lists the commands with the usage lines of §8.
#[test]
fn s8_help_lists_reduce_and_apply() {
    let out = slip(&["--help"]);
    out.expect(0);
    for line in [
        "slip reduce --plan <id or file> --round <n> [--store <dir>] [-j <n>]",
        "slip apply --result <id> (--write | --patch <file>) [--relative-to <dir>]",
        "[--allow-residue] [--allow-partial] [--allow-errors] [--store <dir>]",
        "slip apply --recover [--store <dir>]",
    ] {
        assert!(out.stdout.contains(line), "{line}");
    }
}

/// §7.2 (`spec/landed/edits/WP-147.md`, row L2): `--relative-to` given in
/// another case than on disk names the same directory: the patch is
/// written, its names relative to it. Before WP-147 the directory's real path
/// kept the spelling asked (`<root>/SRC`), no overlay file was inside it
/// by bytes, and `apply` exited 2 ("not inside `--relative-to`").
#[test]
fn s7_2_relative_to_in_another_case() {
    let (fx, set, overlay) = agreed_in(&common::folding_root(), "relative-case", Fx::rename);
    if skip_unless_case_insensitive(&fx, "s7_2_relative_to_in_another_case") {
        return;
    }
    let r = fx.publish_result(&fx.result(set, Some(overlay), State::Verified));
    let patch = fx.root.join("out.patch");
    let other = fx.root.join("SRC");
    let out = apply_patch(&fx, &r, &patch, &["--relative-to", other.to_str().unwrap()]);
    out.expect(0);
    let text = fs::read_to_string(&patch).unwrap();
    assert!(text.starts_with("--- a.cc\n+++ a.cc\n"), "{text}");
}

/// §7.3 step 2 (WP-161): a journal cut short before its publish returned
/// (a prefix of a whole one, shorter than the head or than its declared
/// total) is removed by `apply --recover`, which says so and why and exits
/// 0 with the tree untouched; until then, `--write` refuses the store as it
/// refuses any journal. A damaged whole one is kept.
#[test]
fn s7_3_recover_removes_an_interrupted_journal() {
    let (fx, set, overlay_id) = agreed("interrupted", Fx::rename);
    let r = fx.publish_result(&fx.result(set, Some(overlay_id), State::Verified));
    let overlay = fx.store().read_object(Object::Overlay, &overlay_id).unwrap().unwrap();
    let overlay = Overlay::from_bytes(&overlay).unwrap();
    let whole = journal_of(&fx, overlay_id, &overlay).to_bytes();
    for cut in [whole[..whole.len() / 2].to_vec(), Vec::new(), whole[..20].to_vec()] {
        fx.put_journal(&cut);
        let out = apply_write(&fx, &r, &[]);
        out.expect(3);
        assert!(out.stderr.contains("unfinished write of the working tree"), "{out:#?}");
        let out = fx.slip(&["apply", "--recover"]);
        out.expect(0);
        let lines = out.lines();
        assert_eq!(lines.len(), 1, "{out:#?}");
        assert!(
            lines[0].starts_with(
                "recovered: the write was interrupted before any file was touched (the journal is "
            ) && lines[0].ends_with("); the journal is removed"),
            "{out:#?}"
        );
        assert!(!fx.store().journal_path().exists());
        assert!(fx.tree_is_pre());
    }
    // A whole-length journal with a bit flipped, or a short file that is
    // no journal's start, is damaged, not cut short: `--recover` refuses
    // it, names the file, keeps it, and never says that no file was
    // touched (spec/edits.md §10.4).
    let mut flipped = whole.clone();
    flipped[whole.len() - 40] ^= 1;
    for bad in [flipped, b"anything".to_vec()] {
        fx.put_journal(&bad);
        let out = fx.slip(&["apply", "--recover"]);
        out.expect(3);
        assert!(out.stderr.contains("write.journal"), "{out:#?}");
        assert!(out.stderr.contains("damaged, not cut short: it is kept"), "{out:#?}");
        assert!(
            !out.stdout.contains("no file") && !out.stdout.contains("before any file"),
            "{out:#?}"
        );
        assert_eq!(fs::read(fx.store().journal_path()).unwrap(), bad);
        fs::remove_file(fx.store().journal_path()).unwrap();
    }
    // The store is usable again.
    apply_write(&fx, &r, &[]).expect(0);
    assert!(fx.tree_is_post());
}

/// `spec/exec.md` §2.1 "Which reads are checked" (WP-161): a plan stored
/// under another plan's id is refused by every command that reads a plan
/// by id, as a blob is (`Input`, "the file's content id is not its
/// name").
#[test]
fn s2_1_a_plan_under_another_id_is_refused() {
    let (fx, set, _) = agreed("plan-id", Fx::rename);
    let store = fx.store();
    let (plan, id) = fx.plan();
    let mut other = plan.clone();
    other.tool_slip = "another".into();
    let other_id = store.publish_plan(&other.to_bytes()).unwrap();
    fs::write(store.plan_path(&other_id), plan.to_bytes()).unwrap();
    let (p, set) = (other_id.to_string(), set.to_string());
    let out_file = fx.root.join("decl.out");
    let from = fx.root.join("from.txt");
    fs::write(&from, "").unwrap();
    let (out_s, from_s) = (out_file.to_str().unwrap(), from.to_str().unwrap());
    let commands: Vec<Vec<&str>> = vec![
        vec!["parse", "--plan", &p],
        vec!["verify", "--plan", &p],
        vec!["declare", "--plan", &p, "--from", from_s, "--out", out_s],
        vec!["units", "--plan", &p],
        vec!["map", "--plan", &p, "--from", "f", "--to", "g", "--owned", "src"],
        vec!["rename", "--plan", &p, "--from", "f", "--to", "g", "--owned", "src"],
        vec!["reduce", "--plan", &p, "--round", "1"],
        vec!["check", "--plan", &p, "--round", "1", "--edits", &set],
    ];
    for args in commands {
        let out = fx.slip(&args);
        out.expect(3);
        assert!(out.stderr.contains("the file's content id is not its name"), "{args:?}: {out:#?}");
        assert!(out.stderr.contains(&p), "{args:?}: {out:#?}");
    }
    // The plan under its own id is read.
    fx.reduce(id, ROUND, &[]).expect(0);
}

/// The same for declared changes: `check --declared <id>` refuses
/// declared changes stored under another's id.
#[test]
fn s2_1_declared_changes_under_another_id_are_refused() {
    let (fx, set, _) = agreed("declared-id", Fx::rename);
    let store = fx.store();
    let plan = store.find_plan("").unwrap()[0].to_string();
    let none = Declared::none();
    let none_id = store.publish_declared(&none.to_bytes()).unwrap();
    let other = Declared::new(vec![], vec![Digest([9; 32])], vec![], vec![]).unwrap();
    let other_id = store.publish_declared(&other.to_bytes()).unwrap();
    assert_ne!(none_id, other_id);
    fs::write(store.declared_path(&other_id), none.to_bytes()).unwrap();
    let (set, d) = (set.to_string(), other_id.to_string());
    let out =
        fx.slip(&["check", "--plan", &plan, "--round", "1", "--edits", &set, "--declared", &d]);
    out.expect(3);
    assert!(out.stderr.contains("the file's content id is not its name"), "{out:#?}");
    assert!(out.stderr.contains(&d), "{out:#?}");
}

/// §6 & §10.5 (WP-160 item 2): a `mixed_follower` refusal names an unselected
/// follower that is not in `EditSet.subjects`, carries its joined display in
/// `Refusal.display`, and prints that display instead of its 64-hex key.
#[test]
fn s10_5_mixed_follower_refusal_prints_display() {
    let fx = Fx::new("mixed-follower");
    let other = Digest([0x55; 32]);
    let follower = Digest([0x66; 32]);
    let mut a = S::new(vec![put("h.h", F_H, "g"), put("a.cc", F_A, "g")]);
    a.extra_subjects = vec![
        Subject {
            id: other,
            rule: None,
            role: Role::Candidate,
            of: vec![],
            flags: 0,
            display: "b::f".to_owned(),
        },
        Subject {
            id: follower,
            rule: None,
            role: Role::Follower,
            of: vec![K, other],
            flags: 0,
            display: "c::f".to_owned(),
        },
    ];
    let b = S::new(vec![put("h.h", F_H, "g"), put("b.cc", F_B[0], "g")]);
    let (_, _, out) = fx.reduced(&fx.rename(), &[("a.cc", a), ("b.cc", b)]);
    out.expect(1);
    assert_eq!(out.value("state"), Some("refused"));
    let set = fx.edit_set(out.digest("edits"));
    assert!(set.subjects.iter().all(|s| s.id != follower), "unselected follower stays out");
    assert_eq!(set.refusals.len(), 1);
    assert_eq!(set.refusals[0].subject, Some(follower));
    assert_eq!(set.refusals[0].display.as_deref(), Some("c::f"));
    let want =
        format!("refusal mixed_follower: c::f (1 observer(s), first {})", fx.src("a.cc").display());
    assert!(out.lines().contains(&want.as_str()), "want {want:?}: {out:#?}");
    assert!(!out.stdout.contains(&follower.to_string()), "must not print raw key: {out:#?}");
}

// ---------------------------------------------------------------------------
// WP-151 item 1 (row T1): a request's ownership entries read from the store

/// The folding tests' guard (WP-151, `spec/harness.md` §11.5): fails when
/// the source tree's volume folds and the folding root's does not, so
/// that every test about a volume that folds skipping cannot pass unseen;
/// says which kind each root is.
#[test]
fn s11_5_folding_root() {
    common::work_root::folding_guard(env!("CARGO_TARGET_TMPDIR"));
}

/// `src/dep/x.h` and `src/café/y.h` (NFC): directories a request carves
/// out read-only that no edit of [`agreeing`] touches.
fn with_dependencies(fx: &Fx) {
    for (dir, file) in [("dep", "x.h"), ("caf\u{e9}", "y.h")] {
        fs::create_dir_all(fx.root.join("src").join(dir)).unwrap();
        fs::write(fx.root.join("src").join(dir).join(file), "int x;\n").unwrap();
    }
}

/// The real path of `src/<rel>`, as bytes (spelled as asked).
fn src_entry(fx: &Fx, rel: &str) -> Vec<u8> {
    bytes_of(&fx.root.join("src").join(rel))
}

/// The agreed rename's request with `readonly` (and `src/` owned), its
/// round published as `map` would leave it (the request in the store, a
/// map shard per observer naming it).
fn round_with_readonly(fx: &Fx, readonly: Vec<u8>) -> ((Plan, Digest), Transform, Digest) {
    let mut request = fx.rename();
    request.ownership.readonly = vec![readonly];
    let plan = fx.plan();
    let id = fx.round(&plan, ROUND, &request, &agreeing());
    (plan, request, id)
}

/// The three commands that read the round's request from the store, each
/// with the hand-written request whose read-only entry is `entry`: `slip
/// reduce` and `slip check` refuse with status 3 before anything is
/// published or any job runs, and `slip apply` of a result of that request
/// (its edit set made by `reduce_round`, which takes the request in
/// memory) refuses with status 3 and writes nothing. `why` ends the line.
fn refused_at_each(fx: &Fx, entry: Vec<u8>, why: &str) {
    let line = format!("ownership: the readonly entry {} {why}", String::from_utf8_lossy(&entry));
    let (plan, request, id) = round_with_readonly(fx, entry);
    let out = fx.reduce(plan.1, ROUND, &[]);
    out.expect(3);
    assert!(out.stderr.contains(&line), "{out:#?}");
    assert_eq!(out.value("edits"), None, "no edit set: {out:#?}");
    let (p, r) = (plan.1.to_string(), ROUND.to_string());
    let out = fx.slip(&["check", "--plan", &p, "--round", &r, "--edits", "0000"]);
    out.expect(3);
    assert!(out.stderr.contains(&line), "{out:#?}");
    assert_eq!(out.value("transform"), None, "before the edit set is read: {out:#?}");

    let store = fx.store();
    let selected: Vec<usize> = (0..plan.0.observers.len()).collect();
    let (set_id, _, _) =
        reduce_round(&store, plan.1, &plan.0, ROUND, &selected, (id, &request), 1).unwrap();
    let (overlay, _) = fx.overlay(set_id);
    let result = fx.publish_result(&fx.result(set_id, Some(overlay), State::Verified));
    let out = apply_write(fx, &result, &[]);
    out.expect(3);
    assert!(out.stderr.contains(&line), "{out:#?}");
    assert!(fx.tree_is_pre());
    assert!(!store.journal_path().exists());
}

/// §2.1 "One spelling" (WP-151 item 1): an entry of a request read from
/// the store that does not exist is refused at `reduce`, `check` and
/// `apply` (status 3), never passed: on a volume that answers lookups
/// inconsistently, "missing" is the answer that would let it carve
/// nothing. The control: the entry as on disk passes each (`reduce` 0,
/// `check` past the request to the next thing it reads, `apply` writes).
/// Any volume.
#[test]
fn s2_1_a_request_entry_that_does_not_exist() {
    let fx = Fx::new("t1-missing");
    with_dependencies(&fx);
    refused_at_each(&fx, src_entry(&fx, "nope"), "does not exist");

    let fx = Fx::new("t1-control");
    with_dependencies(&fx);
    let (plan, request, id) = round_with_readonly(&fx, src_entry(&fx, "dep"));
    let out = fx.reduce(plan.1, ROUND, &[]);
    out.expect(0);
    let set = out.digest("edits");
    // `check` past the request (nothing here parses: the edit set it is
    // given is not in the store, the next thing it reads).
    let (p, r) = (plan.1.to_string(), ROUND.to_string());
    let out = fx.slip(&["check", "--plan", &p, "--round", &r, "--edits", "0000"]);
    out.expect(3);
    assert!(!out.stderr.contains("ownership:"), "{out:#?}");
    assert!(out.stderr.contains("edit set 0000: none in the store"), "{out:#?}");
    let _ = (request, id);
    let (overlay, _) = fx.overlay(set);
    let result = fx.publish_result(&fx.result(set, Some(overlay), State::Verified));
    apply_write(&fx, &result, &[]).expect(0);
    assert!(fx.tree_is_post());
}

/// §2.1 "One spelling" (WP-151 item 1, row T1): a hand-written request
/// whose read-only entry is in another case (`src/DEP` for `src/dep`), or
/// in another Unicode normalisation (`src/café` in NFD for the NFC name
/// on disk), carves nothing out of the paths, which are spelled on disk;
/// `reduce`, `check` and `apply` refuse it with status 3 naming the entry
/// and its spelling on disk. An owned entry is checked as a read-only one
/// is. On the folding root; skipped on a volume that does not fold.
#[test]
fn s2_1_a_request_entry_not_spelled_on_disk() {
    let fx = Fx::new_in(&common::folding_root(), "t1-case");
    if skip_unless_case_insensitive(&fx, "s2_1_a_request_entry_not_spelled_on_disk") {
        return;
    }
    with_dependencies(&fx);
    let disk = |rel: &str| {
        format!("is not spelled as on disk ({})", fx.root.join("src").join(rel).display())
    };
    refused_at_each(&fx, src_entry(&fx, "DEP"), &disk("dep"));
    let fx = Fx::new_in(&common::folding_root(), "t1-nfd");
    with_dependencies(&fx);
    let disk = |rel: &str| {
        format!("is not spelled as on disk ({})", fx.root.join("src").join(rel).display())
    };
    refused_at_each(&fx, src_entry(&fx, "cafe\u{301}"), &disk("caf\u{e9}"));

    // The owned entry, in another case.
    let fx = Fx::new_in(&common::folding_root(), "t1-owned");
    let mut request = fx.rename();
    let upper = bytes_of(&fx.root.join("SRC"));
    request.ownership.owned = vec![upper.clone()];
    let plan = fx.plan();
    fx.round(&plan, ROUND, &request, &agreeing());
    let out = fx.reduce(plan.1, ROUND, &[]);
    out.expect(3);
    let line = format!(
        "ownership: the owned entry {} is not spelled as on disk ({})",
        String::from_utf8_lossy(&upper),
        fx.root.join("src").display()
    );
    assert!(out.stderr.contains(&line), "{out:#?}");

    // WP-118 second review SF4: a rebuilt entry is checked as the other
    // two are, in another case and missing, before anything is published.
    for (name, rel, on_disk) in
        [("t1-rebuilt", "DEP", Some("dep")), ("t1-rebuilt-gone", "nope", None)]
    {
        let fx = Fx::new_in(&common::folding_root(), name);
        with_dependencies(&fx);
        let mut request = fx.rename();
        let entry = src_entry(&fx, rel);
        request.ownership.rebuilt = vec![entry.clone()];
        let plan = fx.plan();
        fx.round(&plan, ROUND, &request, &agreeing());
        let out = fx.reduce(plan.1, ROUND, &[]);
        out.expect(3);
        let why = match on_disk {
            Some(d) => {
                format!("is not spelled as on disk ({})", fx.root.join("src").join(d).display())
            }
            None => "does not exist".to_string(),
        };
        let line =
            format!("ownership: the rebuilt entry {} {why}", String::from_utf8_lossy(&entry));
        assert!(out.stderr.contains(&line), "{out:#?}");
        assert_eq!(out.value("edits"), None, "no edit set: {out:#?}");
    }
}

// ----- WP-151 fix round: forged requests at apply (the review's probes) ------

/// A forged request owning `src/`, whose read-only `entry` was meant to
/// carve out `src/vendor`, made into a round and an edit set without the
/// command line (`reduce_round` with the request in memory), and an
/// overlay writing `src/vendor/x.h`, spelled on disk. The stored ownership
/// makes that file owned, so only the entry check stands between it and a
/// write: `apply --write`, `--patch` and `--recover` each refuse with
/// status 3, naming the entry, and `x.h` is never written; `--recover`
/// keeps the journal and says what ends it. Returns the `--recover` output.
fn forged_entry_refused_by_apply(fx: &Fx, entry: &[u8]) -> Out {
    fs::create_dir_all(fx.root.join("src/vendor")).unwrap();
    let x = fx.root.join("src/vendor/x.h");
    fs::write(&x, "int v;\n").unwrap();
    let mut request = fx.rename();
    request.ownership.readonly = vec![entry.to_vec()];
    assert!(request.ownership.is_owned(&bytes_of(&x)), "the forged entry protects nothing");
    let plan = fx.plan();
    let id = fx.round(&plan, ROUND, &request, &agreeing());
    let store = fx.store();
    let selected: Vec<usize> = (0..plan.0.observers.len()).collect();
    let (set_id, _, _) =
        reduce_round(&store, plan.1, &plan.0, ROUND, &selected, (id, &request), 1).unwrap();
    let (r, oid) = hand(fx, set_id, &[(bytes_of(&x), b"int v;\n", b"int w;\n")]);
    let entry_text = String::from_utf8_lossy(entry).into_owned();
    let out = apply_write(fx, &r, &[]);
    out.expect(3);
    assert!(out.stderr.contains(&entry_text), "{out:#?}");
    assert_eq!(fs::read_to_string(&x).unwrap(), "int v;\n");
    assert!(!store.journal_path().exists());
    let patch = fx.root.join("p.patch");
    let out = apply_patch(fx, &r, &patch, &[]);
    out.expect(3);
    assert!(out.stderr.contains(&entry_text), "{out:#?}");
    assert!(!patch.exists());
    fx.put_journal(&Journal::of_write(&fx.overlay_of(oid), oid).to_bytes());
    let out = fx.slip(&["apply", "--recover"]);
    out.expect(3);
    assert!(out.stderr.contains(&entry_text), "{out:#?}");
    assert!(
        out.stderr.contains("nothing was changed, the journal is kept")
            && out.stderr.contains("then run `slip apply --recover` again"),
        "{out:#?}"
    );
    assert!(!out.stderr.contains("cannot be found"), "{out:#?}");
    assert_eq!(fs::read_to_string(&x).unwrap(), "int v;\n");
    assert!(store.journal_path().exists());
    out
}

/// §2.1 (WP-151 review, attack 1): the forged entry is `SRC/Vendor` for
/// `src/vendor`, on a volume that folds. On the folding root.
#[test]
fn s2_1_a_forged_entry_in_another_case_is_refused_by_apply() {
    let Some(base) =
        common::folding_volume("s2_1_a_forged_entry_in_another_case_is_refused_by_apply")
    else {
        return;
    };
    let fx = Fx::new_in(&base, "forged-case");
    let out = forged_entry_refused_by_apply(&fx, &bytes_of(&fx.root.join("SRC/Vendor")));
    let disk = format!("is not spelled as on disk ({})", fx.root.join("src/vendor").display());
    assert!(out.stderr.contains(&disk), "{out:#?}");
}

/// §2.1 (WP-151 review, attack 2): the forged entry reaches `src/vendor`
/// through a symbolic link (`vlink`): not its own real path. Under the
/// work root.
#[test]
fn s2_1_a_forged_entry_through_a_link_is_refused_by_apply() {
    let fx = Fx::new("forged-link");
    fs::create_dir_all(fx.root.join("src/vendor")).unwrap();
    std::os::unix::fs::symlink(fx.root.join("src/vendor"), fx.root.join("vlink")).unwrap();
    let out = forged_entry_refused_by_apply(&fx, &bytes_of(&fx.root.join("vlink")));
    let disk = format!("is not spelled as on disk ({})", fx.root.join("src/vendor").display());
    assert!(out.stderr.contains(&disk), "{out:#?}");
}

/// §2.1, §7.3 (WP-151 review, attack 3, and F3): the forged entry
/// `src/nope` does not exist. `--recover` refuses, keeps the journal and
/// says that the entry must exist again; once the directory is made, as
/// the request records it, `--recover` ends the unfinished write (status
/// 0, the journal gone, `x.h` its pre-image). Under the work root.
#[test]
fn s7_3_recover_refused_until_the_entry_exists_again() {
    let fx = Fx::new("forged-missing");
    let nope = fx.root.join("src/nope");
    let out = forged_entry_refused_by_apply(&fx, &bytes_of(&nope));
    let line = format!("ownership: the readonly entry {} does not exist", nope.display());
    assert!(out.stderr.contains(&line), "{out:#?}");
    fs::create_dir_all(&nope).unwrap();
    let out = fx.slip(&["apply", "--recover"]);
    out.expect(0);
    assert!(!fx.store().journal_path().exists(), "{out:#?}");
    assert_eq!(fs::read_to_string(fx.root.join("src/vendor/x.h")).unwrap(), "int v;\n");
}
