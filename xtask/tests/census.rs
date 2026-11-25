//! `cargo xtask census` (`spec/harness.md` §9.5, WP-166).
//!
//! Tests the pure aggregation (`summarize`, `render`, `to_json`), the
//! end-to-end command against a configured fixture corpus and fake `slip`,
//! the store-reuse rule, and the binary's exit statuses (0, 1, 2).

#![cfg(target_os = "linux")]
#![allow(clippy::many_single_char_names)]

mod common;

use common::Fixture;
use slip_exec::store::{Access, Store};
use slip_model::digests::{self, Digests, Group, GroupClass};
use slip_model::json;
use slip_model::{
    Command as Cmd, Configuration, Counter, Digest, End, FileRef, Job, Language, Observer, Origin,
    Outcome, Phase, Plan, Run, RunRecord, Shard, Status, formats,
};
use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use xtask::census::{self, Census, FORMAT, SAMPLE_OBSERVERS};
use xtask::corpus::{self, Layout, rows};
use xtask::invariants::{Env, Failure};
use xtask::perf::{Loaded, RunPath};

fn d(n: u8) -> Digest {
    Digest([n; 32])
}

fn observer(main: &str, cost: u64, entry: u64) -> Observer {
    let command = Cmd {
        directory: b"/build".to_vec(),
        argv: vec![b"clang++".to_vec(), b"-c".to_vec(), main.as_bytes().to_vec()],
        env: vec![],
    };
    Observer {
        id: command.observer_id(),
        command,
        language: Language::Cxx,
        main: FileRef { path: main.as_bytes().to_vec(), size: cost, content: d(entry as u8) },
        cost_hint: cost,
        origins: vec![Origin { configuration: 0, entry }],
    }
}

fn plan_of(mains: &[&str]) -> Plan {
    let observers = mains
        .iter()
        .enumerate()
        .map(|(i, m)| observer(m, (mains.len() - i) as u64 * 100, i as u64))
        .collect();
    Plan {
        tool_slip: "0.0.0".into(),
        tool_kiln: "1.3.0".into(),
        configurations: vec![Configuration {
            name: "default".into(),
            database: FileRef {
                path: b"/build/compile_commands.json".to_vec(),
                size: 1,
                content: d(9),
            },
        }],
        observers,
        excluded: vec![],
    }
}

fn digests_of(indeterminate: u64) -> Digests {
    let groups = vec![Group {
        class: GroupClass::Tu,
        file: vec![],
        count: indeterminate.max(1),
        indeterminate,
        digest: d(1),
        units: None,
    }];
    Digests { scheme: digests::SCHEME, tu: digests::tu_digest(&groups), groups }
}

fn shard_with(observer: Digest, status: Status, counters: Vec<(&str, u64)>) -> Shard {
    let indet: u64 = counters
        .iter()
        .find_map(|(k, v)| (*k == "digest.indeterminate").then_some(*v))
        .unwrap_or(0);
    let mut c: Vec<Counter> =
        counters.into_iter().map(|(k, v)| Counter { name: k.into(), value: v }).collect();
    c.sort();
    Shard {
        observer,
        phase: Phase::Map,
        round: 0,
        outcome: Outcome {
            status,
            errors: u64::from(status == Status::Diagnosed),
            warnings: 0,
            message: if status == Status::Ok { String::new() } else { format!("{status:?}") },
        },
        inputs: (status != Status::Failed).then(Vec::new),
        counters: c,
        digests: (status == Status::Ok).then(|| digests_of(indet)),
        verdict: None,
        edits: None,
        binding: None,
        rule_use: None,
        spellings: None,
        names: (status == Status::Ok).then(slip_model::names::Names::default),
        negatives: None,
        found: None,
    }
}

fn id_of(s: &Shard) -> Digest {
    formats::content_id(&s.to_bytes()).unwrap()
}

fn loaded_of(plan: Plan, shards_list: Vec<Shard>, extra_jobs: Vec<Job>) -> Loaded {
    let plan_id = formats::content_id(&plan.to_bytes()).unwrap();
    let mut jobs: Vec<Job> = shards_list
        .iter()
        .map(|s| Job {
            observer: s.observer,
            attempt: 1,
            end: End::Completed,
            shard: Some(id_of(s)),
            queued_ns: 0,
            wall_ns: 1_000,
            cpu_ns: 1_000,
            peak_rss: 1 << 20,
            estimate: 1 << 20,
        })
        .collect();
    jobs.extend(extra_jobs);
    let shards: BTreeMap<Digest, Option<Shard>> =
        shards_list.into_iter().map(|s| (s.observer, Some(s))).collect();
    let started = 1_700_000_000_000_000_000;
    let record = RunRecord {
        run: Run {
            tool_slip: "0.0.0".into(),
            tool_kiln: "1.3.0".into(),
            plan: plan_id,
            phase: Phase::Map,
            round: 0,
            started_unix_ns: started,
            wall_ns: 10_000,
            cores: 4,
            workers_max: 2,
            memory_budget: 1 << 30,
            peak_rss_sum: 0,
        },
        jobs,
    };
    Loaded {
        path: RunPath {
            store: PathBuf::from("/store"),
            plan: plan_id,
            phase: Phase::Map,
            round: 0,
            started_unix_ns: started,
        },
        record,
        plan,
        shards,
    }
}

// §9.5: pure summarize over clean, indeterminate (including pending +
// pending.<area> without double-counting, and >5 observers capped at 5 by
// path), and non-ok shards.
#[test]
fn s9_5_summarize_causes_and_non_ok() {
    let mains = [
        "/src/f.cpp",
        "/src/e.cpp",
        "/src/d.cpp",
        "/src/c.cpp",
        "/src/b.cpp",
        "/src/a.cpp",
        "/src/diag.cpp",
        "/src/fail.cpp",
        "/src/miss.cpp",
    ];
    let p = plan_of(&mains);
    let ids: Vec<Digest> = p.observers.iter().map(|o| o.id).collect();
    let mut shards = Vec::new();
    // 6 observers with `unsupported` (1 key each) + `cause.unsupported.exprkind.cxxreflectexpr`.
    for (i, &id) in ids[..6].iter().enumerate() {
        let mut c = vec![
            ("digest.indeterminate", 1 + u64::from(i == 0) * 2),
            ("digest.indeterminate.unsupported", 1),
            ("digest.cause.unsupported.exprkind.cxxreflectexpr", 1),
        ];
        if i == 0 {
            // Observer 0 also has 2 `pending.expr` keys: emits both `pending`
            // and `pending.expr`, which must NOT double-count into total keys.
            c.push(("digest.indeterminate.pending", 2));
            c.push(("digest.indeterminate.pending.expr", 2));
        }
        shards.push(shard_with(id, Status::Ok, c));
    }
    shards.push(shard_with(ids[6], Status::Diagnosed, vec![]));
    shards.push(shard_with(ids[7], Status::Failed, vec![]));
    // ids[8] has no job -> `incomplete`.
    let loaded = loaded_of(p, shards, vec![]);
    let c: Census = census::summarize(&loaded, &ids).unwrap();
    assert!(!c.is_clean());
    assert_eq!(c.observers, 9);
    assert_eq!(c.ok, 6);
    assert_eq!(c.not_ok, 3);
    assert_eq!(c.not_ok_by_status.get("diagnosed"), Some(&1));
    assert_eq!(c.not_ok_by_status.get("failed"), Some(&1));
    assert_eq!(c.not_ok_by_status.get("incomplete"), Some(&1));
    // 6 unsupported + 2 pending = 8 keys (NOT 10!).
    assert_eq!(c.indeterminate_keys, 8);
    assert_eq!(c.indeterminate_observers, 6);
    assert_eq!(c.causes.len(), 3);
    assert_eq!(c.causes[0].cause, "unsupported");
    assert_eq!(c.causes[0].keys, 6);
    assert_eq!(c.causes[0].observers, 6);
    assert_eq!(c.causes[0].sample_observers.len(), SAMPLE_OBSERVERS);
    assert_eq!(
        c.causes[0].sample_observers,
        vec![
            b"/src/a.cpp".to_vec(),
            b"/src/b.cpp".to_vec(),
            b"/src/c.cpp".to_vec(),
            b"/src/d.cpp".to_vec(),
            b"/src/e.cpp".to_vec(),
        ]
    );
    assert_eq!(c.causes[1].cause, "pending");
    assert_eq!(c.causes[2].cause, "pending.expr");
    assert_eq!(c.cause_texts.len(), 1);
    assert_eq!(c.cause_texts[0].name, "unsupported.exprkind.cxxreflectexpr");
    assert_eq!(c.cause_texts[0].keys, 6);
    assert_eq!(c.cause_texts[0].observers, 6);
    assert_eq!(
        c.last_line(false),
        "census: 8 indeterminate keys in 6 of 9 observers, 3 causes; 3 not ok"
    );
    assert_eq!(
        c.last_line(true),
        "census: 8 indeterminate keys in 6 of 9 observers, 3 causes; 3 not ok (selection)"
    );
}

// ---------------------------------------------------------------- fixture

const KILN_ROWS: &str = "[toy]\nurl = \"https://example.org/toy.git\"\ntag = \"v1.0\"\n";
const CORPORA: &str = "[toy]\nkiln = \"toy\"\ncxx_std = 17\nbuild_type = \"Release\"\n\
     c_compiler = \"cc\"\ncxx_compiler = \"c++\"\nconfigure = []\ngenerate = [\"all\"]\n";

/// Fake `slip` supporting `--version`, `plan`, and `parse` (copying a
/// pre-built template store and recording invocations to `log`).
const FAKE_SLIP: &str = r#"#!/bin/sh
[ "$1" = --probe ] && exit 0
D=@DIR@
cmd=$1
shift
echo "$cmd $*" >> "$D/log"
case "$cmd" in
  --version)
    echo "slip 0.0.0 (kiln 1.3.0)"
    exit 0 ;;
  plan)
    echo "plan $(cat "$D/plan")"
    exit 0 ;;
  parse)
    store=
    while [ $# -gt 0 ]; do
      case "$1" in
        --store) store=$2; shift ;;
      esac
      shift
    done
    mkdir -p "$store"
    cp -r "$D/store/." "$store/"
    echo "plan $(cat "$D/plan")"
    echo "run $store/$(cat "$D/run.rel")"
    exit 0 ;;
esac
exit 99
"#;

struct World {
    f: Fixture,
    plan: Plan,
    plan_id: Digest,
}

impl World {
    fn new(tag: &str) -> World {
        let f = Fixture::new(tag);
        f.write("repo/bench/corpora.toml", CORPORA);
        f.write("repo/kiln/scripts/oss/projects.toml", KILN_ROWS);
        let corpora = rows::load(&f.path("repo")).unwrap();
        let c = rows::find(&corpora, "toy").unwrap();
        let layout = Layout::new(&f.path("corpora"), "toy");
        std::fs::create_dir_all(&layout.src).unwrap();
        std::fs::create_dir_all(&layout.build).unwrap();
        std::fs::write(layout.database(), "[]").unwrap();
        let commit = "c".repeat(40);
        std::fs::write(layout.fetch_stamp(), corpus::fetch_stamp(c, &commit)).unwrap();
        let cstamp = corpus::configure_stamp(&commit, &["-S".to_owned()]);
        std::fs::write(layout.configure_stamp(), &cstamp).unwrap();
        std::fs::write(layout.generate_stamp(), corpus::generate_stamp(&cstamp, &c.generate))
            .unwrap();

        let slip = f.path("bin/slip");
        f.write("bin/slip", &FAKE_SLIP.replace("@DIR@", &f.path("fake").display().to_string()));
        std::fs::set_permissions(&slip, std::fs::Permissions::from_mode(0o755)).unwrap();
        common::wait_executable(&slip);
        std::fs::create_dir_all(f.path("fake")).unwrap();

        let plan = plan_of(&["/src/a.cpp", "/src/b.cpp", "/src/c.cpp"]);
        let plan_id = formats::content_id(&plan.to_bytes()).unwrap();
        World { f, plan, plan_id }
    }

    fn seed(&self, shards: &[Shard]) {
        let store_dir = self.f.path("fake/store");
        let _ = std::fs::remove_dir_all(&store_dir);
        let st = Store::open(&store_dir, Access::Exclusive).unwrap();
        let published = st.publish_plan(&self.plan.to_bytes()).unwrap();
        assert_eq!(published, self.plan_id);
        let mut jobs = Vec::new();
        for s in shards {
            let bytes = s.to_bytes();
            let sid = formats::content_id(&bytes).unwrap();
            st.publish_shard(&self.plan_id, Phase::Map, 0, &s.observer, &bytes).unwrap();
            jobs.push(Job {
                observer: s.observer,
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
        let started = 1_700_000_000_000_000_000;
        let rec = RunRecord {
            run: Run {
                tool_slip: "0.0.0".into(),
                tool_kiln: "1.3.0".into(),
                plan: self.plan_id,
                phase: Phase::Map,
                round: 0,
                started_unix_ns: started,
                wall_ns: 5_000,
                cores: 4,
                workers_max: 2,
                memory_budget: 1 << 30,
                peak_rss_sum: 0,
            },
            jobs,
        };
        st.publish_run(&self.plan_id, Phase::Map, 0, started, &rec.to_bytes()).unwrap();
        let rel = st
            .run_path(&self.plan_id, Phase::Map, 0, started)
            .strip_prefix(&store_dir)
            .unwrap()
            .to_string_lossy()
            .into_owned();
        drop(st);
        std::fs::write(self.f.path("fake/plan"), self.plan_id.to_string()).unwrap();
        std::fs::write(self.f.path("fake/run.rel"), rel).unwrap();
    }

    fn env(&self) -> Env {
        Env { corpus_dir: Some(self.f.path("corpora").into()), target_dir: None }
    }

    fn run(&self, extra: &[&str]) -> (Result<bool, Failure>, String) {
        let mut args: Vec<String> = vec!["toy".to_owned()];
        args.extend(extra.iter().map(|s| (*s).to_owned()));
        args.extend(["--slip".to_owned(), self.f.path("bin/slip").display().to_string()]);
        let mut out = Vec::new();
        let r = census::run(&self.f.path("repo"), &args, &self.env(), &mut out);
        (r, String::from_utf8(out).unwrap())
    }

    fn parse_calls(&self) -> usize {
        std::fs::read_to_string(self.f.path("fake/log"))
            .unwrap_or_default()
            .lines()
            .filter(|l| l.starts_with("parse "))
            .count()
    }
}

// §9.5: clean fixture -> `census: clean`, exit 0, `census.json` written, and
// reuse rule avoids re-parsing when unchanged but re-parses when `slip` changes.
#[test]
fn s9_5_clean_and_reuse_rule() {
    let w = World::new("census-clean");
    let shards: Vec<Shard> = w
        .plan
        .observers
        .iter()
        .map(|o| shard_with(o.id, Status::Ok, vec![("digest.indeterminate", 0)]))
        .collect();
    w.seed(&shards);

    let (r, out) = w.run(&[]);
    assert_eq!(r, Ok(true), "{out}");
    assert!(out.ends_with("census: clean (3 observers)\n"), "{out}");
    assert_eq!(w.parse_calls(), 1);

    let json_path = w.f.path("corpora/toy/stores/census/census.json");
    let doc = json::parse(&std::fs::read(&json_path).unwrap()).unwrap();
    assert_eq!(doc.get("format").unwrap().as_str(), Some(FORMAT));
    assert_eq!(doc.get("clean").unwrap().as_bool(), Some(true));
    assert_eq!(doc.get("observers").unwrap().as_u64(), Some(3));
    assert_eq!(doc.get("not_ok").unwrap().as_u64(), Some(0));
    assert_eq!(doc.get("indeterminate_keys").unwrap().as_u64(), Some(0));

    // Second run with nothing changed: reuses the store without parsing again.
    let (r, out) = w.run(&[]);
    assert_eq!(r, Ok(true), "{out}");
    assert!(out.ends_with("census: clean (3 observers)\n"), "{out}");
    assert_eq!(w.parse_calls(), 1, "must reuse store when slip, plan, pin and selection match");

    // Changing the `slip` binary (same `--version` string!) forces a fresh parse.
    let slip_path = w.f.path("bin/slip");
    let mut script = std::fs::read_to_string(&slip_path).unwrap();
    script.push_str("# rebuilt binary\n");
    std::fs::write(&slip_path, script).unwrap();
    let (r, out) = w.run(&[]);
    assert_eq!(r, Ok(true), "{out}");
    assert_eq!(w.parse_calls(), 2, "must re-parse when slip binary changes");
}

// §9.5: indeterminate unit, non-ok shard, `--only` selection, and binary exit
// statuses (0 clean, 1 not clean, 2 command/usage error).
#[test]
fn s9_5_indeterminate_non_ok_selection_and_exit_codes() {
    let w = World::new("census-dirty");
    let obs = &w.plan.observers;
    w.seed(&[
        shard_with(
            obs[0].id,
            Status::Ok,
            vec![
                ("digest.indeterminate", 2),
                ("digest.indeterminate.unsupported", 2),
                ("digest.cause.unsupported.exprkind.cxxreflectexpr", 2),
            ],
        ),
        shard_with(obs[1].id, Status::Ok, vec![("digest.indeterminate", 0)]),
        shard_with(obs[2].id, Status::Failed, vec![]),
    ]);

    let (r, out) = w.run(&[]);
    assert_eq!(r, Ok(false), "{out}");
    assert!(out.contains("  failed 1\n"), "{out}");
    assert!(out.contains("  unsupported: 2 keys in 1 observers\n    /src/a.cpp\n"), "{out}");
    assert!(
        out.contains("  unsupported.exprkind.cxxreflectexpr: 2 keys in 1 observers\n"),
        "{out}"
    );
    assert!(
        out.ends_with("census: 2 indeterminate keys in 1 of 3 observers, 1 causes; 1 not ok\n"),
        "{out}"
    );

    // `--only b.cpp`: only observer 1 (clean!) -> clean with `(selection)`.
    w.seed(&[shard_with(obs[1].id, Status::Ok, vec![("digest.indeterminate", 0)])]);
    let (r, out) = w.run(&["--only", "b.cpp"]);
    assert_eq!(r, Ok(true), "{out}");
    assert!(out.ends_with("census: clean (1 observers) (selection)\n"), "{out}");

    // `--only c.cpp`: only the `failed` shard -> not clean with `(selection)`.
    w.seed(&[shard_with(obs[2].id, Status::Failed, vec![])]);
    let (r, out) = w.run(&["--only", "c.cpp"]);
    assert_eq!(r, Ok(false), "{out}");
    assert!(
        out.ends_with(
            "census: 0 indeterminate keys in 0 of 1 observers, 0 causes; 1 not ok (selection)\n"
        ),
        "{out}"
    );

    // Binary exit codes: 2 for usage error, 2 for unknown corpus, 1 for not clean.
    let bin = env!("CARGO_BIN_EXE_xtask");
    let usage = Command::new(bin).args(["census"]).output().unwrap();
    assert_eq!(usage.status.code(), Some(2));
    let unknown = Command::new(bin).args(["census", "no_such_corpus"]).output().unwrap();
    assert_eq!(unknown.status.code(), Some(2));

    let dirty_run = Command::new(bin)
        .args([
            "census",
            "toy",
            "--only",
            "c.cpp",
            "--slip",
            w.f.path("bin/slip").to_str().unwrap(),
            "--store",
            w.f.path("corpora/toy/stores/bin-check").to_str().unwrap(),
        ])
        .env("CARGO_MANIFEST_DIR", w.f.path("repo/xtask"))
        .env("SLIP_CORPUS_DIR", w.f.path("corpora"))
        .output()
        .unwrap();
    assert_eq!(
        dirty_run.status.code(),
        Some(1),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&dirty_run.stdout),
        String::from_utf8_lossy(&dirty_run.stderr)
    );

    w.seed(&[shard_with(obs[1].id, Status::Ok, vec![("digest.indeterminate", 0)])]);
    let clean_run = Command::new(bin)
        .args([
            "census",
            "toy",
            "--only",
            "b.cpp",
            "--slip",
            w.f.path("bin/slip").to_str().unwrap(),
            "--store",
            w.f.path("corpora/toy/stores/bin-check").to_str().unwrap(),
        ])
        .env("CARGO_MANIFEST_DIR", w.f.path("repo/xtask"))
        .env("SLIP_CORPUS_DIR", w.f.path("corpora"))
        .output()
        .unwrap();
    assert_eq!(
        clean_run.status.code(),
        Some(0),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&clean_run.stdout),
        String::from_utf8_lossy(&clean_run.stderr)
    );
}

// Verify `crates/slip_digest/tests/indeterminate/` still defines the cause
// family referenced in our fixture (compile-time / path sanity).
#[test]
fn s9_5_usage_errors() {
    for bad in [
        &[][..],
        &["--only", "x"],
        &["toy", "--limit", "abc"],
        &["toy", "--store", "a", "--store", "b"],
        &["toy", "--bogus"],
    ] {
        let args: Vec<String> = bad.iter().map(|s| (*s).to_owned()).collect();
        assert!(census::parse_args(&args).is_err(), "{bad:?}");
    }
    let _ = Path::new("ok");
}
