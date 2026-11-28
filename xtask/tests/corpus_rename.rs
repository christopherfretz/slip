//! `cargo xtask corpus rename` (`spec/harness.md` §6.5).
//!
//! A toy corpus `tiny` (three units under `src/`, a `.git` that is not
//! copied, a symbolic link and a subdirectory), stamped as configured, and
//! stand-ins written by the test: a `slip` whose `rename --patch` prints
//! the lines of a store the test made with `slip_exec::store` and copies a
//! fixed patch, whose `reduce` prints an edit set id, and whose `rename
//! --write` undoes the patch in the owned copy (`patch -R`); a clang
//! (`fakecc`, `fakecxx`) that rejects the units listed in files, always or
//! only through `-ivfsoverlay`; and `cmake` and `make` that write a build
//! tree and log their calls. The patch is applied by the real `patch`.
//!
//! No helper here makes a hard link: the stand-ins copy with `cp` and
//! `patch` (which writes new files), and the copy under test is checked to
//! have one link per file.

#![cfg(target_os = "linux")]
#![allow(
    clippy::too_many_lines,
    clippy::many_single_char_names,
    clippy::format_push_string,
    reason = "a test sets up a store and reads a transcript of lines built as text"
)]

mod common;

use common::Fixture;
use slip_exec::store::{Access, Object, Store};
use slip_model::digests::{self, Digests, Group, GroupClass};
use slip_model::edits::{MacroSite, Reason};
use slip_model::editset::{Cause, EditSet, Refusal, SetState};
use slip_model::hash::content_digest;
use slip_model::json::{self, Value};
use slip_model::overlay::{Content, Overlay, OverlayFile};
use slip_model::result::{self, Category, Residue, RunVerdict, State, Tally, TransformResult};
use slip_model::transform_run::{Step, StepName, TransformRun};
use slip_model::{
    Command as Cmd, Configuration, Counter, Digest, End, Excluded, FileRef, Job, Language,
    Observer, Origin, Outcome as ShardOutcome, Phase, Plan, Reason as PlanReason, Run, RunRecord,
    Shard, Status, formats,
};
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use xtask::corpus::rename::{self, Env};
use xtask::corpus::rows::{self, ExpectedExcluded, ExpectedUnparsed};
use xtask::corpus::{self, Layout};
use xtask::invariants::Failure;

const KILN_ROWS: &str = "[tiny_project]\nurl = \"https://example.org/tiny.git\"\ntag = \"v1.0\"\n";
const CORPORA: &str = "[tiny]\nkiln = \"tiny_project\"\ncxx_std = 17\nbuild_type = \"Release\"\n\
     c_compiler = \"fakecc\"\ncxx_compiler = \"fakecxx\"\nconfigure = []\ngenerate = [\"gen\"]\n";

const PRE: &str = "namespace lib {\nint f();\n}\nint g() { return lib::f(); }\n";
const POST: &str = "namespace lib {\nint h();\n}\nint g() { return lib::h(); }\n";
const PATCH: &str = "--- a.cpp\n+++ a.cpp\n@@ -1,4 +1,4 @@\n namespace lib {\n-int f();\n+int h();\n }\n\
     -int g() { return lib::f(); }\n+int g() { return lib::h(); }\n";
const STARTED: u64 = 1_800_000_000_000_000_000;
const RESULT_FILE: &str = "repo/bench/results/tiny.rename.lib.f.json";

/// The stand-in `slip`.
const SLIP: &str = r#"#!/bin/sh
[ "$1" = --probe ] && exit 0
D=@DIR@
cmd=$1
shift
echo "$cmd $*" >> "$D/log"
patch=
owned=
write=
store=
while [ $# -gt 0 ]; do
  case "$1" in
    --patch) patch=$2; shift ;;
    --owned) owned=$2; shift ;;
    --store) store=$2; shift ;;
    --write) write=1 ;;
  esac
  shift
done
case "$cmd" in
  --version)
    echo "slip 0.0.0 (kiln 1.3.0)"
    exit 0 ;;
  plan)
    echo "plan $(cat "$D/census.plan")"
    exit 0 ;;
  parse)
    [ -e "$D/census.fail" ] && exit 3
    mkdir -p "$store"
    cp -r "$D/census.store/." "$store/"
    echo "plan $(cat "$D/census.plan")"
    echo "run $store/$(cat "$D/census.run")"
    exit 0 ;;
  rename)
    [ -e "$D/rename.fail" ] && exit 3
    if [ -n "$write" ]; then
      if [ ! -e "$D/back.broken" ]; then
        (cd "$owned" && patch -R -p0 --batch --no-backup-if-mismatch -i "$D/patch" > /dev/null) || exit 9
      fi
      echo "state verified"
      echo "written 1 file(s)"
      exit 0
    fi
    if [ -e "$D/patch" ]; then cp "$D/patch" "$patch" || exit 9; fi
    cat "$D/rename.out"
    exit $(cat "$D/rename.status") ;;
  reduce)
    if [ -e "$D/reduce.edits" ]; then echo "edits $(cat "$D/reduce.edits")"; else echo "edits $(cat "$D/edits")"; fi
    echo "state accepted"
    exit 0 ;;
esac
exit 99
"#;

/// The stand-in clang: rejects the units named in `reject` always and
/// those in `reject.after` only with `-ivfsoverlay`; crashes on units
/// listed in `crash` (exit 139 on both), `crash.before` / `crash.after`
/// (exit 139 on pre/post), `crash.text` / `crash.signal_text` (exit 1 with
/// clang crash text on stderr), or `crash.signal` (killed by SIGSEGV).
const CLANG: &str = r#"#!/bin/sh
[ "$1" = --probe ] && exit 0
D=@DIR@
vfs=
file=
syntax=
no_crash=
while [ $# -gt 0 ]; do
  case "$1" in
    -ivfsoverlay) vfs=$2; shift ;;
    -fsyntax-only) syntax=1 ;;
    -fno-crash-diagnostics) no_crash=1 ;;
    *.cpp) file=$(basename "$1") ;;
  esac
  shift
done
[ -n "$syntax" ] || exit 8
[ -n "$no_crash" ] || exit 9
case "$TMPDIR" in
  */rename/accept-tmp) ;;
  *) exit 10 ;;
esac
: > "$TMPDIR/touched-$file"
[ -n "$vfs" ] && [ ! -f "$vfs" ] && exit 7
echo "$file ${vfs:+post}" >> "$D/clang.log"
if grep -qx "$file" "$D/crash" 2>/dev/null; then
  echo "clang: error: clang frontend command failed with exit code 139" >&2
  exit 139
fi
if [ -z "$vfs" ] && grep -qx "$file" "$D/crash.before" 2>/dev/null; then
  echo "clang: error: clang frontend command failed with exit code 139" >&2
  exit 139
fi
if [ -n "$vfs" ] && grep -qx "$file" "$D/crash.after" 2>/dev/null; then
  echo "clang: error: clang frontend command failed with exit code 139" >&2
  exit 139
fi
if grep -qx "$file" "$D/crash.text" 2>/dev/null; then
  echo "PLEASE submit a bug report to https://github.com/llvm/llvm-project/issues/" >&2
  exit 1
fi
if grep -qx "$file" "$D/crash.signal_text" 2>/dev/null; then
  echo "clang: error: clang frontend command failed due to signal" >&2
  exit 1
fi
if grep -qx "$file" "$D/crash.signal" 2>/dev/null; then
  kill -SEGV $$
fi
if grep -qx "$file" "$D/reject" 2>/dev/null; then echo "$file:1:1: error: always" >&2; exit 1; fi
if [ -n "$vfs" ] && grep -qx "$file" "$D/reject.after" 2>/dev/null; then
  echo "$file:2:5: error: use of undeclared identifier 'h'" >&2; exit 1
fi
exit 0
"#;

const CMAKE: &str = r#"#!/bin/sh
[ "$1" = --probe ] && exit 0
D=@DIR@
echo "cmake $*" >> "$D/log"
while [ $# -gt 0 ]; do case "$1" in -B) shift; B="$1";; esac; shift; done
[ -e "$D/cmake.fail" ] && { echo "CMake Error: failing"; exit 1; }
mkdir -p "$B/CMakeFiles/gen.dir"
printf 'gen: CMakeFiles/gen.dir/rule\n' > "$B/CMakeFiles/Makefile2"
printf 'gen: CMakeFiles/gen\n' > "$B/CMakeFiles/gen.dir/build.make"
if [ -e "$D/back.bad_db" ]; then cp "$D/back.bad_db" "$B/compile_commands.json"; else echo "[]" > "$B/compile_commands.json"; fi
"#;

const MAKE: &str = r#"#!/bin/sh
[ "$1" = --probe ] && exit 0
D=@DIR@
echo "make $*" >> "$D/log"
[ -e "$D/make.fail" ] && { echo "a.cpp:2:5: error: failing"; exit 2; }
exit 0
"#;

fn d(n: u8) -> Digest {
    Digest([n; 32])
}

fn id(bytes: &[u8]) -> Digest {
    formats::content_id(bytes).unwrap()
}

/// A refused edit set with one `no_edit` refusal and `counters` (the
/// result is read on its own, so the stand-in needs no edits).
fn edit_set(plan: Digest, counters: Vec<Counter>) -> EditSet {
    EditSet {
        scheme: 1,
        plan,
        round: 1,
        transform: d(0x22),
        basis: d(0x03),
        // An edit set with no edit is `refused`; the result is read on
        // its own, so the stand-in needs no edits.
        state: SetState::Refused,
        files: Vec::new(),
        subjects: Vec::new(),
        rules: Vec::new(),
        edits: Vec::new(),
        groups: Vec::new(),
        changed: Vec::new(),
        kept: Vec::new(),
        unresolved: Vec::new(),
        refusals: vec![Refusal {
            cause: Cause::NoEdit,
            file: None,
            lo: 0,
            hi: 0,
            reason: Reason::None,
            subject: None,
            display: None,
            macro_site: None,
            other: None,
            observers: 0,
            first: Vec::new(),
        }],
        reports: Vec::new(),
        failures: Vec::new(),
        unparsed: Vec::new(),
        counters,
        instances: Vec::new(),
    }
}

/// What a run's store holds and prints: the state, the counters of the
/// edit set, the transform run record's wall time, any non-`ok` map shard
/// outcomes by source filename (`a.cpp`, `b.cpp`, `bad.cpp`), any plan
/// exclusions, and whether to omit the map run record from `TransformRun`.
#[derive(Clone)]
struct Run1 {
    state: State,
    counters: Vec<(&'static str, u64)>,
    wall_ns: u64,
    started: u64,
    map_outcomes: Vec<(&'static str, Status, &'static str)>,
    excluded: Vec<Excluded>,
    no_map_run: bool,
}

impl Run1 {
    fn of(state: State) -> Run1 {
        Run1 {
            state,
            counters: vec![("edits.kept", 1), ("edits.planned", 2)],
            wall_ns: 5_000_000,
            started: STARTED,
            map_outcomes: Vec::new(),
            excluded: Vec::new(),
            no_map_run: false,
        }
    }
}

struct World {
    f: Fixture,
    /// The real path of the corpus's `src/`.
    src: PathBuf,
}

impl World {
    fn new(tag: &str) -> World {
        World::with_corpora(tag, CORPORA)
    }

    fn with_corpora(tag: &str, corpora: &str) -> World {
        let f = Fixture::empty(tag);
        f.write("repo/bench/corpora.toml", corpora);
        f.write("repo/kiln/scripts/oss/projects.toml", KILN_ROWS);
        let c = rows::parse(corpora, KILN_ROWS).unwrap().remove(0);
        let layout = Layout::new(&f.path("corpora"), "tiny");
        f.write("corpora/tiny/src/a.cpp", PRE);
        f.write("corpora/tiny/src/b.cpp", "int b() { return 1; }\n");
        f.write("corpora/tiny/src/bad.cpp", "int bad( {\n");
        f.write("corpora/tiny/src/sub/c.h", "#pragma once\n");
        f.write("corpora/tiny/src/.git/HEAD", "ref: refs/heads/main\n");
        std::os::unix::fs::symlink("sub/c.h", layout.src.join("link.h")).unwrap();
        std::fs::set_permissions(
            layout.src.join("bad.cpp"),
            std::fs::Permissions::from_mode(0o640),
        )
        .unwrap();
        std::fs::create_dir_all(&layout.build).unwrap();
        let src = std::fs::canonicalize(&layout.src).unwrap();
        let commit = "c".repeat(40);
        std::fs::write(layout.fetch_stamp(), corpus::fetch_stamp(&c, &commit)).unwrap();
        let cstamp = corpus::configure_stamp(&commit, &["-S".to_owned()]);
        std::fs::write(layout.configure_stamp(), &cstamp).unwrap();
        std::fs::write(layout.generate_stamp(), corpus::generate_stamp(&cstamp, &c.generate))
            .unwrap();
        let db = Value::Array(
            ["a.cpp", "b.cpp", "bad.cpp"]
                .iter()
                .map(|m| {
                    Value::Object(vec![
                        ("directory".to_owned(), Value::Str(src.display().to_string())),
                        ("file".to_owned(), Value::Str(src.join(m).display().to_string())),
                        (
                            "arguments".to_owned(),
                            Value::Array(vec![
                                Value::Str("clang++".to_owned()),
                                Value::Str("-std=c++17".to_owned()),
                                Value::Str(src.join(m).display().to_string()),
                            ]),
                        ),
                    ])
                })
                .collect(),
        );
        std::fs::write(layout.database(), json::write_pretty(&db).unwrap()).unwrap();
        std::fs::create_dir_all(f.path("fake")).unwrap();
        let dir = f.path("fake").display().to_string();
        for (name, text) in [
            ("slip", SLIP),
            ("fakecc", CLANG),
            ("fakecxx", CLANG),
            ("cmake", CMAKE),
            ("make", MAKE),
        ] {
            let p = f.path(&format!("bin/{name}"));
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(&p, text.replace("@DIR@", &dir)).unwrap();
            std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
            common::wait_executable(&p);
        }
        std::fs::write(f.path("fake/patch"), PATCH).unwrap();
        World { f, src }
    }

    fn dir(&self) -> PathBuf {
        self.f.path("corpora/tiny")
    }

    fn store(&self) -> PathBuf {
        self.dir().join("slip-store")
    }

    fn plan(&self) -> Plan {
        let observers = ["a.cpp", "b.cpp", "bad.cpp"]
            .iter()
            .enumerate()
            .map(|(i, m)| {
                let main = self.src.join(m).display().to_string().into_bytes();
                let command = Cmd {
                    directory: self.src.display().to_string().into_bytes(),
                    argv: vec![b"clang++".to_vec(), b"-std=c++17".to_vec(), main.clone()],
                    env: vec![],
                };
                Observer {
                    id: command.observer_id(),
                    command,
                    language: Language::Cxx,
                    main: FileRef { path: main, size: 3 - i as u64, content: d(i as u8) },
                    cost_hint: 300 - 100 * i as u64,
                    origins: vec![Origin { configuration: 0, entry: i as u64 }],
                }
            })
            .collect();
        Plan {
            tool_slip: "0.0.0".into(),
            tool_kiln: "1.3.0".into(),
            configurations: vec![Configuration {
                name: "default".into(),
                database: FileRef {
                    path: self
                        .dir()
                        .join("build/compile_commands.json")
                        .display()
                        .to_string()
                        .into_bytes(),
                    size: 3,
                    content: d(9),
                },
            }],
            observers,
            excluded: vec![],
        }
    }

    /// Publishes the records of a `slip rename` run into the corpus's
    /// store and writes the lines the stand-in prints; returns the edit
    /// set id.
    fn records(&self, r: &Run1) -> Digest {
        let st = Store::open(&self.store(), Access::Exclusive).unwrap();
        let mut plan_obj = self.plan();
        plan_obj.excluded.clone_from(&r.excluded);
        let plan = st.publish_plan(&plan_obj.to_bytes()).unwrap();
        let refused = r.state == State::Refused;
        let mut counters: Vec<Counter> =
            r.counters.iter().map(|(n, v)| Counter { name: (*n).into(), value: *v }).collect();
        counters.sort_by(|a, b| a.name.cmp(&b.name));
        let set = edit_set(plan, counters);
        let set_bytes = set.to_bytes();
        let set_id = st.publish_object(Object::EditSet, &set_bytes).unwrap();
        let overlay = (!refused).then(|| {
            let o = Overlay {
                edits: set_id,
                files: vec![OverlayFile {
                    path: self.src.join("a.cpp").display().to_string().into_bytes(),
                    pre: Content {
                        size: PRE.len() as u64,
                        content: content_digest(PRE.as_bytes()),
                    },
                    post: Content {
                        size: POST.len() as u64,
                        content: content_digest(POST.as_bytes()),
                    },
                }],
                spellings: Vec::new(),
            };
            st.publish_blob(POST.as_bytes()).unwrap();
            st.publish_object(Object::Overlay, &o.to_bytes()).unwrap()
        });
        let (verdict, tally) = match r.state {
            State::Refused => (None, Tally { selected: 3, ..Tally::default() }),
            State::NotVerified => (
                Some(RunVerdict::NotVerified),
                Tally { selected: 3, verified: 2, mismatch: 1, ..Tally::default() },
            ),
            State::Incomplete => (
                Some(RunVerdict::Incomplete),
                Tally { selected: 3, verified: 2, failed: 1, ..Tally::default() },
            ),
            _ => {
                (Some(RunVerdict::Verified), Tally { selected: 3, verified: 3, ..Tally::default() })
            }
        };
        let residue = if r.state == State::VerifiedWithResidue {
            vec![Residue {
                category: Category::Mention,
                file: self.src.join("b.cpp").display().to_string().into_bytes(),
                lo: 0,
                hi: 1,
                line: 1,
                column: 1,
            }]
        } else {
            Vec::new()
        };
        let res = TransformResult {
            scheme: 1,
            plan,
            round: 1,
            transform: d(0x22),
            edits: set_id,
            declared: (!refused).then_some(d(0x44)),
            overlay,
            state: r.state,
            partial: false,
            allow_unparsed: true,
            verdict,
            tally,
            agreement: Vec::new(),
            unused: Vec::new(),
            spellings: result::Spellings { obligations: 0, passed: 0, failed: Vec::new() },
            residue,
            exempt: Vec::new(),
        };
        let res_id = st.publish_object(Object::Result, &res.to_bytes()).unwrap();
        let phase = |phase: Phase, started: u64, rss: u64| {
            let jobs: Vec<Job> = if phase == Phase::Map {
                plan_obj
                    .observers
                    .iter()
                    .enumerate()
                    .map(|(i, o)| {
                        let main = String::from_utf8_lossy(&o.main.path);
                        let (status, msg) = r
                            .map_outcomes
                            .iter()
                            .find(|(name, _, _)| main.ends_with(&format!("/{name}")))
                            .map_or((Status::Ok, ""), |(_, st, m)| (*st, *m));
                        let sh = Shard {
                            observer: o.id,
                            phase: Phase::Map,
                            round: 1,
                            outcome: ShardOutcome {
                                status,
                                errors: u64::from(status != Status::Ok),
                                warnings: 0,
                                message: msg.to_owned(),
                            },
                            inputs: (status != Status::Failed).then(Vec::new),
                            counters: Vec::new(),
                            digests: None,
                            verdict: None,
                            edits: None,
                            binding: None,
                            rule_use: None,
                            spellings: None,
                            names: None,
                            negatives: None,
                            found: None,
                            rebuilt_use: None,
                        };
                        let sh_bytes = sh.to_bytes();
                        let sh_id = id(&sh_bytes);
                        st.publish_shard(&plan, Phase::Map, 1, &o.id, &sh_bytes).unwrap();
                        Job {
                            observer: o.id,
                            attempt: 1,
                            end: End::Completed,
                            shard: Some(sh_id),
                            queued_ns: 0,
                            wall_ns: 1_000,
                            cpu_ns: if i == 0 { 7_000 } else { 0 },
                            peak_rss: rss,
                            estimate: 1 << 20,
                        }
                    })
                    .collect()
            } else {
                vec![Job {
                    observer: plan_obj.observers[0].id,
                    attempt: 1,
                    end: End::Completed,
                    shard: None,
                    queued_ns: 0,
                    wall_ns: 1_000,
                    cpu_ns: 7_000,
                    peak_rss: rss,
                    estimate: 1 << 20,
                }]
            };
            let rec = RunRecord {
                run: Run {
                    tool_slip: "0.0.0".into(),
                    tool_kiln: "1.3.0".into(),
                    plan,
                    phase,
                    round: 1,
                    started_unix_ns: started,
                    wall_ns: 2_000_000,
                    cores: 4,
                    workers_max: 2,
                    memory_budget: 0,
                    peak_rss_sum: rss * 3,
                },
                jobs,
            };
            let bytes = rec.to_bytes();
            st.publish_run(&plan, phase, 1, started, &bytes).unwrap();
            id(&bytes)
        };
        let map = (!r.no_map_run).then(|| phase(Phase::Map, r.started + 1, 1 << 20));
        let verify = (!refused).then(|| phase(Phase::Verify, r.started + 2, 2 << 20));
        let step = |name, run| Step { name, wall_ns: r.wall_ns / 4, cpu_ns: 1, peak_rss: 9, run };
        let mut steps = vec![
            step(StepName::Plan, None),
            step(StepName::Map, map),
            step(StepName::Reduce, None),
        ];
        if let Some(v) = verify {
            steps.push(step(StepName::Verify, Some(v)));
        }
        steps.push(step(StepName::Conclude, None));
        let tr = TransformRun {
            tool_slip: "0.0.0".into(),
            tool_kiln: "1.3.0".into(),
            plan,
            round: 1,
            transform: d(0x22),
            edits: Some(set_id),
            declared: res.declared,
            overlay,
            result: Some(res_id),
            started_unix_ns: r.started,
            wall_ns: r.wall_ns,
            steps,
        };
        st.publish_transform_run(&plan, 1, r.started, &tr.to_bytes()).unwrap();
        drop(st);
        let mut out = format!("plan {plan}\nround 1\nedits {set_id}\n");
        if let Some(o) = overlay {
            out += &format!("overlay {o}\n");
        }
        out += &format!("result {res_id}\nstate {}\n", r.state.name());
        if !refused {
            out += &format!("patch {}\n", self.dir().join("rename/lib.f.patch").display());
        }
        let status = match r.state {
            State::Verified => 0,
            State::Incomplete => 4,
            _ => 1,
        };
        if refused {
            let _ = std::fs::remove_file(self.f.path("fake/patch"));
        }
        std::fs::write(self.f.path("fake/rename.out"), out).unwrap();
        std::fs::write(self.f.path("fake/rename.status"), status.to_string()).unwrap();
        std::fs::write(self.f.path("fake/edits"), set_id.to_string()).unwrap();
        self.seed_census(0);
        set_id
    }

    /// Seeds the stand-in's census store: `indeterminate` keys of cause
    /// `unsupported` on the first observer (0 = clean).
    fn seed_census(&self, indeterminate: u64) {
        let store_dir = self.f.path("fake/census.store");
        let _ = std::fs::remove_dir_all(&store_dir);
        let st = Store::open(&store_dir, Access::Exclusive).unwrap();
        let p = self.plan();
        let plan_id = st.publish_plan(&p.to_bytes()).unwrap();
        let mut jobs = Vec::new();
        for (i, o) in p.observers.iter().enumerate() {
            let k = if i == 0 { indeterminate } else { 0 };
            let mut counters = vec![Counter { name: "digest.indeterminate".into(), value: k }];
            if k > 0 {
                counters
                    .push(Counter { name: "digest.indeterminate.unsupported".into(), value: k });
                counters.push(Counter {
                    name: "digest.cause.unsupported.exprkind.cxxreflectexpr".into(),
                    value: k,
                });
            }
            counters.sort();
            let groups = vec![Group {
                class: GroupClass::Tu,
                file: vec![],
                count: k.max(1),
                indeterminate: k,
                digest: d(1),
                units: None,
            }];
            let sh = Shard {
                observer: o.id,
                phase: Phase::Map,
                round: 0,
                outcome: ShardOutcome {
                    status: Status::Ok,
                    errors: 0,
                    warnings: 0,
                    message: String::new(),
                },
                inputs: Some(Vec::new()),
                counters,
                digests: Some(Digests {
                    scheme: digests::SCHEME,
                    tu: digests::tu_digest(&groups),
                    groups,
                }),
                verdict: None,
                edits: None,
                binding: None,
                rule_use: None,
                spellings: None,
                names: Some(slip_model::names::Names::default()),
                negatives: Some(Vec::new()),
                found: None,
                rebuilt_use: None,
            };
            let bytes = sh.to_bytes();
            let sid = id(&bytes);
            st.publish_shard(&plan_id, Phase::Map, 0, &o.id, &bytes).unwrap();
            jobs.push(Job {
                observer: o.id,
                attempt: 1,
                end: End::Completed,
                shard: Some(sid),
                queued_ns: 0,
                wall_ns: 1_000,
                cpu_ns: 1_000,
                peak_rss: 1 << 20,
                estimate: 1 << 20,
            });
        }
        let rec = RunRecord {
            run: Run {
                tool_slip: "0.0.0".into(),
                tool_kiln: "1.3.0".into(),
                plan: plan_id,
                phase: Phase::Map,
                round: 0,
                started_unix_ns: STARTED,
                wall_ns: 1_000,
                cores: 4,
                workers_max: 2,
                memory_budget: 1 << 30,
                peak_rss_sum: 0,
            },
            jobs,
        };
        st.publish_run(&plan_id, Phase::Map, 0, STARTED, &rec.to_bytes()).unwrap();
        let rel = st
            .run_path(&plan_id, Phase::Map, 0, STARTED)
            .strip_prefix(&store_dir)
            .unwrap()
            .to_string_lossy()
            .into_owned();
        drop(st);
        std::fs::write(self.f.path("fake/census.plan"), plan_id.to_string()).unwrap();
        std::fs::write(self.f.path("fake/census.run"), rel).unwrap();
    }

    fn env(&self) -> Env {
        let path = format!("{}:/usr/bin:/bin", self.f.path("bin").display());
        Env {
            corpus_dir: Some(self.f.path("corpora").into()),
            target_dir: None,
            path: Some(path.into()),
        }
    }

    fn run(&self, args: &[&str]) -> (Result<bool, Failure>, String) {
        let mut a: Vec<String> = vec!["tiny".to_owned()];
        a.extend(["--from", "lib::f", "--to", "h"].iter().map(|s| (*s).to_owned()));
        a.extend(args.iter().map(|s| (*s).to_owned()));
        a.extend(["--slip".to_owned(), self.f.path("bin/slip").display().to_string()]);
        let mut out = Vec::new();
        let r = rename::run(&self.f.path("repo"), &a, &self.env(), &mut out);
        (r, String::from_utf8(out).unwrap())
    }

    fn log(&self) -> String {
        std::fs::read_to_string(self.f.path("fake/log")).unwrap_or_default()
    }

    fn result_file(&self) -> Value {
        json::parse(&std::fs::read(self.f.path(RESULT_FILE)).unwrap()).unwrap()
    }
}

/// `patch` and `df`, which `--build` runs for real; `None` (skip) if one is
/// missing and `SLIP_REQUIRE_ALL` is not set.
fn tools_present() -> bool {
    let path = std::env::var_os("PATH");
    let ok = ["patch", "df"].iter().all(|t| corpus::find_tool(t, path.as_deref()).is_ok());
    if !ok {
        assert!(std::env::var_os("SLIP_REQUIRE_ALL").is_none(), "patch or df is not on PATH");
        eprintln!("skipped: patch or df is not on PATH");
    }
    ok
}

/// The `check …` lines of `out`, in order.
fn checks(out: &str) -> Vec<&str> {
    out.lines().filter(|l| l.starts_with("check ")).collect()
}

/// The position of the first line of `out` that starts with `prefix`.
fn at(out: &str, prefix: &str) -> usize {
    out.lines()
        .position(|l| l.starts_with(prefix))
        .unwrap_or_else(|| panic!("no `{prefix}` in\n{out}"))
}

// §6.5: a rename that verifies, with every check, runs the checks in
// order (each step's start and end printed), uses `--patch` and never
// `--write` on the corpus, `--write` only on the copy, removes the copy
// and its build tree, and writes the result file.
#[test]
fn s6_5_rename_verifies() {
    if !tools_present() {
        return;
    }
    let w = World::new("rename-verifies");
    w.records(&Run1::of(State::Verified));
    let (r, out) = w.run(&["--accept", "--build", "-j", "2"]);
    assert!(matches!(r, Ok(true)), "{r:?}\n{out}");
    let c = checks(&out);
    assert_eq!(c.len(), 6, "{out}");
    for (line, name) in
        c.iter().zip(["coverage", "state", "determinism", "accept", "build", "back"])
    {
        assert!(line.starts_with(&format!("check {name}: ok")), "{line}\n{out}");
    }
    assert!(c[3].contains("3 commands; 0 rejected on both"), "{}", c[3]);
    // Steps start and end in order, each check after its steps.
    let order = [
        "step rename: started",
        "step rename: done",
        "check coverage:",
        "check state:",
        "step reduce: started",
        "step reduce: done",
        "check determinism:",
        "step accept: started",
        "step accept: done",
        "check accept:",
        "step copy: started",
        "step copy: done",
        "step patch: started",
        "step patch: done",
        "step configure: started",
        "step configure: done",
        "step generate: started",
        "step generate: done",
        "step build: started",
        "step build: done",
        "check build:",
        "step back: started",
        "back| state verified",
        "step back: done",
        "step compare: started",
        "step compare: done",
        "check back:",
        "removed ",
        "counters: new",
        "result bench/results/tiny.rename.lib.f.json",
        "corpus rename: ok",
    ];
    let pos: Vec<usize> = order.iter().map(|p| at(&out, p)).collect();
    assert!(pos.windows(2).all(|p| p[0] < p[1]), "{out}");
    // The lines of `slip rename` are printed.
    assert!(out.contains("\nstate verified\n"), "{out}");
    // The calls: the pre-flight census (`--version`, `parse`), the corpus
    // rename with --patch and never --write, `reduce`, and the inverse over
    // the copy with --write.
    let log = w.log();
    let calls: Vec<&str> =
        log.lines().filter(|l| !l.starts_with("make ") && !l.starts_with("cmake ")).collect();
    assert_eq!(calls.len(), 5, "{log}");
    let dir = w.dir().display().to_string();
    assert!(calls[0].starts_with("--version"), "{}", calls[0]);
    assert!(calls[1].starts_with("parse --db "), "{}", calls[1]);
    assert!(calls[1].contains(&format!("--store {dir}/stores/census -j 2")), "{}", calls[1]);
    let first = calls[2];
    assert!(first.starts_with("rename --from lib::f --to h --db "), "{first}");
    for word in ["--allow-unparsed", "--allow-residue", "--patch", "--relative-to", "-j 2"] {
        assert!(first.contains(word), "{first}");
    }
    assert!(first.contains(&format!("--owned {}", w.src.display())), "{first}");
    assert!(first.contains(&format!("--store {dir}/slip-store")), "{first}");
    assert!(!first.contains("--write"), "{first}");
    assert!(calls[3].starts_with("reduce --plan "), "{}", calls[3]);
    assert!(calls[3].contains(&format!("--store {dir}/slip-store")), "{}", calls[3]);
    let back = calls[4];
    assert!(back.starts_with("rename --from lib::h --to f --db "), "{back}");
    assert!(back.contains(&format!("{dir}/rename-build/compile_commands.json")), "{back}");
    assert!(back.contains(&format!("--owned {dir}/rename-copy")), "{back}");
    assert!(back.contains("--write"), "{back}");
    assert!(!back.contains("--patch"), "{back}");
    // cmake configured the copy; make ran the generate target and `all`.
    assert!(log.contains(&format!("-S {dir}/rename-copy -B {dir}/rename-build")), "{log}");
    assert!(log.contains("make gen\n") && log.contains("make all\n"), "{log}");
    // Both trees removed; the corpus source untouched.
    assert!(!w.dir().join("rename-copy").exists());
    assert!(!w.dir().join("rename-build").exists());
    assert_eq!(std::fs::read_to_string(w.src.join("a.cpp")).unwrap(), PRE);
    assert!(w.dir().join("rename/lib.f.patch").is_file());
    // Each unit compiled on both images, through the overlay file.
    let clang = std::fs::read_to_string(w.f.path("fake/clang.log")).unwrap();
    let mut runs: Vec<&str> = clang.lines().collect();
    runs.sort_unstable();
    assert_eq!(runs, ["a.cpp ", "a.cpp post", "b.cpp ", "b.cpp post", "bad.cpp ", "bad.cpp post"]);
    let vfs = std::fs::read(w.dir().join("rename/lib.f.vfs.json")).unwrap();
    let vfs = json::parse(&vfs).unwrap();
    let root = &vfs.get("roots").unwrap().as_array().unwrap()[0];
    assert_eq!(root.get("name").unwrap().as_str(), Some(w.src.display().to_string().as_str()));
    let entry = &root.get("contents").unwrap().as_array().unwrap()[0];
    assert_eq!(entry.get("name").unwrap().as_str(), Some("a.cpp"));
    let blob = entry.get("external-contents").unwrap().as_str().unwrap();
    assert_eq!(std::fs::read_to_string(blob).unwrap(), POST);
    assert!(w.f.path(RESULT_FILE).is_file());
    assert!(!rename::new_path(&w.f.path(RESULT_FILE)).exists());
}

// §6.5, check 1: `--expect refused` holds for a refused result, whose
// missing post-image skips the checks that need it; without `--expect` a
// refused result fails check 1 and the rest are not run.
#[test]
fn s6_5_expect_refused() {
    let w = World::new("rename-refused");
    w.records(&Run1::of(State::Refused));
    let (r, out) = w.run(&["--expect", "refused", "--accept"]);
    assert!(matches!(r, Ok(true)), "{r:?}\n{out}");
    let c = checks(&out);
    assert_eq!(c[0], "check coverage: ok (3 of 3 observers ok)");
    assert_eq!(c[1], "check state: ok (refused)");
    assert!(c[2].starts_with("check determinism: ok"), "{}", c[2]);
    assert_eq!(c[3], "check accept: skipped (the result is refused: there is no post-image)");
    assert_eq!(c[4], "check build: skipped (the result is refused: there is no post-image)");
    let doc = w.result_file();
    assert_eq!(doc.get("state").unwrap().as_str(), Some("refused"));
    assert_eq!(doc.get("overlay"), Some(&Value::Null));
    // Only the map phase is measured: a refused run has no verify phase.
    let phases = doc.get("measured").unwrap().get("phases").unwrap().as_array().unwrap();
    assert_eq!(phases.len(), 1);
    assert!(!w.f.path("fake/clang.log").exists());

    let w = World::new("rename-refused-unexpected");
    w.records(&Run1::of(State::Refused));
    let (r, out) = w.run(&[]);
    assert!(matches!(r, Ok(false)), "{r:?}\n{out}");
    let c = checks(&out);
    assert_eq!(c[0], "check coverage: ok (3 of 3 observers ok)");
    assert_eq!(
        c[1],
        "check state: FAIL (state refused, expected verified or verified_with_residue)"
    );
    for (line, name) in c[2..].iter().zip(["determinism", "accept", "build", "back"]) {
        assert_eq!(*line, format!("check {name}: not run (an earlier check failed)"));
    }
    assert!(out.ends_with("corpus rename: FAILED (state)\n"), "{out}");
    // A failing run does not write the result file, only `<file>.new`.
    assert!(!w.f.path(RESULT_FILE).exists());
    assert!(rename::new_path(&w.f.path(RESULT_FILE)).is_file());
    assert!(!w.log().contains("reduce"), "{}", w.log());
}

// §6.5, check 2: a `slip reduce` that gives another edit set id fails
// determinism; the later checks are not run.
#[test]
fn s6_5_determinism_fails() {
    let w = World::new("rename-determinism");
    w.records(&Run1::of(State::Verified));
    std::fs::write(w.f.path("fake/reduce.edits"), d(0x77).to_string()).unwrap();
    let (r, out) = w.run(&["--accept"]);
    assert!(matches!(r, Ok(false)), "{r:?}\n{out}");
    let c = checks(&out);
    assert!(c[0].starts_with("check coverage: ok"));
    assert!(c[1].starts_with("check state: ok"));
    assert!(
        c[2].starts_with(&format!(
            "check determinism: FAIL (`slip reduce` gave the edit set {}",
            d(0x77)
        )),
        "{}",
        c[2]
    );
    assert_eq!(c[3], "check accept: not run (an earlier check failed)");
    assert!(out.contains(&format!("reduce| edits {}", d(0x77))), "{out}");
    assert!(!w.f.path("fake/clang.log").exists());
    assert!(out.ends_with("corpus rename: FAILED (determinism)\n"), "{out}");
}

// Check 1 (`coverage`, via `perf::load`) reads the plan by id and compares it
// with the id (review of WP-161, note 7; WP-169 moves the first plan read
// from `accept` to `coverage`, while `accept` still verifies on its own
// read): a plan stored under another's id fails the check.
#[test]
fn s6_5_accept_refuses_a_plan_under_another_id() {
    let w = World::new("rename-plan-id");
    w.records(&Run1::of(State::Verified));
    let plans = w.store().join("plans");
    let name = std::fs::read_dir(&plans).unwrap().next().unwrap().unwrap().path();
    let mut other = w.plan();
    other.tool_slip = "another".into();
    std::fs::write(&name, other.to_bytes()).unwrap();
    let (r, out) = w.run(&["--accept", "-j", "1"]);
    assert!(matches!(r, Ok(false)), "{r:?}\n{out}");
    assert!(out.contains("check coverage: FAIL"), "{out}");
    assert!(out.contains("the plan's content id is"), "{out}");
    assert!(out.contains("check accept: not run"), "{out}");
}

// §6.5, check 3: a unit clang rejects on both images passes; one it
// rejects only on the post-image fails, named with its first error.
#[test]
fn s6_5_accept() {
    let w = World::new("rename-accept");
    w.records(&Run1::of(State::Verified));
    std::fs::write(w.f.path("fake/reject"), "bad.cpp\n").unwrap();
    let (r, out) = w.run(&["--accept", "-j", "3"]);
    assert!(matches!(r, Ok(true)), "{r:?}\n{out}");
    assert!(
        out.contains("check accept: ok (3 commands; 1 rejected on both images, none otherwise)"),
        "{out}"
    );
    assert!(out.contains("accept: 3 of 3 commands"), "{out}");

    std::fs::write(w.f.path("fake/reject.after"), "b.cpp\n").unwrap();
    let (r, out) = w.run(&["--accept", "-j", "1"]);
    assert!(matches!(r, Ok(false)), "{r:?}\n{out}");
    assert!(
        out.contains("check accept: FAIL (1 of 3 commands end otherwise on the post-image)"),
        "{out}"
    );
    let b = w.src.join("b.cpp").display().to_string();
    assert!(
        out.contains(&format!(
            "  {b}: pre-image exit 0, post-image exit 1: b.cpp:2:5: error: use of undeclared identifier 'h'"
        )),
        "{out}"
    );
    assert!(out.contains("check build: not run"), "{out}");
}

// §6.5, the result file: its members, no absolute path, the counters,
// tally, residue and measurements of the run's records.
#[test]
fn s6_5_result_file() {
    let w = World::new("rename-file");
    w.records(&Run1::of(State::VerifiedWithResidue));
    let (r, out) = w.run(&["--kind", "function", "-j", "2"]);
    assert!(matches!(r, Ok(true)), "{r:?}\n{out}");
    let text = std::fs::read_to_string(w.f.path(RESULT_FILE)).unwrap();
    assert!(!text.contains(&w.f.root.display().to_string()), "{text}");
    let doc = w.result_file();
    let keys: Vec<&str> = doc.as_object().unwrap().iter().map(|(k, _)| k.as_str()).collect();
    assert_eq!(
        keys,
        [
            "corpus",
            "pin",
            "from",
            "to",
            "kind",
            "options",
            "tool_slip",
            "tool_kiln",
            "plan",
            "round",
            "transform",
            "edits",
            "declared",
            "overlay",
            "result",
            "checks",
            "state",
            "counters",
            "tally",
            "agreement",
            "spellings",
            "residue",
            "not_plain_at_macro",
            "measured"
        ]
    );
    let s = |k: &str| doc.get(k).unwrap().as_str().unwrap().to_owned();
    assert_eq!(s("corpus"), "tiny");
    assert_eq!(s("pin"), "v1.0");
    assert_eq!(s("from"), "lib::f");
    assert_eq!(s("to"), "h");
    assert_eq!(s("kind"), "function");
    assert_eq!(s("state"), "verified_with_residue");
    assert_eq!(s("tool_slip"), "0.0.0");
    let opts = doc.get("options").unwrap();
    assert_eq!(opts.get("owned").unwrap().as_array().unwrap(), &[Value::Str(".".into())]);
    assert_eq!(opts.get("jobs"), Some(&Value::Int(2)));
    assert_eq!(opts.get("accept"), Some(&Value::Bool(false)));
    assert_eq!(opts.get("skip_census"), Some(&Value::Bool(false)));
    let counters = doc.get("counters").unwrap();
    assert_eq!(counters.get("edits.planned"), Some(&Value::Int(2)));
    assert_eq!(counters.get("edits.kept"), Some(&Value::Int(1)));
    assert_eq!(doc.get("tally").unwrap().get("verified"), Some(&Value::Int(3)));
    let residue = doc.get("residue").unwrap();
    assert_eq!(residue.get("mention"), Some(&Value::Int(1)));
    assert_eq!(residue.get("dependent"), Some(&Value::Int(0)));
    let c = doc.get("checks").unwrap().as_array().unwrap();
    assert_eq!(c.len(), 6);
    assert_eq!(c[0].get("check").unwrap().as_str(), Some("coverage"));
    assert_eq!(c[0].get("outcome").unwrap().as_str(), Some("ok"));
    assert_eq!(c[1].get("check").unwrap().as_str(), Some("state"));
    assert_eq!(c[1].get("outcome").unwrap().as_str(), Some("ok"));
    assert_eq!(c[3].get("outcome").unwrap().as_str(), Some("skipped"));
    let m = doc.get("measured").unwrap();
    assert_eq!(m.get("wall_ns"), Some(&Value::Int(5_000_000)));
    let steps = m.get("steps").unwrap().as_array().unwrap();
    let names: Vec<&str> = steps.iter().map(|s| s.get("step").unwrap().as_str().unwrap()).collect();
    assert_eq!(names, ["plan", "map", "reduce", "verify", "conclude"]);
    assert_eq!(steps[0].get("peak_rss"), Some(&Value::Int(9)));
    let phases = m.get("phases").unwrap().as_array().unwrap();
    assert_eq!(phases[0].get("phase").unwrap().as_str(), Some("map"));
    assert_eq!(phases[0].get("peak_rss_max"), Some(&Value::Int(1 << 20)));
    assert_eq!(phases[1].get("peak_rss_sum"), Some(&Value::Int(3 * (2 << 20))));
    assert_eq!(phases[1].get("cpu_ns"), Some(&Value::Int(7_000)));
    let mc = m.get("checks").unwrap().as_array().unwrap();
    assert_eq!(mc.len(), 6);
    assert_eq!(mc[0].get("check").unwrap().as_str(), Some("coverage"));
}

// §6.5: the counters are compared exactly with the file there; times and
// sizes are not.
#[test]
fn s6_5_counters_compared() {
    let w = World::new("rename-counters");
    w.records(&Run1::of(State::Verified));
    let (r, out) = w.run(&[]);
    assert!(matches!(r, Ok(true)), "{r:?}\n{out}");
    let first = std::fs::read_to_string(w.f.path(RESULT_FILE)).unwrap();

    // Another run, later and slower: the same counters pass.
    let mut slower = Run1::of(State::Verified);
    slower.started = STARTED + 1_000;
    slower.wall_ns = 9_000_000;
    // The same result: the transform run record differs only in times.
    w.records(&slower);
    let (r, out) = w.run(&[]);
    assert!(matches!(r, Ok(true)), "{r:?}\n{out}");
    assert!(out.contains("counters: the same as bench/results/tiny.rename.lib.f.json"), "{out}");
    let doc = w.result_file();
    assert_eq!(doc.get("measured").unwrap().get("wall_ns"), Some(&Value::Int(9_000_000)));

    // Another counter value fails and leaves the file.
    let mut other = Run1::of(State::Verified);
    other.counters = vec![("edits.kept", 1), ("edits.planned", 3)];
    other.started = STARTED + 2_000;
    w.records(&other);
    let before = std::fs::read_to_string(w.f.path(RESULT_FILE)).unwrap();
    let (r, out) = w.run(&[]);
    assert!(matches!(r, Ok(false)), "{r:?}\n{out}");
    assert!(
        out.contains("counters: DIFFERENT from bench/results/tiny.rename.lib.f.json (1 member(s))"),
        "{out}"
    );
    assert!(out.contains("  counters.edits.planned: 2 -> 3"), "{out}");
    assert!(out.ends_with("corpus rename: FAILED (counters)\n"), "{out}");
    assert_eq!(std::fs::read_to_string(w.f.path(RESULT_FILE)).unwrap(), before);
    assert_ne!(before, first);
    let new =
        json::parse(&std::fs::read(rename::new_path(&w.f.path(RESULT_FILE))).unwrap()).unwrap();
    assert_eq!(new.get("counters").unwrap().get("edits.planned"), Some(&Value::Int(3)));
}

// The comparison itself: only the exact members count.
#[test]
fn s6_5_exact_differences() {
    let doc = |planned: i128, wall: i128, state: &str| {
        let o = |m: Vec<(&str, Value)>| {
            Value::Object(m.into_iter().map(|(k, v)| (k.to_owned(), v)).collect())
        };
        o(vec![
            ("state", Value::Str(state.into())),
            ("counters", o(vec![("edits.planned", Value::Int(planned))])),
            ("tally", o(vec![])),
            ("measured", o(vec![("wall_ns", Value::Int(wall))])),
        ])
    };
    assert!(rename::exact_differences(&doc(2, 1, "verified"), &doc(2, 99, "verified")).is_empty());
    assert_eq!(
        rename::exact_differences(&doc(2, 1, "verified"), &doc(3, 1, "not_verified")),
        [
            "state: \"verified\" -> \"not_verified\"".to_owned(),
            "counters.edits.planned: 2 -> 3".to_owned(),
        ]
    );
}

/// Asserts the kept copy is a true copy of the source: no `.git`, the
/// link a link, the mode kept, one link per file, other inodes.
fn assert_true_copy(w: &World) {
    let copy = w.dir().join("rename-copy");
    assert!(!copy.join(".git").exists());
    assert_eq!(std::fs::read_link(copy.join("link.h")).unwrap(), Path::new("sub/c.h"));
    let m = std::fs::metadata(copy.join("bad.cpp")).unwrap();
    assert_eq!(m.permissions().mode() & 0o777, 0o640);
    for f in ["a.cpp", "b.cpp", "bad.cpp", "sub/c.h"] {
        let (a, b) =
            (std::fs::metadata(w.src.join(f)).unwrap(), std::fs::metadata(copy.join(f)).unwrap());
        assert_eq!(b.nlink(), 1, "{f}");
        assert_eq!(a.nlink(), 1, "{f}");
        assert_ne!(a.ino(), b.ino(), "{f}");
    }
}

// §6.5: a failing check keeps the copy and its build tree, printing their
// paths. Here the inverse rename does not restore the copy (back), and,
// in a second corpus, the build fails (build; back not run).
#[test]
fn s6_5_failing_check_keeps_the_copy() {
    if !tools_present() {
        return;
    }
    let w = World::new("rename-keep-back");
    w.records(&Run1::of(State::Verified));
    std::fs::write(w.f.path("fake/back.broken"), "").unwrap();
    let (r, out) = w.run(&["--build"]);
    assert!(matches!(r, Ok(false)), "{r:?}\n{out}");
    assert!(out.contains("check build: ok"), "{out}");
    assert!(
        out.contains(
            "check back: FAIL (the inverse rename ended verified; 1 difference(s) from the source)"
        ),
        "{out}"
    );
    assert!(out.contains("\n  a.cpp: other bytes\n"), "{out}");
    let copy = w.dir().join("rename-copy");
    let build = w.dir().join("rename-build");
    assert!(out.contains(&format!("kept {}", copy.display())), "{out}");
    assert!(out.contains(&format!("kept {}", build.display())), "{out}");
    assert_eq!(std::fs::read_to_string(copy.join("a.cpp")).unwrap(), POST);
    assert!(build.join("build.log").is_file());
    assert_true_copy(&w);
    assert_eq!(std::fs::read_to_string(w.src.join("a.cpp")).unwrap(), PRE);

    // A second run removes the kept trees of the first before it copies.
    std::fs::remove_file(w.f.path("fake/back.broken")).unwrap();
    let (r, out) = w.run(&["--build"]);
    assert!(matches!(r, Ok(true)), "{r:?}\n{out}");
    assert!(
        out.contains(&format!("build: removing {} of an earlier run", copy.display())),
        "{out}"
    );
    assert!(!copy.exists() && !build.exists());

    let w = World::new("rename-keep-build");
    w.records(&Run1::of(State::Verified));
    std::fs::write(w.f.path("fake/make.fail"), "").unwrap();
    let (r, out) = w.run(&["--build"]);
    assert!(matches!(r, Ok(false)), "{r:?}\n{out}");
    assert!(out.contains("check build: FAIL (generate: make failed"), "{out}");
    assert!(out.contains("check back: not run"), "{out}");
    assert!(out.contains(&format!("kept {}", w.dir().join("rename-copy").display())), "{out}");
    assert_true_copy(&w);
    assert!(!w.log().contains("--write"), "{}", w.log());
}

// Usage errors and a corpus that is not configured.
#[test]
fn s6_5_usage() {
    let w = World::new("rename-usage");
    let run = |args: &[&str]| {
        let a: Vec<String> = args.iter().map(|s| (*s).to_owned()).collect();
        rename::run(&w.f.path("repo"), &a, &w.env(), &mut Vec::new())
    };
    for bad in [
        &["tiny", "--to", "h"][..],
        &["tiny", "--from", "lib::f"],
        &["tiny", "--from", "lib/f", "--to", "h"],
        &["tiny", "--from", "lib::f", "--to", "a::h"],
        &["tiny", "--from", "lib::f", "--to", "h", "--expect", "fine"],
        &["tiny", "--from", "lib::f", "--to", "h", "-j", "0"],
        &["tiny", "--from", "lib::f", "--to", "h", "--accept", "--accept"],
        &["tiny", "--from", "lib::f", "--to", "h", "--write"],
        &["--from", "lib::f", "--to", "h"],
    ] {
        assert!(matches!(run(bad), Err(Failure::Usage(_))), "{bad:?}");
    }
    std::fs::remove_file(w.dir().join("generate.stamp")).unwrap();
    let r = run(&["tiny", "--from", "lib::f", "--to", "h", "--slip", "/nonexistent"]);
    assert!(matches!(&r, Err(Failure::Error(e)) if e.contains("is not configured")), "{r:?}");
    // `--owned` outside the source directory is an error before slip runs.
    let w = World::new("rename-owned");
    w.records(&Run1::of(State::Verified));
    let outside = w.f.path("repo").display().to_string();
    let (r, _) = w.run(&["--owned", &outside]);
    assert!(
        matches!(&r, Err(Failure::Error(e)) if e.contains("not under the corpus's source directory")),
        "{r:?}"
    );
    assert!(w.log().is_empty());
}

// The copy is a true copy, and the comparison finds every kind of
// difference.
#[test]
fn copy_and_compare() {
    let w = World::new("rename-copy");
    let to = w.f.path("copy");
    let (files, bytes) = rename::copy_tree(&w.src, &to, Some(".git")).unwrap();
    assert_eq!(files, 4);
    assert_eq!(bytes, rename::tree_size(&w.src, Some(".git")).unwrap());
    assert!(rename::copy_tree(&w.src, &to, None).unwrap_err().contains("exists already"));
    let (n, lines) = rename::tree_differences(&w.src, &to, Some(".git"), 20).unwrap();
    assert_eq!((n, lines.len()), (0, 0));
    let t = std::fs::metadata(w.src.join("a.cpp")).unwrap().modified().unwrap();
    assert_eq!(std::fs::metadata(to.join("a.cpp")).unwrap().modified().unwrap(), t);
    std::fs::write(to.join("a.cpp"), POST).unwrap();
    std::fs::remove_file(to.join("b.cpp")).unwrap();
    std::fs::write(to.join("new.cpp"), "").unwrap();
    std::fs::remove_file(to.join("link.h")).unwrap();
    std::os::unix::fs::symlink("a.cpp", to.join("link.h")).unwrap();
    std::fs::remove_file(to.join("sub/c.h")).unwrap();
    std::fs::create_dir(to.join("sub/c.h")).unwrap();
    let (n, lines) = rename::tree_differences(&w.src, &to, Some(".git"), 3).unwrap();
    assert_eq!(n, 5);
    assert_eq!(
        lines,
        ["a.cpp: other bytes", "b.cpp: only in the source", "link.h: a link to another target"]
    );
    let (_, all) = rename::tree_differences(&w.src, &to, Some(".git"), 20).unwrap();
    assert_eq!(all[3..], ["new.cpp: only in the copy", "sub/c.h: of another kind in the copy"]);
}

// The count of `not_plain` refusals at a macro invocation, which demo-2's
// record reads from the result file.
#[test]
fn s6_5_not_plain_at_macro() {
    let mut set = edit_set(d(1), Vec::new());
    let refusal = |reason, macro_site| Refusal {
        cause: Cause::SiteRefused,
        file: Some(0),
        lo: 1,
        hi: 2,
        reason,
        subject: None,
        display: None,
        macro_site,
        other: None,
        observers: 3,
        first: Vec::new(),
    };
    let site = || Some(MacroSite { name_start: 0, name: b"M".to_vec() });
    set.refusals.extend([
        refusal(Reason::NotPlain, site()),
        refusal(Reason::NotPlain, None),
        refusal(Reason::MacroBody, site()),
        refusal(Reason::NotPlain, site()),
    ]);
    assert_eq!(rename::not_plain_at_macro(&set), 2);
}

// Check 2 (`state`): a state that permits a patch, but `slip` wrote none (its
// `--patch` refused at apply), fails.
#[test]
fn s6_5_verified_without_a_patch() {
    let w = World::new("rename-no-patch");
    w.records(&Run1::of(State::Verified));
    std::fs::remove_file(w.f.path("fake/patch")).unwrap();
    let (r, out) = w.run(&[]);
    assert!(matches!(r, Ok(false)), "{r:?}\n{out}");
    assert_eq!(
        checks(&out)[1],
        "check state: FAIL (state verified, but slip wrote no patch: its lines say why)"
    );
}

// WP-171 item 1: a compiler crash (status != 0/1, signal, or crash text on
// stderr) is never a rejection, even when both images fail the same way.
#[test]
fn s6_5_accept_crash_is_not_rejection() {
    // Exit 139 on both images: FAILS, naming the command.
    let w = World::new("rename-accept-crash-both");
    w.records(&Run1::of(State::Verified));
    std::fs::write(w.f.path("fake/crash"), "bad.cpp\n").unwrap();
    let (r, out) = w.run(&["--accept", "-j", "1"]);
    assert!(matches!(r, Ok(false)), "{r:?}\n{out}");
    assert!(out.contains("check accept: FAIL"), "{out}");
    let bad = w.src.join("bad.cpp").display().to_string();
    assert!(out.contains(&format!("  {bad}: pre-image exit 139, post-image exit 139")), "{out}");

    // Exit 139 on pre-image and exit 1 on post-image: FAILS, naming the command.
    let w = World::new("rename-accept-crash-pre-reject-post");
    w.records(&Run1::of(State::Verified));
    std::fs::write(w.f.path("fake/crash.before"), "bad.cpp\n").unwrap();
    std::fs::write(w.f.path("fake/reject.after"), "bad.cpp\n").unwrap();
    let (r, out) = w.run(&["--accept", "-j", "1"]);
    assert!(matches!(r, Ok(false)), "{r:?}\n{out}");
    assert!(out.contains("check accept: FAIL"), "{out}");
    let bad = w.src.join("bad.cpp").display().to_string();
    assert!(out.contains(&format!("  {bad}: pre-image exit 139, post-image exit 1")), "{out}");

    // Exit 1 on pre-image and exit 139 on post-image: FAILS, naming the command.
    let w = World::new("rename-accept-reject-pre-crash-post");
    w.records(&Run1::of(State::Verified));
    std::fs::write(w.f.path("fake/reject"), "bad.cpp\n").unwrap();
    std::fs::write(w.f.path("fake/crash.after"), "bad.cpp\n").unwrap();
    let (r, out) = w.run(&["--accept", "-j", "1"]);
    assert!(matches!(r, Ok(false)), "{r:?}\n{out}");
    assert!(out.contains("check accept: FAIL"), "{out}");
    let bad = w.src.join("bad.cpp").display().to_string();
    assert!(out.contains(&format!("  {bad}: pre-image exit 1, post-image exit 139")), "{out}");

    // Exit 1 on both images with crash text ("PLEASE submit a bug report"): FAILS.
    let w = World::new("rename-accept-crash-text");
    w.records(&Run1::of(State::Verified));
    std::fs::write(w.f.path("fake/crash.text"), "bad.cpp\n").unwrap();
    let (r, out) = w.run(&["--accept", "-j", "1"]);
    assert!(matches!(r, Ok(false)), "{r:?}\n{out}");
    assert!(out.contains("check accept: FAIL"), "{out}");
    let bad = w.src.join("bad.cpp").display().to_string();
    assert!(
        out.contains(&format!(
            "  {bad}: pre-image crashed (exit 1), post-image crashed (exit 1): PLEASE submit a bug report"
        )),
        "{out}"
    );

    // Exit 1 on both images with "frontend command failed due to signal": FAILS.
    let w = World::new("rename-accept-crash-signal-text");
    w.records(&Run1::of(State::Verified));
    std::fs::write(w.f.path("fake/crash.signal_text"), "bad.cpp\n").unwrap();
    let (r, out) = w.run(&["--accept", "-j", "1"]);
    assert!(matches!(r, Ok(false)), "{r:?}\n{out}");
    assert!(out.contains("check accept: FAIL"), "{out}");
    let bad = w.src.join("bad.cpp").display().to_string();
    assert!(
        out.contains(&format!(
            "  {bad}: pre-image crashed (exit 1), post-image crashed (exit 1): clang: error: clang frontend command failed due to signal"
        )),
        "{out}"
    );

    // Killed by signal on both images: FAILS.
    let w = World::new("rename-accept-crash-signal");
    w.records(&Run1::of(State::Verified));
    std::fs::write(w.f.path("fake/crash.signal"), "bad.cpp\n").unwrap();
    let (r, out) = w.run(&["--accept", "-j", "1"]);
    assert!(matches!(r, Ok(false)), "{r:?}\n{out}");
    assert!(out.contains("check accept: FAIL"), "{out}");
    let bad = w.src.join("bad.cpp").display().to_string();
    assert!(out.contains(&format!("  {bad}: pre-image killed, post-image killed")), "{out}");
}

// WP-171 item 5: `rename/accept-tmp` is removed after `--accept`, both when
// the check passes and when it fails (post-image rejection, crash, or spawn error).
#[test]
fn s6_5_accept_removes_accept_tmp() {
    let w = World::new("rename-accept-tmp-pass");
    w.records(&Run1::of(State::Verified));
    let (r, out) = w.run(&["--accept", "-j", "2"]);
    assert!(matches!(r, Ok(true)), "{r:?}\n{out}");
    assert!(!w.dir().join("rename/accept-tmp").exists(), "accept-tmp leaked on pass");

    let w = World::new("rename-accept-tmp-fail");
    w.records(&Run1::of(State::Verified));
    std::fs::write(w.f.path("fake/reject.after"), "b.cpp\n").unwrap();
    let (r, out) = w.run(&["--accept", "-j", "1"]);
    assert!(matches!(r, Ok(false)), "{r:?}\n{out}");
    assert!(!w.dir().join("rename/accept-tmp").exists(), "accept-tmp leaked on reject fail");

    let w = World::new("rename-accept-tmp-spawn-err");
    w.records(&Run1::of(State::Verified));
    let broken = w.f.path("bin/fakecxx");
    std::fs::write(&broken, "#!/nonexistent/wp171-interpreter\n").unwrap();
    std::fs::set_permissions(&broken, std::fs::Permissions::from_mode(0o755)).unwrap();
    let (r, out) = w.run(&["--accept", "-j", "1"]);
    assert!(matches!(r, Ok(false)), "{r:?}\n{out}");
    assert!(out.contains("check accept: FAIL (cannot run "), "{out}");
    assert!(!w.dir().join("rename/accept-tmp").exists(), "accept-tmp leaked on spawn error");
}

/// Whether any logged stand-in command line starts with `cmd`.
fn has_call(log: &str, cmd: &str) -> bool {
    log.lines().any(|l| l.starts_with(cmd))
}

// §6.5 / §9.5 (WP-166): census pre-flight check on every run that needs
// `verified` / `verified_with_residue` (including default with no `--expect`,
// `--expect verified`, `--expect verified_with_residue`, and `--accept`),
// bypassed by `--skip-census` (printed up front, recorded in result.json,
// appended to final line including on FAILED), census errors surfaced rather
// than treated as clean, and skipped on `--expect refused` / `not_verified` /
// `incomplete`.
#[test]
fn s6_5_census_gate() {
    // 1. `--expect verified` refused before rename's map phase on unclean census.
    let w = World::new("rename-census-verified");
    w.records(&Run1::of(State::Verified));
    w.seed_census(2);
    let (r, out) = w.run(&["--expect", "verified"]);
    assert_eq!(r, Ok(false), "{out}");
    assert!(
        out.contains("census: 2 indeterminate keys in 1 of 3 observers, 1 causes; 0 not ok"),
        "{out}"
    );
    assert!(out.contains("cargo xtask census tiny"), "{out}");
    assert!(!has_call(&w.log(), "rename "), "rename must not start: {}", w.log());

    // 2. `--expect verified_with_residue` refused on unclean census.
    let w = World::new("rename-census-residue");
    w.records(&Run1::of(State::VerifiedWithResidue));
    w.seed_census(2);
    let (r, out) = w.run(&["--expect", "verified_with_residue"]);
    assert_eq!(r, Ok(false), "{out}");
    assert!(out.contains("cargo xtask census tiny"), "{out}");
    assert!(!has_call(&w.log(), "rename "), "rename must not start: {}", w.log());

    // 3. Default (no `--expect`, no `--accept`) still requires verified /
    //    verified_with_residue and is refused on unclean census.
    let w = World::new("rename-census-default");
    w.records(&Run1::of(State::Verified));
    w.seed_census(2);
    let (r, out) = w.run(&[]);
    assert_eq!(r, Ok(false), "{out}");
    assert!(out.contains("cargo xtask census tiny"), "{out}");
    assert!(!has_call(&w.log(), "rename "), "rename must not start: {}", w.log());

    // 4. `--accept` (without `--expect`) also refused on unclean census.
    let w = World::new("rename-census-accept");
    w.records(&Run1::of(State::Verified));
    w.seed_census(2);
    let (r, out) = w.run(&["--accept"]);
    assert_eq!(r, Ok(false), "{out}");
    assert!(out.contains("cargo xtask census tiny"), "{out}");
    assert!(!has_call(&w.log(), "rename "), "rename must not start: {}", w.log());

    // 5. `--skip-census` prints `census: skipped (--skip-census)` up front,
    //    records `options.skip_census: true` in result.json, and notes it on
    //    the final `ok` line.
    let w = World::new("rename-census-skip");
    w.records(&Run1::of(State::Verified));
    w.seed_census(2);
    let (r, out) = w.run(&["--expect", "verified", "--skip-census"]);
    assert_eq!(r, Ok(true), "{out}");
    assert!(
        at(&out, "census: skipped (--skip-census)") < at(&out, "step rename: started"),
        "{out}"
    );
    assert!(out.ends_with("corpus rename: ok (census skipped)\n"), "{out}");
    assert_eq!(
        w.result_file().get("options").unwrap().get("skip_census"),
        Some(&Value::Bool(true))
    );
    assert!(!has_call(&w.log(), "parse "), "census must not run: {}", w.log());
    assert!(has_call(&w.log(), "rename "), "rename must run: {}", w.log());

    // 6. `--skip-census` on a failing check still prints the upfront line,
    //    records `skip_census: true` in `.new`, and keeps `(census skipped)`
    //    on the final `FAILED` line.
    let w = World::new("rename-census-skip-fail");
    w.records(&Run1::of(State::Refused));
    w.seed_census(2);
    let (r, out) = w.run(&["--expect", "verified", "--skip-census"]);
    assert_eq!(r, Ok(false), "{out}");
    assert!(
        at(&out, "census: skipped (--skip-census)") < at(&out, "step rename: started"),
        "{out}"
    );
    assert!(out.ends_with("corpus rename: FAILED (state) (census skipped)\n"), "{out}");
    let new_doc =
        json::parse(&std::fs::read(rename::new_path(&w.f.path(RESULT_FILE))).unwrap()).unwrap();
    assert_eq!(new_doc.get("options").unwrap().get("skip_census"), Some(&Value::Bool(true)));

    // 7. `--skip-census` when `slip rename` errors still prints the upfront line.
    let w = World::new("rename-census-skip-err");
    w.records(&Run1::of(State::Verified));
    std::fs::write(w.f.path("fake/rename.fail"), "").unwrap();
    let (r, out) = w.run(&["--expect", "verified", "--skip-census"]);
    assert!(matches!(r, Err(Failure::Error(_))), "{r:?}\n{out}");
    assert!(out.contains("census: skipped (--skip-census)\n"), "{out}");

    // 8. A census error inside `corpus rename` is surfaced as a command error
    //    (never treated as clean), rename does not start, and the binary exits 2.
    let w = World::new("rename-census-err");
    w.records(&Run1::of(State::Verified));
    std::fs::write(w.f.path("fake/census.fail"), "").unwrap();
    let (r, out) = w.run(&["--expect", "verified"]);
    assert!(matches!(&r, Err(Failure::Error(e)) if e.contains("census:")), "{r:?}\n{out}");
    assert!(!has_call(&w.log(), "rename "), "rename must not start on census error: {}", w.log());
    let bin_err = std::process::Command::new(env!("CARGO_BIN_EXE_xtask"))
        .args([
            "corpus",
            "rename",
            "tiny",
            "--from",
            "lib::f",
            "--to",
            "h",
            "--expect",
            "verified",
            "--slip",
            w.f.path("bin/slip").to_str().unwrap(),
        ])
        .env("CARGO_MANIFEST_DIR", w.f.path("repo/xtask"))
        .env("SLIP_CORPUS_DIR", w.f.path("corpora"))
        .output()
        .unwrap();
    assert_eq!(
        bin_err.status.code(),
        Some(2),
        "census error inside corpus rename must exit 2; stderr={}",
        String::from_utf8_lossy(&bin_err.stderr)
    );
    assert!(
        String::from_utf8_lossy(&bin_err.stderr).contains("xtask corpus rename: census:"),
        "stderr={}",
        String::from_utf8_lossy(&bin_err.stderr)
    );

    // 9. `--expect refused`, `--expect not_verified` (including with
    //    `--accept`), and `--expect incomplete` do not run the census.
    let w = World::new("rename-census-refused");
    w.records(&Run1::of(State::Refused));
    w.seed_census(2);
    let (r, out) = w.run(&["--expect", "refused"]);
    assert_eq!(r, Ok(true), "{out}");
    assert!(!has_call(&w.log(), "parse "), "refused must not run census: {}", w.log());

    let w = World::new("rename-census-not-verified");
    w.records(&Run1::of(State::NotVerified));
    w.seed_census(2);
    let (r, out) = w.run(&["--expect", "not_verified"]);
    assert_eq!(r, Ok(true), "{out}");
    assert!(!has_call(&w.log(), "parse "), "not_verified must not run census: {}", w.log());

    let w = World::new("rename-census-accept-not-verified");
    w.records(&Run1::of(State::NotVerified));
    w.seed_census(2);
    let (r, out) = w.run(&["--accept", "--expect", "not_verified"]);
    assert_eq!(r, Ok(true), "{out}");
    assert!(
        !has_call(&w.log(), "parse "),
        "--accept --expect not_verified must not run census: {}",
        w.log()
    );

    let w = World::new("rename-census-incomplete");
    w.records(&Run1::of(State::Incomplete));
    w.seed_census(2);
    let (r, out) = w.run(&["--expect", "incomplete"]);
    assert_eq!(r, Ok(true), "{out}");
    assert!(!has_call(&w.log(), "parse "), "incomplete must not run census: {}", w.log());
}

// §6.5 (WP-169 case a): all observers `ok` passes `coverage`, and a corpus
// whose not-ok set exactly equals its `expected_unparsed` list with matching
// outcomes also passes `coverage`.
#[test]
fn s6_5_coverage_ok() {
    let w = World::new("rename-cov-all-ok");
    w.records(&Run1::of(State::Verified));
    let (r, out) = w.run(&[]);
    assert!(matches!(r, Ok(true)), "{r:?}\n{out}");
    let c = checks(&out);
    assert_eq!(c[0], "check coverage: ok (3 of 3 observers ok)");
    assert!(out.ends_with("corpus rename: ok\n"), "{out}");

    let corpora =
        format!("{CORPORA}expected_unparsed = [\"diagnosed:bad.cpp:intentional syntax error\"]\n");
    let w = World::with_corpora("rename-cov-expected-ok", &corpora);
    let mut run = Run1::of(State::Verified);
    run.map_outcomes = vec![("bad.cpp", Status::Diagnosed, "bad.cpp:1:9: error: expected ')'")];
    w.records(&run);
    let (r, out) = w.run(&[]);
    assert!(matches!(r, Ok(true)), "{r:?}\n{out}");
    let c = checks(&out);
    assert_eq!(c[0], "check coverage: ok (2 of 3 observers ok (1 expected unparsed))");
    assert!(out.ends_with("corpus rename: ok\n"), "{out}");
}

// §6.5 (WP-169 case b): an observer not `ok` beyond the expected list fails
// `coverage`, prints the observer with outcome and first error line, leaves
// later checks `not run`, and exits as a failed check — including when the
// total not-ok count equals the expected count but the observer sets differ.
#[test]
fn s6_5_coverage_unexpected_not_ok() {
    let w = World::new("rename-cov-unexpected");
    let mut run = Run1::of(State::Verified);
    run.map_outcomes = vec![(
        "bad.cpp",
        Status::Diagnosed,
        "bad.cpp:1:9: error: expected ')'\nsecond diagnostic line",
    )];
    w.records(&run);
    let (r, out) = w.run(&["--accept"]);
    assert!(matches!(r, Ok(false)), "{r:?}\n{out}");
    assert!(out.contains("  bad.cpp: diagnosed: bad.cpp:1:9: error: expected ')'\n"), "{out}");
    assert!(!out.contains("second diagnostic line"), "{out}");
    let c = checks(&out);
    assert_eq!(c.len(), 6);
    assert_eq!(c[0], "check coverage: FAIL (1 observer not ok beyond the expected 0)");
    for (line, name) in c[1..].iter().zip(["state", "determinism", "accept", "build", "back"]) {
        assert_eq!(*line, format!("check {name}: not run (an earlier check failed)"));
    }
    assert!(out.ends_with("corpus rename: FAILED (coverage)\n"), "{out}");
    assert!(!w.log().contains("reduce"), "{}", w.log());
    assert!(!w.f.path(RESULT_FILE).exists());
    assert!(rename::new_path(&w.f.path(RESULT_FILE)).is_file());

    // Equal count, different set: `bad.cpp` is expected diagnosed but parsed ok,
    // while `a.cpp` failed with diagnosed (1 not-ok == 1 expected, different set).
    let corpora =
        format!("{CORPORA}expected_unparsed = [\"diagnosed:bad.cpp:intentional syntax error\"]\n");
    let w = World::with_corpora("rename-cov-set-not-count", &corpora);
    let mut run = Run1::of(State::Verified);
    run.map_outcomes = vec![("a.cpp", Status::Diagnosed, "a.cpp:1:1: error: boom")];
    w.records(&run);
    let (r, out) = w.run(&[]);
    assert!(matches!(r, Ok(false)), "{r:?}\n{out}");
    let c = checks(&out);
    assert_eq!(c[0], "check coverage: FAIL (1 observer not ok beyond the expected 1)");
    assert!(out.contains("  a.cpp: diagnosed: a.cpp:1:1: error: boom\n"), "{out}");
    assert!(out.contains("  bad.cpp: ok (expected diagnosed)\n"), "{out}");
    assert_eq!(c[1], "check state: not run (an earlier check failed)");
    assert!(out.ends_with("corpus rename: FAILED (coverage)\n"), "{out}");
}

// §6.5 (WP-169 case c): a listed expected-unparsed observer that parsed (`ok`)
// or ended with another outcome (`failed` instead of `diagnosed`) fails
// `coverage`, stops later checks, and writes no absolute path to result JSON.
#[test]
fn s6_5_coverage_expected_parsed_or_changed() {
    let corpora =
        format!("{CORPORA}expected_unparsed = [\"diagnosed:bad.cpp:intentional syntax error\"]\n");
    let w = World::with_corpora("rename-cov-expected-parsed", &corpora);
    w.records(&Run1::of(State::Verified));
    let (r, out) = w.run(&[]);
    assert!(matches!(r, Ok(false)), "{r:?}\n{out}");
    let c = checks(&out);
    assert_eq!(c[0], "check coverage: FAIL (expected unparsed observer bad.cpp is ok)");
    assert!(out.contains("  bad.cpp: ok (expected diagnosed)\n"), "{out}");
    for (line, name) in c[1..].iter().zip(["state", "determinism", "accept", "build", "back"]) {
        assert_eq!(*line, format!("check {name}: not run (an earlier check failed)"));
    }
    assert!(out.ends_with("corpus rename: FAILED (coverage)\n"), "{out}");

    // Same path, different non-ok outcome (`failed` instead of `diagnosed`).
    let w = World::with_corpora("rename-cov-expected-changed", &corpora);
    let mut run = Run1::of(State::Verified);
    run.map_outcomes = vec![("bad.cpp", Status::Failed, "worker exited 139")];
    w.records(&run);
    let (r, out) = w.run(&[]);
    assert!(matches!(r, Ok(false)), "{r:?}\n{out}");
    let c = checks(&out);
    assert_eq!(c[0], "check coverage: FAIL (expected unparsed observer bad.cpp is failed)");
    assert!(out.contains("  bad.cpp: failed (expected diagnosed): worker exited 139\n"), "{out}");
    assert_eq!(c[1], "check state: not run (an earlier check failed)");
    assert!(out.ends_with("corpus rename: FAILED (coverage)\n"), "{out}");
}

// §6.5 (WP-169 item 2): `coverage` prints at most 10 unexpected observers.
#[test]
fn s6_5_coverage_lists_at_most_ten() {
    let f = Fixture::empty("rename-cov-cap10");
    let src = f.path("src");
    std::fs::create_dir_all(&src).unwrap();
    let store = f.path("store");
    let st = Store::open(&store, Access::Exclusive).unwrap();
    let observers: Vec<Observer> = (0..12u8)
        .map(|i| {
            let main = src.join(format!("u{i:02}.cpp")).display().to_string().into_bytes();
            let command = Cmd {
                directory: src.display().to_string().into_bytes(),
                argv: vec![b"clang++".to_vec(), main.clone()],
                env: vec![],
            };
            Observer {
                id: command.observer_id(),
                command,
                language: Language::Cxx,
                main: FileRef { path: main, size: 1, content: d(i) },
                cost_hint: 100 - u64::from(i),
                origins: vec![Origin { configuration: 0, entry: u64::from(i) }],
            }
        })
        .collect();
    let plan_obj = Plan {
        tool_slip: "0.0.0".into(),
        tool_kiln: "1.3.0".into(),
        configurations: vec![Configuration {
            name: "default".into(),
            database: FileRef { path: b"/db".to_vec(), size: 2, content: d(9) },
        }],
        observers: observers.clone(),
        excluded: vec![],
    };
    let plan = st.publish_plan(&plan_obj.to_bytes()).unwrap();
    let mut jobs = Vec::new();
    for (i, o) in observers.iter().enumerate() {
        let sh = Shard {
            observer: o.id,
            phase: Phase::Map,
            round: 1,
            outcome: ShardOutcome {
                status: Status::Failed,
                errors: 1,
                warnings: 0,
                message: format!("cannot enter /missing/{i:02}: No such file or directory"),
            },
            inputs: None,
            counters: Vec::new(),
            digests: None,
            verdict: None,
            edits: None,
            binding: None,
            rule_use: None,
            spellings: None,
            names: None,
            negatives: None,
            found: None,
            rebuilt_use: None,
        };
        let bytes = sh.to_bytes();
        let sid = id(&bytes);
        st.publish_shard(&plan, Phase::Map, 1, &o.id, &bytes).unwrap();
        jobs.push(Job {
            observer: o.id,
            attempt: 1,
            end: End::Completed,
            shard: Some(sid),
            queued_ns: 0,
            wall_ns: 1,
            cpu_ns: 1,
            peak_rss: 1,
            estimate: 1,
        });
    }
    let rec = RunRecord {
        run: Run {
            tool_slip: "0.0.0".into(),
            tool_kiln: "1.3.0".into(),
            plan,
            phase: Phase::Map,
            round: 1,
            started_unix_ns: STARTED,
            wall_ns: 1,
            cores: 1,
            workers_max: 1,
            memory_budget: 0,
            peak_rss_sum: 1,
        },
        jobs,
    };
    let run_path = st.publish_run(&plan, Phase::Map, 1, STARTED, &rec.to_bytes()).unwrap();
    drop(st);
    let loaded = xtask::perf::load(&run_path).unwrap();
    let (outcome, lines) = rename::check_coverage(&loaded, &src, &[], &[]);
    assert_eq!(outcome, rename::Outcome::Fail("12 observers not ok beyond the expected 0".into()));
    assert_eq!(lines.len(), 11);
    assert!(lines[0].contains("u00.cpp: failed: cannot enter /missing/00"), "{}", lines[0]);
    assert_eq!(lines[10], "  (+2 more)");
}

// §6.5 (WP-169 case d): before `slip` runs, every compilation database entry's
// `directory` must exist and be a directory, and its `file` (relative or
// absolute) must exist; otherwise stop with exit status 2 before `slip` runs,
// naming the count and the first 10, with no store written.
#[test]
fn s6_5_preflight_database_entries() {
    let w = World::new("rename-db-missing-dir");
    let missing_dir = w.dir().join("no-such-build-dir");
    let db = Value::Array(vec![Value::Object(vec![
        ("directory".to_owned(), Value::Str(missing_dir.display().to_string())),
        ("file".to_owned(), Value::Str(w.src.join("a.cpp").display().to_string())),
        ("arguments".to_owned(), Value::Array(vec![Value::Str("clang++".to_owned())])),
    ])]);
    std::fs::write(w.dir().join("build/compile_commands.json"), json::write_pretty(&db).unwrap())
        .unwrap();
    let (r, _) = w.run(&[]);
    let Err(Failure::Error(msg)) = r else { panic!("expected Failure::Error, got {r:?}") };
    assert!(
        msg.starts_with(&format!("1 database entry cannot run: {}:", missing_dir.display())),
        "{msg}"
    );
    assert!(w.log().is_empty(), "slip must not run: {}", w.log());
    assert!(!w.store().exists(), "no store may be written");
    assert!(!w.dir().join("rename").exists(), "no rename/ dir may be written");

    // Verify the `xtask` CLI exits with status 2 (not 1) and writes no store.
    let cli = std::process::Command::new(env!("CARGO_BIN_EXE_xtask"))
        .current_dir(w.f.path("repo"))
        .args([
            "corpus",
            "rename",
            "tiny",
            "--from",
            "lib::f",
            "--to",
            "h",
            "--slip",
            &w.f.path("bin/slip").display().to_string(),
        ])
        .env("CARGO_MANIFEST_DIR", w.f.path("repo/xtask"))
        .env("SLIP_CORPUS_DIR", w.f.path("corpora"))
        .output()
        .unwrap();
    assert_eq!(cli.status.code(), Some(2), "stderr: {}", String::from_utf8_lossy(&cli.stderr));
    let stderr = String::from_utf8_lossy(&cli.stderr);
    assert!(
        stderr.contains(&format!(
            "corpus rename: 1 database entry cannot run: {}:",
            missing_dir.display()
        )),
        "{stderr}"
    );
    assert!(w.log().is_empty(), "slip must not run: {}", w.log());
    assert!(!w.store().exists(), "no store may be written");

    // Directory is a regular file, missing relative file, missing absolute file,
    // and >10 broken entries (capped at first 10).
    let w = World::new("rename-db-various");
    let not_a_dir = w.src.join("a.cpp");
    let missing_abs = w.src.join("missing_abs.cpp");
    let mut entries = vec![
        Value::Object(vec![
            ("directory".to_owned(), Value::Str(not_a_dir.display().to_string())),
            ("file".to_owned(), Value::Str("a.cpp".to_owned())),
            ("arguments".to_owned(), Value::Array(vec![Value::Str("clang++".to_owned())])),
        ]),
        Value::Object(vec![
            ("directory".to_owned(), Value::Str(w.src.display().to_string())),
            ("file".to_owned(), Value::Str("sub/missing_rel.cpp".to_owned())),
            ("arguments".to_owned(), Value::Array(vec![Value::Str("clang++".to_owned())])),
        ]),
        Value::Object(vec![
            ("directory".to_owned(), Value::Str(w.src.display().to_string())),
            ("file".to_owned(), Value::Str(missing_abs.display().to_string())),
            ("arguments".to_owned(), Value::Array(vec![Value::Str("clang++".to_owned())])),
        ]),
    ];
    for i in 0..9 {
        entries.push(Value::Object(vec![
            (
                "directory".to_owned(),
                Value::Str(w.dir().join(format!("missing_{i:02}")).display().to_string()),
            ),
            ("file".to_owned(), Value::Str("x.cpp".to_owned())),
            ("arguments".to_owned(), Value::Array(vec![Value::Str("clang++".to_owned())])),
        ]));
    }
    let db = Value::Array(entries);
    std::fs::write(w.dir().join("build/compile_commands.json"), json::write_pretty(&db).unwrap())
        .unwrap();
    let (r, _) = w.run(&[]);
    let Err(Failure::Error(msg)) = r else { panic!("expected Failure::Error, got {r:?}") };
    assert!(msg.starts_with("12 database entries cannot run:"), "{msg}");
    assert!(msg.contains("not a directory"), "{msg}");
    assert!(msg.contains("sub/missing_rel.cpp"), "{msg}");
    assert!(msg.contains("missing_abs.cpp"), "{msg}");
    assert_eq!(msg.lines().count(), 10, "{msg}");
    assert!(msg.contains("missing_06"), "{msg}");
    assert!(!msg.contains("missing_07"), "{msg}");
    assert!(w.log().is_empty());
    assert!(!w.store().exists());
}

// §6.5 (WP-169 + WP-166 ordering): `check_database` MUST run before
// `check_census`. With BOTH a broken database AND a census that would fail
// (unclean census or failing stand-in `slip parse`), the database error wins
// with exit status 2 (`Failure::Error`), zero `slip` calls (`plan`/`parse`
// never invoked), and no store written.
#[test]
fn s6_5_preflight_database_before_census() {
    // Subcase 1: broken database + unclean census -> database error wins (exit 2),
    // zero census calls, no census store written.
    let w = World::new("rename-db-before-unclean");
    w.seed_census(2);
    let missing_dir = w.dir().join("missing-build-dir-1");
    let db = Value::Array(vec![Value::Object(vec![
        ("directory".to_owned(), Value::Str(missing_dir.display().to_string())),
        ("file".to_owned(), Value::Str(w.src.join("a.cpp").display().to_string())),
        ("arguments".to_owned(), Value::Array(vec![Value::Str("clang++".to_owned())])),
    ])]);
    std::fs::write(w.dir().join("build/compile_commands.json"), json::write_pretty(&db).unwrap())
        .unwrap();
    let (r, out) = w.run(&["--expect", "verified"]);
    let Err(Failure::Error(msg)) = r else {
        panic!("expected database Failure::Error before unclean census, got {r:?}\n{out}");
    };
    assert!(
        msg.starts_with(&format!("1 database entry cannot run: {}:", missing_dir.display())),
        "{msg}"
    );
    assert!(!out.contains("census:"), "census must not run: {out}");
    assert!(w.log().is_empty(), "zero slip calls expected before database check: {}", w.log());
    assert!(!w.dir().join("stores/census").exists(), "no census store may be written");
    assert!(!w.store().exists(), "no rename store may be written");

    let cli = std::process::Command::new(env!("CARGO_BIN_EXE_xtask"))
        .current_dir(w.f.path("repo"))
        .args([
            "corpus",
            "rename",
            "tiny",
            "--from",
            "lib::f",
            "--to",
            "h",
            "--expect",
            "verified",
            "--slip",
            &w.f.path("bin/slip").display().to_string(),
        ])
        .env("CARGO_MANIFEST_DIR", w.f.path("repo/xtask"))
        .env("SLIP_CORPUS_DIR", w.f.path("corpora"))
        .output()
        .unwrap();
    assert_eq!(cli.status.code(), Some(2), "stderr: {}", String::from_utf8_lossy(&cli.stderr));
    let stderr = String::from_utf8_lossy(&cli.stderr);
    assert!(stderr.contains("1 database entry cannot run:"), "{stderr}");
    assert!(!stderr.contains("census:"), "{stderr}");
    assert!(
        !String::from_utf8_lossy(&cli.stdout).contains("census:"),
        "stdout: {}",
        String::from_utf8_lossy(&cli.stdout)
    );
    assert!(w.log().is_empty(), "zero slip calls expected: {}", w.log());

    // Subcase 2: broken database + failing census stand-in (`fake/census.fail`) ->
    // database error wins (exit 2), NOT census error, with zero `slip` calls.
    let w = World::new("rename-db-before-failing");
    w.seed_census(0);
    std::fs::write(w.f.path("fake/census.fail"), "").unwrap();
    let missing_dir = w.dir().join("missing-build-dir-2");
    let db = Value::Array(vec![Value::Object(vec![
        ("directory".to_owned(), Value::Str(missing_dir.display().to_string())),
        ("file".to_owned(), Value::Str(w.src.join("a.cpp").display().to_string())),
        ("arguments".to_owned(), Value::Array(vec![Value::Str("clang++".to_owned())])),
    ])]);
    std::fs::write(w.dir().join("build/compile_commands.json"), json::write_pretty(&db).unwrap())
        .unwrap();
    let (r, out) = w.run(&["--expect", "verified"]);
    let Err(Failure::Error(msg)) = r else {
        panic!("expected database Failure::Error before failing census, got {r:?}\n{out}");
    };
    assert!(
        msg.starts_with(&format!("1 database entry cannot run: {}:", missing_dir.display())),
        "{msg}"
    );
    assert!(!msg.contains("census:"), "database error must win over census error: {msg}");
    assert!(w.log().is_empty(), "zero slip calls expected: {}", w.log());
    assert!(!w.dir().join("stores/census").exists(), "no census store may be written");
    assert!(!w.store().exists(), "no rename store may be written");

    let cli = std::process::Command::new(env!("CARGO_BIN_EXE_xtask"))
        .current_dir(w.f.path("repo"))
        .args([
            "corpus",
            "rename",
            "tiny",
            "--from",
            "lib::f",
            "--to",
            "h",
            "--expect",
            "verified",
            "--slip",
            &w.f.path("bin/slip").display().to_string(),
        ])
        .env("CARGO_MANIFEST_DIR", w.f.path("repo/xtask"))
        .env("SLIP_CORPUS_DIR", w.f.path("corpora"))
        .output()
        .unwrap();
    assert_eq!(cli.status.code(), Some(2), "stderr: {}", String::from_utf8_lossy(&cli.stderr));
    let stderr = String::from_utf8_lossy(&cli.stderr);
    assert!(stderr.contains("1 database entry cannot run:"), "{stderr}");
    assert!(!stderr.contains("census:"), "{stderr}");
    assert!(w.log().is_empty(), "zero slip calls expected: {}", w.log());
}

// Helper for hand-built store unit tests of `check_coverage`.
fn one_observer(src: &Path, name: &str, salt: u8) -> Observer {
    let main = src.join(name).display().to_string().into_bytes();
    let command = Cmd {
        directory: src.display().to_string().into_bytes(),
        argv: vec![b"clang++".to_vec(), main.clone()],
        env: vec![],
    };
    Observer {
        id: command.observer_id(),
        command,
        language: Language::Cxx,
        main: FileRef { path: main, size: 1, content: d(salt) },
        cost_hint: 10,
        origins: vec![Origin { configuration: 0, entry: u64::from(salt) }],
    }
}

fn ok_shard(observer: Digest, warnings: u64) -> Shard {
    Shard {
        observer,
        phase: Phase::Map,
        round: 1,
        outcome: ShardOutcome { status: Status::Ok, errors: 0, warnings, message: String::new() },
        inputs: Some(Vec::new()),
        counters: Vec::new(),
        digests: None,
        verdict: None,
        edits: None,
        binding: None,
        rule_use: None,
        spellings: None,
        names: None,
        negatives: None,
        found: None,
        rebuilt_use: None,
    }
}

fn hand_plan(observers: Vec<Observer>, excluded: Vec<Excluded>) -> Plan {
    Plan {
        tool_slip: "0.0.0".into(),
        tool_kiln: "1.3.0".into(),
        configurations: vec![Configuration {
            name: "default".into(),
            database: FileRef { path: b"/db".to_vec(), size: 2, content: d(9) },
        }],
        observers,
        excluded,
    }
}

fn hand_run(plan: Digest, jobs: Vec<Job>) -> RunRecord {
    RunRecord {
        run: Run {
            tool_slip: "0.0.0".into(),
            tool_kiln: "1.3.0".into(),
            plan,
            phase: Phase::Map,
            round: 1,
            started_unix_ns: STARTED,
            wall_ns: 1,
            cores: 1,
            workers_max: 1,
            memory_budget: 0,
            peak_rss_sum: 1,
        },
        jobs,
    }
}

// §6.5 (WP-169 fix round 1, item 1): `coverage` enforces `plan.excluded`
// against `expected_excluded` (exact set and reason match), caps printed lines
// at 10 with `  (+n more)`, and gives excluded failures precedence over
// observer failures.
#[test]
fn s6_5_coverage_excluded() {
    // 1. Non-empty `plan.excluded` with empty `expected_excluded` fails `coverage`.
    let w = World::new("rename-cov-excl-fail");
    let mut run = Run1::of(State::Verified);
    run.excluded = vec![Excluded {
        configuration: 0,
        entry: 3,
        file: w.src.join("extra.cpp").display().to_string().into_bytes(),
        reason: PlanReason::UnsupportedFlag,
        detail: "-fweird".into(),
    }];
    w.records(&run);
    let (r, out) = w.run(&[]);
    assert!(matches!(r, Ok(false)), "{r:?}\n{out}");
    let c = checks(&out);
    assert_eq!(c[0], "check coverage: FAIL (1 database entry excluded beyond the expected 0)");
    assert!(out.contains("  excluded extra.cpp: unsupported_flag: -fweird\n"), "{out}");
    assert_eq!(c[1], "check state: not run (an earlier check failed)");

    // 2. Matching `expected_excluded` passes `coverage` (both alone and with `expected_unparsed`).
    let corpora = format!(
        "{CORPORA}expected_excluded = [\"unsupported_flag:extra.cpp:uses -fweird\"]\n\
         expected_unparsed = [\"diagnosed:bad.cpp:intentional syntax error\"]\n"
    );
    let w = World::with_corpora("rename-cov-excl-ok", &corpora);
    let mut run = Run1::of(State::Verified);
    run.map_outcomes = vec![("bad.cpp", Status::Diagnosed, "bad.cpp:1:9: error: expected ')'")];
    run.excluded = vec![Excluded {
        configuration: 0,
        entry: 3,
        file: w.src.join("extra.cpp").display().to_string().into_bytes(),
        reason: PlanReason::UnsupportedFlag,
        detail: "-fweird".into(),
    }];
    w.records(&run);
    let (r, out) = w.run(&[]);
    assert!(matches!(r, Ok(true)), "{r:?}\n{out}");
    assert_eq!(
        checks(&out)[0],
        "check coverage: ok (2 of 3 observers ok (1 expected unparsed, 1 expected excluded))"
    );

    // 3. Listed `expected_excluded` entry that was NOT excluded fails `coverage`.
    let corpora =
        format!("{CORPORA}expected_excluded = [\"unsupported_flag:extra.cpp:uses -fweird\"]\n");
    let w = World::with_corpora("rename-cov-excl-missing", &corpora);
    w.records(&Run1::of(State::Verified));
    let (r, out) = w.run(&[]);
    assert!(matches!(r, Ok(false)), "{r:?}\n{out}");
    assert_eq!(
        checks(&out)[0],
        "check coverage: FAIL (expected excluded entry extra.cpp is not excluded)"
    );
    assert!(
        out.contains("  excluded extra.cpp: not excluded (expected unsupported_flag)\n"),
        "{out}"
    );

    // 4. Listed `expected_excluded` entry excluded with a different reason fails `coverage`.
    let w = World::with_corpora("rename-cov-excl-wrong-reason", &corpora);
    let mut run = Run1::of(State::Verified);
    run.excluded = vec![Excluded {
        configuration: 0,
        entry: 3,
        file: b"extra.cpp".to_vec(),
        reason: PlanReason::MalformedEntry,
        detail: "missing arguments".into(),
    }];
    w.records(&run);
    let (r, out) = w.run(&[]);
    assert!(matches!(r, Ok(false)), "{r:?}\n{out}");
    assert_eq!(
        checks(&out)[0],
        "check coverage: FAIL (expected excluded entry extra.cpp is malformed_entry)"
    );
    assert!(
        out.contains(
            "  excluded extra.cpp: malformed_entry (expected unsupported_flag): missing arguments\n"
        ),
        "{out}"
    );

    // 5. >10 unexpected excluded entries capped at 10 with `  (+2 more)`, and
    // precedence when both excluded and unparsed fail.
    let f = Fixture::empty("rename-cov-excl-cap10");
    let src = f.path("src");
    std::fs::create_dir_all(&src).unwrap();
    let st = Store::open(&f.path("store"), Access::Exclusive).unwrap();
    let o = one_observer(&src, "a.cpp", 1);
    let excluded: Vec<Excluded> = (0..12u64)
        .map(|i| Excluded {
            configuration: 0,
            entry: i + 10,
            file: src.join(format!("e{i:02}.cpp")).display().to_string().into_bytes(),
            reason: PlanReason::UnsupportedFlag,
            detail: format!("-fbad-{i:02}"),
        })
        .collect();
    let plan = st.publish_plan(&hand_plan(vec![o.clone()], excluded).to_bytes()).unwrap();
    // Leave `a.cpp` with no job as well to test precedence (excluded wins detail).
    let run_path =
        st.publish_run(&plan, Phase::Map, 1, STARTED, &hand_run(plan, vec![]).to_bytes()).unwrap();
    drop(st);
    let loaded = xtask::perf::load(&run_path).unwrap();
    let (outcome, lines) = rename::check_coverage(&loaded, &src, &[], &[]);
    assert_eq!(
        outcome,
        rename::Outcome::Fail("12 database entries excluded beyond the expected 0".into())
    );
    // 10 entries listed + trailing `(+3 more)` (12 excluded + 1 observer no-job = 13 total).
    assert_eq!(lines.len(), 11, "{lines:?}");
    assert_eq!(lines[0], "  excluded e00.cpp: unsupported_flag: -fbad-00");
    assert_eq!(lines[9], "  excluded e09.cpp: unsupported_flag: -fbad-09");
    assert_eq!(lines[10], "  (+3 more)");

    // Also test >10 missing `expected_excluded` entries (both `detail` and `lines` `(+n more)`).
    let f = Fixture::empty("rename-cov-excl-missing-cap10");
    let src = f.path("src");
    std::fs::create_dir_all(&src).unwrap();
    let st = Store::open(&f.path("store"), Access::Exclusive).unwrap();
    let o = one_observer(&src, "a.cpp", 1);
    let plan = st.publish_plan(&hand_plan(vec![o.clone()], vec![]).to_bytes()).unwrap();
    let sh_bytes = ok_shard(o.id, 0).to_bytes();
    let sid = id(&sh_bytes);
    st.publish_shard(&plan, Phase::Map, 1, &o.id, &sh_bytes).unwrap();
    let job = Job {
        observer: o.id,
        attempt: 1,
        end: End::Completed,
        shard: Some(sid),
        queued_ns: 0,
        wall_ns: 1,
        cpu_ns: 1,
        peak_rss: 1,
        estimate: 1,
    };
    let run_path = st
        .publish_run(&plan, Phase::Map, 1, STARTED, &hand_run(plan, vec![job]).to_bytes())
        .unwrap();
    drop(st);
    let loaded = xtask::perf::load(&run_path).unwrap();
    let expected: Vec<ExpectedExcluded> = (0..12)
        .map(|i| ExpectedExcluded {
            path: format!("m{i:02}.cpp"),
            reason: PlanReason::NotACompile,
            note: "link step".into(),
        })
        .collect();
    let (outcome, lines) = rename::check_coverage(&loaded, &src, &[], &expected);
    assert_eq!(
        outcome,
        rename::Outcome::Fail("expected excluded entry m00.cpp is not excluded (+11 more)".into())
    );
    assert_eq!(lines.len(), 11, "{lines:?}");
    assert_eq!(lines[0], "  excluded m00.cpp: not excluded (expected not_a_compile)");
    assert_eq!(lines[10], "  (+2 more)");
}

// §6.5 (WP-169 fix round 1, item 2): hand-built store cases covering every
// `check_coverage` failure path: (a) observer with no job, (b) missing shard,
// (c) shard from another run, (d) job with no shard, (e) no map run record,
// and (f) listed expected path absent from plan / not excluded; plus singular
// `1 of 1 observer ok`.
#[test]
fn s6_5_coverage_store_failure_paths() {
    // Baseline: 1 observer ok uses singular `1 of 1 observer ok`.
    let f = Fixture::empty("rename-cov-paths");
    let src = f.path("src");
    std::fs::create_dir_all(&src).unwrap();
    let o = one_observer(&src, "a.cpp", 1);
    let make_store = |sub: &str, jobs: Vec<Job>, shard: Option<&[u8]>| {
        let st = Store::open(&f.path(sub), Access::Exclusive).unwrap();
        let plan = st.publish_plan(&hand_plan(vec![o.clone()], vec![]).to_bytes()).unwrap();
        if let Some(bytes) = shard {
            st.publish_shard(&plan, Phase::Map, 1, &o.id, bytes).unwrap();
        }
        let run_path = st
            .publish_run(&plan, Phase::Map, 1, STARTED, &hand_run(plan, jobs).to_bytes())
            .unwrap();
        drop(st);
        xtask::perf::load(&run_path).unwrap()
    };

    let sh_ok = ok_shard(o.id, 0).to_bytes();
    let sh_ok_id = id(&sh_ok);
    let job_with = |end, shard| Job {
        observer: o.id,
        attempt: 1,
        end,
        shard,
        queued_ns: 0,
        wall_ns: 1,
        cpu_ns: 1,
        peak_rss: 1,
        estimate: 1,
    };

    // Singular ok message on a 1-observer plan.
    let loaded = make_store("s-ok", vec![job_with(End::Completed, Some(sh_ok_id))], Some(&sh_ok));
    let (outcome, lines) = rename::check_coverage(&loaded, &src, &[], &[]);
    assert_eq!(outcome, rename::Outcome::Ok("1 of 1 observer ok".into()));
    assert!(lines.is_empty());

    // (a) Observer with no job (`Side::NoJob`).
    let loaded = make_store("s-no-job", vec![], None);
    let (outcome, lines) = rename::check_coverage(&loaded, &src, &[], &[]);
    assert_eq!(outcome, rename::Outcome::Fail("1 observer not ok beyond the expected 0".into()));
    assert_eq!(lines, ["  a.cpp: no job"]);

    // (b) Missing shard (`Side::Absent`): job names a shard id, store has no shard file.
    let loaded = make_store("s-absent", vec![job_with(End::Completed, Some(sh_ok_id))], None);
    let (outcome, lines) = rename::check_coverage(&loaded, &src, &[], &[]);
    assert_eq!(outcome, rename::Outcome::Fail("1 observer not ok beyond the expected 0".into()));
    assert_eq!(lines, ["  a.cpp: missing shard"]);

    // (c) Shard from another run (`Side::Other`): store holds an `ok` shard,
    // but the job names a different shard content id.
    let other_sh = ok_shard(o.id, 1).to_bytes();
    assert_ne!(id(&other_sh), sh_ok_id);
    let loaded =
        make_store("s-other", vec![job_with(End::Completed, Some(sh_ok_id))], Some(&other_sh));
    let (outcome, lines) = rename::check_coverage(&loaded, &src, &[], &[]);
    assert_eq!(outcome, rename::Outcome::Fail("1 observer not ok beyond the expected 0".into()));
    assert_eq!(lines, ["  a.cpp: other shard"]);

    // (d) Job with no shard (`Side::NoShard`): even if an `ok` shard exists in the store,
    // the job's last attempt ended crashed with `shard: None`.
    let loaded = make_store("s-no-shard", vec![job_with(End::Crashed, None)], Some(&sh_ok));
    let (outcome, lines) = rename::check_coverage(&loaded, &src, &[], &[]);
    assert_eq!(outcome, rename::Outcome::Fail("1 observer not ok beyond the expected 0".into()));
    assert_eq!(lines, ["  a.cpp: no shard (crashed)"]);

    // (e) No map run record in `TransformRun`.
    let w = World::new("rename-cov-no-map-run");
    let mut run = Run1::of(State::Verified);
    run.no_map_run = true;
    w.records(&run);
    let (r, out) = w.run(&[]);
    assert!(matches!(r, Ok(false)), "{r:?}\n{out}");
    let c = checks(&out);
    assert_eq!(c[0], "check coverage: FAIL (the transform run record names no map run record)");
    assert_eq!(c[1], "check state: not run (an earlier check failed)");

    // (f) Listed `expected_unparsed` path not in plan (and `expected_excluded` not excluded).
    let loaded =
        make_store("s-absent-listed", vec![job_with(End::Completed, Some(sh_ok_id))], Some(&sh_ok));
    let listed_unparsed = vec![ExpectedUnparsed {
        path: "ghost.cpp".into(),
        status: Status::Diagnosed,
        reason: "not in plan".into(),
    }];
    let (outcome, lines) = rename::check_coverage(&loaded, &src, &listed_unparsed, &[]);
    assert_eq!(
        outcome,
        rename::Outcome::Fail("expected unparsed observer ghost.cpp is absent from plan".into())
    );
    assert_eq!(lines, ["  ghost.cpp: absent from plan (expected diagnosed)"]);

    let listed_excluded = vec![ExpectedExcluded {
        path: "ghost.cpp".into(),
        reason: PlanReason::UnsupportedFlag,
        note: "not in plan".into(),
    }];
    let (outcome, lines) = rename::check_coverage(&loaded, &src, &[], &listed_excluded);
    assert_eq!(
        outcome,
        rename::Outcome::Fail("expected excluded entry ghost.cpp is not excluded".into())
    );
    assert_eq!(lines, ["  excluded ghost.cpp: not excluded (expected unsupported_flag)"]);
}

// §6.5 (WP-169 fix round 1, item 3): pre-flight `check_database` edge cases:
// (a) relative `file` that exists passes (resolved against `directory`),
// (b) `file` that is a directory fails,
// (c) relative `directory` fails.
#[test]
fn s6_5_preflight_database_edge_cases() {
    let w = World::new("rename-db-edges");
    let db_path = w.dir().join("build/compile_commands.json");

    // (a) Relative `file` that exists under `directory` MUST PASS.
    let db = Value::Array(vec![
        Value::Object(vec![
            ("directory".to_owned(), Value::Str(w.src.display().to_string())),
            ("file".to_owned(), Value::Str("a.cpp".to_owned())),
            ("arguments".to_owned(), Value::Array(vec![Value::Str("clang++".to_owned())])),
        ]),
        Value::Object(vec![
            ("directory".to_owned(), Value::Str(w.src.display().to_string())),
            ("file".to_owned(), Value::Str("sub/c.h".to_owned())),
            ("arguments".to_owned(), Value::Array(vec![Value::Str("clang++".to_owned())])),
        ]),
    ]);
    std::fs::write(&db_path, json::write_pretty(&db).unwrap()).unwrap();
    rename::check_database(&db_path).unwrap();
    w.records(&Run1::of(State::Verified));
    let (r, out) = w.run(&[]);
    assert!(matches!(r, Ok(true)), "{r:?}\n{out}");

    // (b) `file` that is a directory (both absolute and relative) MUST FAIL.
    let sub_dir = w.src.join("sub");
    let db = Value::Array(vec![Value::Object(vec![
        ("directory".to_owned(), Value::Str(w.src.display().to_string())),
        ("file".to_owned(), Value::Str(sub_dir.display().to_string())),
        ("arguments".to_owned(), Value::Array(vec![Value::Str("clang++".to_owned())])),
    ])]);
    std::fs::write(&db_path, json::write_pretty(&db).unwrap()).unwrap();
    let err = rename::check_database(&db_path).unwrap_err();
    assert_eq!(
        err,
        format!("1 database entry cannot run: {}: not a regular file", sub_dir.display())
    );

    let db = Value::Array(vec![Value::Object(vec![
        ("directory".to_owned(), Value::Str(w.src.display().to_string())),
        ("file".to_owned(), Value::Str("sub".to_owned())),
        ("arguments".to_owned(), Value::Array(vec![Value::Str("clang++".to_owned())])),
    ])]);
    std::fs::write(&db_path, json::write_pretty(&db).unwrap()).unwrap();
    let err = rename::check_database(&db_path).unwrap_err();
    assert_eq!(
        err,
        format!("1 database entry cannot run: {}: not a regular file", sub_dir.display())
    );

    // (c) Relative `directory` MUST FAIL even when `.` exists.
    let db = Value::Array(vec![Value::Object(vec![
        ("directory".to_owned(), Value::Str(".".to_owned())),
        ("file".to_owned(), Value::Str("a.cpp".to_owned())),
        ("arguments".to_owned(), Value::Array(vec![Value::Str("clang++".to_owned())])),
    ])]);
    std::fs::write(&db_path, json::write_pretty(&db).unwrap()).unwrap();
    let err = rename::check_database(&db_path).unwrap_err();
    assert_eq!(err, "1 database entry cannot run: .: not an absolute path");

    // Empty `directory` reports `entry 0: empty \`directory\``.
    let db = Value::Array(vec![Value::Object(vec![
        ("directory".to_owned(), Value::Str(String::new())),
        ("file".to_owned(), Value::Str("a.cpp".to_owned())),
    ])]);
    std::fs::write(&db_path, json::write_pretty(&db).unwrap()).unwrap();
    let err = rename::check_database(&db_path).unwrap_err();
    assert_eq!(err, "1 database entry cannot run: entry 0: empty `directory`");
}

// §6.5 (WP-169 fix round 1, item 6): `back` runs `check_database` on
// `rename-build/compile_commands.json` before invoking the inverse `slip rename`.
#[test]
fn s6_5_back_checks_database() {
    if !tools_present() {
        return;
    }
    let w = World::new("rename-back-bad-db");
    w.records(&Run1::of(State::Verified));
    let missing_dir = w.dir().join("missing-back-dir");
    let bad_db = Value::Array(vec![Value::Object(vec![
        ("directory".to_owned(), Value::Str(missing_dir.display().to_string())),
        ("file".to_owned(), Value::Str("a.cpp".to_owned())),
    ])]);
    std::fs::write(w.f.path("fake/back.bad_db"), json::write_pretty(&bad_db).unwrap()).unwrap();
    let (r, out) = w.run(&["--build"]);
    assert!(matches!(r, Ok(false)), "{r:?}\n{out}");
    assert!(out.contains("check build: ok"), "{out}");
    assert!(
        out.contains(&format!(
            "check back: FAIL (database: 1 database entry cannot run: {}:",
            missing_dir.display()
        )),
        "{out}"
    );
    assert!(!w.log().contains("--write"), "slip rename --write must not run: {}", w.log());
}
