//! `cargo xtask census` (`spec/harness.md` §9.5, WP-166).
//!
//! Tests the pure aggregation (`summarize`, `render`, `to_json`), the
//! end-to-end command against a configured fixture corpus and fake `slip`,
//! the store-reuse rule, and the binary's exit statuses (0, 1, 2).

#![cfg(target_os = "linux")]
#![allow(
    clippy::too_many_lines,
    clippy::many_single_char_names,
    reason = "integration tests set up fixture stores and check multiple reuse and status cases"
)]

mod common;

use common::Fixture;
use slip_exec::store::{Access, Store};
use slip_model::digests::{self, Digests, Group, GroupClass};
use slip_model::hash::content_digest;
use slip_model::json;
use slip_model::{
    Command as Cmd, Configuration, Counter, Digest, End, FileRef, Job, Language, Observer, Origin,
    Outcome, Phase, Plan, Run, RunRecord, Shard, Status, formats,
};
use std::collections::BTreeMap;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use xtask::census::{self, CauseRow, Census, FORMAT, SAMPLE_OBSERVERS};
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
        negatives: (status != Status::Failed).then(Vec::new),
        found: None,
        rebuilt_use: None,
        ownership: None,
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
    assert_eq!(c.unparsed, 1);
    assert_eq!(c.unparsed_by_status.get("diagnosed"), Some(&1));
    assert_eq!(c.not_ok, 2);
    assert_eq!(c.not_ok_by_status.get("failed"), Some(&1));
    assert_eq!(c.not_ok_by_status.get("incomplete"), Some(&1));
    assert_eq!(c.not_ok_by_status.get("diagnosed"), None);
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
        "census: 8 indeterminate keys in 6 of 9 observers, 3 causes; 2 not ok; 1 unparsed (not counted)"
    );
    assert_eq!(
        c.last_line(true),
        "census: 8 indeterminate keys in 6 of 9 observers, 3 causes; 2 not ok; 1 unparsed (not counted) (selection)"
    );
}

// §9.5: unparsed observers (`diagnosed`, `rejected`) are listed and noted in
// the last line without making the census unclean; `ok` without `digests`
// (`no_digests`) and `NoJob`/`NoShard`/`Absent`/`Other` (`incomplete`) are
// counted in `not_ok` and make the census unclean.
#[test]
fn s9_5_unparsed_clean_and_not_ok_statuses() {
    // 1. Clean with `u == 0`.
    let p = plan_of(&["/src/a.cpp", "/src/b.cpp"]);
    let ids: Vec<Digest> = p.observers.iter().map(|o| o.id).collect();
    let loaded = loaded_of(
        p,
        vec![
            shard_with(ids[0], Status::Ok, vec![("digest.indeterminate", 0)]),
            shard_with(ids[1], Status::Ok, vec![("digest.indeterminate", 0)]),
        ],
        vec![],
    );
    let c = census::summarize(&loaded, &ids).unwrap();
    assert!(c.is_clean());
    assert_eq!(c.unparsed, 0);
    assert_eq!(c.last_line(false), "census: clean (2 observers)");
    assert_eq!(c.last_line(true), "census: clean (2 observers) (selection)");

    // 2. Clean with `u > 0` (`diagnosed` + `rejected` do NOT make census unclean).
    let p = plan_of(&["/src/a.cpp", "/src/diag.cpp", "/src/rej.cpp"]);
    let ids: Vec<Digest> = p.observers.iter().map(|o| o.id).collect();
    let loaded = loaded_of(
        p,
        vec![
            shard_with(ids[0], Status::Ok, vec![("digest.indeterminate", 0)]),
            shard_with(ids[1], Status::Diagnosed, vec![]),
            shard_with(ids[2], Status::Rejected, vec![]),
        ],
        vec![],
    );
    let c = census::summarize(&loaded, &ids).unwrap();
    assert!(c.is_clean());
    assert_eq!(c.ok, 1);
    assert_eq!(c.unparsed, 2);
    assert_eq!(c.unparsed_by_status.get("diagnosed"), Some(&1));
    assert_eq!(c.unparsed_by_status.get("rejected"), Some(&1));
    assert_eq!(c.not_ok, 0);
    assert_eq!(c.last_line(false), "census: clean (3 observers; 2 unparsed (not counted))");
    assert_eq!(
        c.last_line(true),
        "census: clean (3 observers; 2 unparsed (not counted)) (selection)"
    );

    // 3. `ok` map shard with `digests: None` (`spec/verify.md` §5.3 P3) -> `no_digests`, unclean.
    let p = plan_of(&["/src/nodig.cpp"]);
    let ids: Vec<Digest> = p.observers.iter().map(|o| o.id).collect();
    let mut no_dig = shard_with(ids[0], Status::Ok, vec![]);
    no_dig.digests = None;
    no_dig.names = None;
    let loaded = loaded_of(p, vec![no_dig], vec![]);
    let c = census::summarize(&loaded, &ids).unwrap();
    assert!(!c.is_clean());
    assert_eq!(c.ok, 0);
    assert_eq!(c.not_ok, 1);
    assert_eq!(c.not_ok_by_status.get("no_digests"), Some(&1));

    // 4. Each incomplete `Side` variant individually counted: `NoJob`, `NoShard`, `Absent`, `Other`.
    // 4a. `NoJob`.
    let p = plan_of(&["/src/nojob.cpp"]);
    let ids: Vec<Digest> = p.observers.iter().map(|o| o.id).collect();
    let loaded = loaded_of(p, vec![], vec![]);
    let c = census::summarize(&loaded, &ids).unwrap();
    assert!(!c.is_clean());
    assert_eq!(c.not_ok, 1);
    assert_eq!(c.not_ok_by_status.get("incomplete"), Some(&1));

    // 4b. `NoShard`.
    let p = plan_of(&["/src/noshard.cpp"]);
    let ids: Vec<Digest> = p.observers.iter().map(|o| o.id).collect();
    let no_shard_job = Job {
        observer: ids[0],
        attempt: 1,
        end: End::Crashed,
        shard: None,
        queued_ns: 0,
        wall_ns: 1_000,
        cpu_ns: 1_000,
        peak_rss: 1 << 20,
        estimate: 1 << 20,
    };
    let loaded = loaded_of(p, vec![], vec![no_shard_job]);
    let c = census::summarize(&loaded, &ids).unwrap();
    assert!(!c.is_clean());
    assert_eq!(c.not_ok, 1);
    assert_eq!(c.not_ok_by_status.get("incomplete"), Some(&1));

    // 4c. `Absent` (job names a shard id, store has `None`).
    let p = plan_of(&["/src/absent.cpp"]);
    let ids: Vec<Digest> = p.observers.iter().map(|o| o.id).collect();
    let absent_job = Job {
        observer: ids[0],
        attempt: 1,
        end: End::Completed,
        shard: Some(d(0x55)),
        queued_ns: 0,
        wall_ns: 1_000,
        cpu_ns: 1_000,
        peak_rss: 1 << 20,
        estimate: 1 << 20,
    };
    let mut loaded = loaded_of(p, vec![], vec![absent_job]);
    loaded.shards.insert(ids[0], None);
    let c = census::summarize(&loaded, &ids).unwrap();
    assert!(!c.is_clean());
    assert_eq!(c.not_ok, 1);
    assert_eq!(c.not_ok_by_status.get("incomplete"), Some(&1));

    // 4d. `Other` (job names a different shard content id than the shard in the store).
    let p = plan_of(&["/src/other.cpp"]);
    let ids: Vec<Digest> = p.observers.iter().map(|o| o.id).collect();
    let sh = shard_with(ids[0], Status::Ok, vec![("digest.indeterminate", 0)]);
    let mut loaded = loaded_of(p, vec![sh], vec![]);
    loaded.record.jobs[0].shard = Some(d(0x66));
    let c = census::summarize(&loaded, &ids).unwrap();
    assert!(!c.is_clean());
    assert_eq!(c.not_ok, 1);
    assert_eq!(c.not_ok_by_status.get("incomplete"), Some(&1));
}

// §9.5: `is_clean` requires `causes.is_empty()` and
// `indeterminate_observers == 0`, dotted non-pending `digest.indeterminate.x.y`
// is counted without a top-level total, and `digest.indeterminate` alone is
// respected via `.max(total_counter)`.
#[test]
fn s9_5_is_clean_guards_and_counter_edge_cases() {
    // 1. Dotted non-pending `digest.indeterminate.x.y` with no top-level counter.
    let p = plan_of(&["/src/dotted.cpp"]);
    let ids: Vec<Digest> = p.observers.iter().map(|o| o.id).collect();
    let loaded = loaded_of(
        p,
        vec![shard_with(ids[0], Status::Ok, vec![("digest.indeterminate.x.y", 2)])],
        vec![],
    );
    let c = census::summarize(&loaded, &ids).unwrap();
    assert!(!c.is_clean());
    assert_eq!(c.indeterminate_keys, 2);
    assert_eq!(c.indeterminate_observers, 1);
    assert_eq!(c.causes.len(), 1);
    assert_eq!(c.causes[0].cause, "x.y");

    // 2. Top-level `digest.indeterminate` alone (no `digest.indeterminate.<cause>`):
    //    kills dropping `.max(total_counter)`.
    let p = plan_of(&["/src/total.cpp"]);
    let ids: Vec<Digest> = p.observers.iter().map(|o| o.id).collect();
    let loaded = loaded_of(
        p,
        vec![shard_with(ids[0], Status::Ok, vec![("digest.indeterminate", 4)])],
        vec![],
    );
    let c = census::summarize(&loaded, &ids).unwrap();
    assert!(!c.is_clean());
    assert_eq!(c.indeterminate_keys, 4);
    assert_eq!(c.indeterminate_observers, 1);
    assert!(c.causes.is_empty());

    // 3. Direct `Census::is_clean()` guards: `indeterminate_observers > 0` and
    //    `!causes.is_empty()` independently mark a census unclean even when
    //    `indeterminate_keys == 0`.
    let base = Census {
        observers: 1,
        omitted: 0,
        ok: 1,
        unparsed: 0,
        unparsed_by_status: BTreeMap::new(),
        not_ok: 0,
        not_ok_by_status: BTreeMap::new(),
        indeterminate_keys: 0,
        indeterminate_observers: 0,
        causes: Vec::new(),
        cause_texts: Vec::new(),
    };
    assert!(base.is_clean());

    let mut only_indet_obs = base.clone();
    only_indet_obs.indeterminate_observers = 1;
    assert!(!only_indet_obs.is_clean());

    let mut only_causes = base.clone();
    only_causes.causes.push(CauseRow {
        cause: "x.y".to_owned(),
        keys: 0,
        observers: 0,
        sample_observers: Vec::new(),
    });
    assert!(!only_causes.is_clean());

    let mut zero_obs = base.clone();
    zero_obs.observers = 0;
    assert!(!zero_obs.is_clean());

    let mut only_unparsed = base;
    only_unparsed.unparsed = 2;
    assert!(only_unparsed.is_clean());
}

// §9.5 end-to-end: `diagnosed` + `rejected` yield a clean census (exit 0,
// `census.json` lists `unparsed` and `unparsed_by_status`); an `ok` map shard
// without `digests` yields an unclean census (`no_digests`, exit 1).
#[test]
fn s9_5_unparsed_and_no_digests_end_to_end() {
    let w = World::new("census-unparsed-e2e");
    let obs = &w.plan.observers;
    w.seed(&[
        shard_with(obs[0].id, Status::Ok, vec![("digest.indeterminate", 0)]),
        shard_with(obs[1].id, Status::Diagnosed, vec![]),
        shard_with(obs[2].id, Status::Rejected, vec![]),
    ]);
    let (r, out) = w.run(&[]);
    assert_eq!(r, Ok(true), "{out}");
    assert!(out.contains("unparsed 2\n  diagnosed 1\n  rejected 1\n"), "{out}");
    assert!(out.ends_with("census: clean (3 observers; 2 unparsed (not counted))\n"), "{out}");
    let doc =
        json::parse(&std::fs::read(w.f.path("corpora/toy/stores/census/census.json")).unwrap())
            .unwrap();
    assert_eq!(doc.get("clean").unwrap().as_bool(), Some(true));
    assert_eq!(doc.get("unparsed").unwrap().as_u64(), Some(2));
    assert_eq!(doc.get("unparsed_by_status").unwrap().get("diagnosed").unwrap().as_u64(), Some(1));
    assert_eq!(doc.get("unparsed_by_status").unwrap().get("rejected").unwrap().as_u64(), Some(1));
    assert_eq!(doc.get("not_ok").unwrap().as_u64(), Some(0));

    let w = World::new("census-no-digests-e2e");
    let obs = &w.plan.observers;
    let mut no_dig = shard_with(obs[0].id, Status::Ok, vec![]);
    no_dig.digests = None;
    no_dig.names = None;
    w.seed(&[
        no_dig,
        shard_with(obs[1].id, Status::Ok, vec![("digest.indeterminate", 0)]),
        shard_with(obs[2].id, Status::Ok, vec![("digest.indeterminate", 0)]),
    ]);
    let (r, out) = w.run(&[]);
    assert_eq!(r, Ok(false), "{out}");
    assert!(out.contains("not_ok 1\n  no_digests 1\n"), "{out}");
    let doc =
        json::parse(&std::fs::read(w.f.path("corpora/toy/stores/census/census.json")).unwrap())
            .unwrap();
    assert_eq!(doc.get("clean").unwrap().as_bool(), Some(false));
    assert_eq!(doc.get("not_ok").unwrap().as_u64(), Some(1));
    assert_eq!(doc.get("not_ok_by_status").unwrap().get("no_digests").unwrap().as_u64(), Some(1));
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
        self.seed_plan(&self.plan, self.plan_id, shards);
    }

    fn seed_plan(&self, plan: &Plan, plan_id: Digest, shards: &[Shard]) {
        let store_dir = self.f.path("fake/store");
        let _ = std::fs::remove_dir_all(&store_dir);
        let st = Store::open(&store_dir, Access::Exclusive).unwrap();
        let published = st.publish_plan(&plan.to_bytes()).unwrap();
        assert_eq!(published, plan_id);
        let mut jobs = Vec::new();
        for s in shards {
            let bytes = s.to_bytes();
            let sid = formats::content_id(&bytes).unwrap();
            st.publish_shard(&plan_id, Phase::Map, 0, &s.observer, &bytes).unwrap();
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
                plan: plan_id,
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
        st.publish_run(&plan_id, Phase::Map, 0, started, &rec.to_bytes()).unwrap();
        let rel = st
            .run_path(&plan_id, Phase::Map, 0, started)
            .strip_prefix(&store_dir)
            .unwrap()
            .to_string_lossy()
            .into_owned();
        drop(st);
        std::fs::write(self.f.path("fake/plan"), plan_id.to_string()).unwrap();
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
    assert_eq!(doc.get("unparsed").unwrap().as_u64(), Some(0));
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

// §9.5: every store-reuse invalidation condition: recorded shard inputs on
// disk (same-size content edit and deletion), `plan`, missing shards,
// observer set, `selection` (`--only` / `--limit`), and round != 0.
#[test]
fn s9_5_reuse_invalidation_rules() {
    // 1. Modifying or deleting a header recorded in `Shard::inputs` invalidates reuse.
    let w = World::new("census-reuse-inputs");
    let hdr = w.f.path("corpora/toy/src/hdr.h");
    let v1 = b"int x = 1;\n";
    std::fs::write(&hdr, v1).unwrap();
    let mk_shards = |bytes: &[u8]| -> Vec<Shard> {
        let fref = FileRef {
            path: hdr.as_os_str().as_bytes().to_vec(),
            size: bytes.len() as u64,
            content: content_digest(bytes),
        };
        w.plan
            .observers
            .iter()
            .map(|o| {
                let mut sh = shard_with(o.id, Status::Ok, vec![("digest.indeterminate", 0)]);
                sh.inputs = Some(vec![fref.clone()]);
                sh
            })
            .collect()
    };
    w.seed(&mk_shards(v1));
    assert_eq!(w.run(&[]).0, Ok(true));
    assert_eq!(w.parse_calls(), 1);
    // Unchanged header on disk -> reuses.
    assert_eq!(w.run(&[]).0, Ok(true));
    assert_eq!(w.parse_calls(), 1);
    // Same-length content change in header -> invalidates reuse.
    let v2 = b"int x = 2;\n";
    assert_eq!(v1.len(), v2.len());
    std::fs::write(&hdr, v2).unwrap();
    w.seed(&mk_shards(v2));
    assert_eq!(w.run(&[]).0, Ok(true));
    assert_eq!(w.parse_calls(), 2, "same-size header edit must invalidate reuse");
    // Unchanged v2 -> reuses again.
    assert_eq!(w.run(&[]).0, Ok(true));
    assert_eq!(w.parse_calls(), 2);
    // Missing header on disk -> invalidates reuse.
    std::fs::remove_file(&hdr).unwrap();
    w.seed(
        &w.plan
            .observers
            .iter()
            .map(|o| shard_with(o.id, Status::Ok, vec![("digest.indeterminate", 0)]))
            .collect::<Vec<_>>(),
    );
    assert_eq!(w.run(&[]).0, Ok(true));
    assert_eq!(w.parse_calls(), 3, "deleted header must invalidate reuse");

    // 2. Plan mismatch: even when the store ALREADY contains a valid round-0
    //    run for the new plan, `prev.plan != plan` must force a fresh parse.
    let w = World::new("census-reuse-plan");
    let shards_a: Vec<Shard> = w
        .plan
        .observers
        .iter()
        .map(|o| shard_with(o.id, Status::Ok, vec![("digest.indeterminate", 0)]))
        .collect();
    w.seed(&shards_a);
    assert_eq!(w.run(&[]).0, Ok(true));
    assert_eq!(w.parse_calls(), 1);
    // Publish a second plan directly into the existing census store, and point
    // `fake/plan` and `fake/store` to it.
    let mut plan_b = w.plan.clone();
    plan_b.configurations[0].database.content = d(0x42);
    let plan_b_id = formats::content_id(&plan_b.to_bytes()).unwrap();
    assert_ne!(plan_b_id, w.plan_id);
    let census_store = w.f.path("corpora/toy/stores/census");
    {
        let st = Store::open(&census_store, Access::Exclusive).unwrap();
        st.publish_plan(&plan_b.to_bytes()).unwrap();
        let mut jobs = Vec::new();
        for s in &shards_a {
            let bytes = s.to_bytes();
            let sid = formats::content_id(&bytes).unwrap();
            st.publish_shard(&plan_b_id, Phase::Map, 0, &s.observer, &bytes).unwrap();
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
        let started = 1_700_000_000_000_000_500;
        let rec = RunRecord {
            run: Run {
                tool_slip: "0.0.0".into(),
                tool_kiln: "1.3.0".into(),
                plan: plan_b_id,
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
        st.publish_run(&plan_b_id, Phase::Map, 0, started, &rec.to_bytes()).unwrap();
    }
    w.seed_plan(&plan_b, plan_b_id, &shards_a);
    assert_eq!(w.run(&[]).0, Ok(true));
    assert_eq!(w.parse_calls(), 2, "plan change must invalidate reuse");

    // 3. Missing shard in store -> invalidates reuse.
    let w = World::new("census-reuse-missing-shard");
    let shards: Vec<Shard> = w
        .plan
        .observers
        .iter()
        .map(|o| shard_with(o.id, Status::Ok, vec![("digest.indeterminate", 0)]))
        .collect();
    w.seed(&shards);
    assert_eq!(w.run(&[]).0, Ok(true));
    assert_eq!(w.parse_calls(), 1);
    let census_store = w.f.path("corpora/toy/stores/census");
    {
        let st = Store::open(&census_store, Access::Shared).unwrap();
        let shard_file = st.shard_path(&w.plan_id, Phase::Map, 0, &w.plan.observers[0].id);
        drop(st);
        std::fs::remove_file(&shard_file).unwrap();
    }
    assert_eq!(w.run(&[]).0, Ok(true));
    assert_eq!(w.parse_calls(), 2, "missing shard must invalidate reuse");

    // 4. Observer set mismatch (`observers != selected`) -> invalidates reuse.
    //    Update `census.json`'s `run` to the 2-observer run so `run_rel == prev.run`
    //    passes and `observers == selected` is the sole reason reuse is rejected.
    let w = World::new("census-reuse-observers");
    let shards: Vec<Shard> = w
        .plan
        .observers
        .iter()
        .map(|o| shard_with(o.id, Status::Ok, vec![("digest.indeterminate", 0)]))
        .collect();
    w.seed(&shards);
    assert_eq!(w.run(&[]).0, Ok(true));
    assert_eq!(w.parse_calls(), 1);
    let census_store = w.f.path("corpora/toy/stores/census");
    {
        // Publish a newer round-0 run covering only 2 of the 3 observers.
        let st = Store::open(&census_store, Access::Exclusive).unwrap();
        let jobs: Vec<Job> = shards[..2]
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
        let started = 1_700_000_000_000_001_000;
        let rec = RunRecord {
            run: Run {
                tool_slip: "0.0.0".into(),
                tool_kiln: "1.3.0".into(),
                plan: w.plan_id,
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
        st.publish_run(&w.plan_id, Phase::Map, 0, started, &rec.to_bytes()).unwrap();
        let new_rel = st
            .run_path(&w.plan_id, Phase::Map, 0, started)
            .strip_prefix(&census_store)
            .unwrap()
            .to_string_lossy()
            .into_owned();
        drop(st);
        rewrite_census_run(&census_store.join("census.json"), &new_rel);
    }
    assert_eq!(w.run(&[]).0, Ok(true));
    assert_eq!(w.parse_calls(), 2, "observer set mismatch must invalidate reuse");

    // 5. Selection change (`--only` and `--limit`) against the same `--store`
    //    even when selected observers match.
    let w = World::new("census-reuse-selection");
    let shared_sel = w.f.path("corpora/toy/stores/sel-reuse");
    let shared_sel_s = shared_sel.to_str().unwrap();
    w.seed(&[shard_with(w.plan.observers[0].id, Status::Ok, vec![("digest.indeterminate", 0)])]);
    assert_eq!(w.run(&["--store", shared_sel_s, "--only", "a.cpp"]).0, Ok(true));
    assert_eq!(w.parse_calls(), 1);
    assert_eq!(w.run(&["--store", shared_sel_s, "--only", "/src/a.cpp"]).0, Ok(true));
    assert_eq!(w.parse_calls(), 2, "changed --only must invalidate reuse");
    assert_eq!(
        w.run(&["--store", shared_sel_s, "--only", "/src/a.cpp", "--limit", "1"]).0,
        Ok(true)
    );
    assert_eq!(w.parse_calls(), 3, "changed --limit must invalidate reuse");

    // 6. Round != 0: a round-1 map run in the store must invalidate reuse.
    //    Update `census.json`'s `run` to the round-1 run so `run_rel == prev.run`
    //    passes and `run.round == 0` is the sole reason reuse is rejected.
    let w = World::new("census-reuse-round");
    let shards: Vec<Shard> = w
        .plan
        .observers
        .iter()
        .map(|o| shard_with(o.id, Status::Ok, vec![("digest.indeterminate", 0)]))
        .collect();
    w.seed(&shards);
    assert_eq!(w.run(&[]).0, Ok(true));
    assert_eq!(w.parse_calls(), 1);
    let census_store = w.f.path("corpora/toy/stores/census");
    {
        let st = Store::open(&census_store, Access::Exclusive).unwrap();
        let mut jobs = Vec::new();
        for o in &w.plan.observers {
            let mut sh = shard_with(o.id, Status::Ok, vec![("digest.indeterminate", 0)]);
            sh.round = 1;
            let bytes = sh.to_bytes();
            let sid = formats::content_id(&bytes).unwrap();
            st.publish_shard(&w.plan_id, Phase::Map, 1, &o.id, &bytes).unwrap();
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
        let started = 1_700_000_000_000_002_000;
        let rec = RunRecord {
            run: Run {
                tool_slip: "0.0.0".into(),
                tool_kiln: "1.3.0".into(),
                plan: w.plan_id,
                phase: Phase::Map,
                round: 1,
                started_unix_ns: started,
                wall_ns: 5_000,
                cores: 4,
                workers_max: 2,
                memory_budget: 1 << 30,
                peak_rss_sum: 0,
            },
            jobs,
        };
        st.publish_run(&w.plan_id, Phase::Map, 1, started, &rec.to_bytes()).unwrap();
        let new_rel = st
            .run_path(&w.plan_id, Phase::Map, 1, started)
            .strip_prefix(&census_store)
            .unwrap()
            .to_string_lossy()
            .into_owned();
        drop(st);
        rewrite_census_run(&census_store.join("census.json"), &new_rel);
    }
    assert_eq!(w.run(&[]).0, Ok(true));
    assert_eq!(w.parse_calls(), 2, "round != 0 map run must invalidate reuse");
}

/// Replaces the `"run"` member of an existing `census.json` in place so tests
/// can isolate later reuse checks from the `run_rel == prev.run` guard.
fn rewrite_census_run(json_path: &Path, new_run: &str) {
    let mut doc = json::parse(&std::fs::read(json_path).unwrap()).unwrap();
    if let json::Value::Object(ref mut members) = doc {
        for (k, v) in members.iter_mut() {
            if k == "run" {
                *v = json::Value::Str(new_run.to_owned());
            }
        }
    }
    std::fs::write(json_path, json::write_pretty(&doc).unwrap()).unwrap();
}

// §9.5: negative-input invalidation (`spec/model.md` §8.6):
// - creating a regular file or dangling symlink at any recorded negative path
//   invalidates reuse and triggers a fresh parse;
// - a shard with `inputs: Some(...)` and `negatives: None` is treated as stale;
// - a shard with `inputs: None` (and `negatives: None`) is treated as stale.
#[test]
fn s9_5_reuse_negative_inputs_and_missing_sections() {
    // 1. Creating a file or dangling symlink at a recorded negative path invalidates reuse.
    let w = World::new("census-reuse-negatives");
    let missed = w.f.path("corpora/toy/src/early_inc.h");
    let missed_bytes = missed.as_os_str().as_bytes().to_vec();
    let shards_with_neg = || -> Vec<Shard> {
        w.plan
            .observers
            .iter()
            .map(|o| {
                let mut sh = shard_with(o.id, Status::Ok, vec![("digest.indeterminate", 0)]);
                sh.negatives = Some(vec![missed_bytes.clone()]);
                sh
            })
            .collect()
    };
    w.seed(&shards_with_neg());
    assert_eq!(w.run(&[]).0, Ok(true));
    assert_eq!(w.parse_calls(), 1);
    // Negative path still absent -> reuses.
    assert_eq!(w.run(&[]).0, Ok(true));
    assert_eq!(w.parse_calls(), 1);

    // Regular file appears at the recorded negative path -> invalidates reuse.
    std::fs::write(&missed, "int shadowed = 1;\n").unwrap();
    assert_eq!(w.run(&[]).0, Ok(true));
    assert_eq!(w.parse_calls(), 2, "creating a file at a negative path must invalidate reuse");

    // Dangling symlink at the recorded negative path -> fails closed, invalidates reuse.
    std::fs::remove_file(&missed).unwrap();
    std::os::unix::fs::symlink("/no/such/target/header.h", &missed).unwrap();
    assert_eq!(w.run(&[]).0, Ok(true));
    assert_eq!(w.parse_calls(), 3, "dangling symlink at a negative path must invalidate reuse");

    // Remove the dangling symlink -> fresh parse then reuses again.
    std::fs::remove_file(&missed).unwrap();
    assert_eq!(w.run(&[]).0, Ok(true));
    assert_eq!(w.parse_calls(), 3, "once absent again, store is reusable");

    // 2. Shard with `inputs: Some(...)` and `negatives: None` must fail closed (stale).
    let w = World::new("census-reuse-missing-negatives");
    let shards_no_neg: Vec<Shard> = w
        .plan
        .observers
        .iter()
        .map(|o| {
            let mut sh = shard_with(o.id, Status::Ok, vec![("digest.indeterminate", 0)]);
            sh.inputs = Some(Vec::new());
            sh.negatives = None;
            sh
        })
        .collect();
    w.seed(&shards_no_neg);
    assert_eq!(w.run(&[]).0, Ok(true));
    assert_eq!(w.parse_calls(), 1);
    assert_eq!(w.run(&[]).0, Ok(true));
    assert_eq!(w.parse_calls(), 2, "inputs: Some(_) with negatives: None must invalidate reuse");

    // 3. Shard with `inputs: None` (and `negatives: None`) must fail closed (stale).
    let w = World::new("census-reuse-missing-inputs");
    let shards_no_inputs: Vec<Shard> = w
        .plan
        .observers
        .iter()
        .map(|o| {
            let mut sh = shard_with(o.id, Status::Ok, vec![("digest.indeterminate", 0)]);
            sh.inputs = None;
            sh.negatives = None;
            sh
        })
        .collect();
    w.seed(&shards_no_inputs);
    assert_eq!(w.run(&[]).0, Ok(true));
    assert_eq!(w.parse_calls(), 1);
    assert_eq!(w.run(&[]).0, Ok(true));
    assert_eq!(w.parse_calls(), 2, "inputs: None must invalidate reuse");
}

// §9.5: a newer map run record written into the store (even with the same
// `--version` string, same plan, round 0, all observers, and current inputs)
// does not match `prev.run` in `census.json`, so reuse is rejected and a fresh
// parse runs (preventing a foreign run from turning an unclean census clean).
#[test]
fn s9_5_reuse_rejects_foreign_newer_run() {
    let w = World::new("census-reuse-foreign-run");
    let obs = &w.plan.observers;
    let dirty_shards = vec![
        shard_with(
            obs[0].id,
            Status::Ok,
            vec![("digest.indeterminate", 1), ("digest.indeterminate.unsupported", 1)],
        ),
        shard_with(obs[1].id, Status::Ok, vec![("digest.indeterminate", 0)]),
        shard_with(obs[2].id, Status::Ok, vec![("digest.indeterminate", 0)]),
    ];
    w.seed(&dirty_shards);
    let (r, out) = w.run(&[]);
    assert_eq!(r, Ok(false), "{out}");
    assert_eq!(w.parse_calls(), 1);

    // Another `slip` with the same `--version` string writes a newer round-0
    // clean run directly into the census store without updating `census.json`.
    let census_store = w.f.path("corpora/toy/stores/census");
    {
        let st = Store::open(&census_store, Access::Exclusive).unwrap();
        let mut jobs = Vec::new();
        for o in obs {
            let sh = shard_with(o.id, Status::Ok, vec![("digest.indeterminate", 0)]);
            let bytes = sh.to_bytes();
            let sid = formats::content_id(&bytes).unwrap();
            st.publish_shard(&w.plan_id, Phase::Map, 0, &o.id, &bytes).unwrap();
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
        let started = 1_700_000_000_000_009_000;
        let rec = RunRecord {
            run: Run {
                tool_slip: "0.0.0".into(),
                tool_kiln: "1.3.0".into(),
                plan: w.plan_id,
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
        st.publish_run(&w.plan_id, Phase::Map, 0, started, &rec.to_bytes()).unwrap();
    }

    // Must NOT reuse the foreign newer run; must re-parse and stay unclean!
    let (r, out) = w.run(&[]);
    assert_eq!(r, Ok(false), "foreign newer run must not turn unclean census clean:\n{out}");
    assert_eq!(w.parse_calls(), 2, "latest_map_run != prev.run must invalidate reuse");
}

// §9.5: changing `kiln_pin` invalidates reuse even when `slip`, `plan`,
// `run`, selection, and shard inputs/negatives are unchanged.
#[test]
fn s9_5_reuse_kiln_pin_invalidation() {
    let w = World::new("census-reuse-kiln-pin");
    let shards: Vec<Shard> = w
        .plan
        .observers
        .iter()
        .map(|o| shard_with(o.id, Status::Ok, vec![("digest.indeterminate", 0)]))
        .collect();
    w.seed(&shards);

    assert_eq!(w.run(&[]).0, Ok(true));
    assert_eq!(w.parse_calls(), 1);
    let json_path = w.f.path("corpora/toy/stores/census/census.json");
    let doc = json::parse(&std::fs::read(&json_path).unwrap()).unwrap();
    assert_eq!(doc.get("kiln_pin").unwrap().as_str(), Some("v1.0"));

    // Unchanged pin -> reuses.
    assert_eq!(w.run(&[]).0, Ok(true));
    assert_eq!(w.parse_calls(), 1);

    // Bump the pin (`v1.0` -> `v2.0`) and refresh fetch stamp (keeping commit,
    // database, plan, slip binary, and store untouched).
    w.f.write(
        "repo/kiln/scripts/oss/projects.toml",
        "[toy]\nurl = \"https://example.org/toy.git\"\ntag = \"v2.0\"\n",
    );
    let corpora = rows::load(&w.f.path("repo")).unwrap();
    let c = rows::find(&corpora, "toy").unwrap();
    let layout = Layout::new(&w.f.path("corpora"), "toy");
    let commit = "c".repeat(40);
    std::fs::write(layout.fetch_stamp(), corpus::fetch_stamp(c, &commit)).unwrap();

    assert_eq!(w.run(&[]).0, Ok(true));
    assert_eq!(w.parse_calls(), 2, "different kiln_pin must invalidate reuse");
    let doc = json::parse(&std::fs::read(&json_path).unwrap()).unwrap();
    assert_eq!(doc.get("kiln_pin").unwrap().as_str(), Some("v2.0"));
}

// §9.5: selections (`--only` / `--limit`) without `--store` use their own
// deterministic store under `<corpus dir>/stores/census-<hex16>`, leaving
// `<corpus dir>/stores/census` untouched, reusing on repeat runs, and printing
// `omitted <k>` for plan entries left out by the selection.
#[test]
fn s9_5_selection_default_store_and_omitted_count() {
    let w = World::new("census-selection-store");
    let obs = &w.plan.observers;
    let all_shards: Vec<Shard> = obs
        .iter()
        .map(|o| shard_with(o.id, Status::Ok, vec![("digest.indeterminate", 0)]))
        .collect();
    w.seed(&all_shards);

    // 1. Full census into default `<corpus>/stores/census`.
    let (r, out) = w.run(&[]);
    assert_eq!(r, Ok(true), "{out}");
    assert_eq!(w.parse_calls(), 1);
    assert!(out.contains("observers 3\n"), "{out}");
    assert!(!out.contains("omitted "), "full run must not print omitted:\n{out}");
    let full_json_path = w.f.path("corpora/toy/stores/census/census.json");
    let full_json_before = std::fs::read(&full_json_path).unwrap();
    let full_doc = json::parse(&full_json_before).unwrap();
    assert_eq!(full_doc.get("omitted").unwrap().as_u64(), Some(0));

    // 2. `--only a.cpp` without `--store` uses its own selection store, leaves
    //    `stores/census` untouched, and reports 2 omitted plan entries.
    w.seed(&[shard_with(obs[0].id, Status::Ok, vec![("digest.indeterminate", 0)])]);
    let (r, out) = w.run(&["--only", "a.cpp"]);
    assert_eq!(r, Ok(true), "{out}");
    assert_eq!(w.parse_calls(), 2);
    assert!(out.contains("observers 1\nomitted 2\n"), "{out}");
    assert!(out.ends_with("census: clean (1 observers) (selection)\n"), "{out}");

    let layout = Layout::new(&w.f.path("corpora"), "toy");
    let only_opts =
        xtask::invariants::Options { only: vec!["a.cpp".to_owned()], ..Default::default() };
    let only_store = census::default_store(&layout, &only_opts);
    assert_ne!(only_store, w.f.path("corpora/toy/stores/census"));
    assert_eq!(only_store.parent(), Some(w.f.path("corpora/toy/stores").as_path()));
    assert!(
        only_store.file_name().unwrap().to_str().unwrap().starts_with("census-"),
        "{}",
        only_store.display()
    );
    let only_doc = json::parse(&std::fs::read(only_store.join("census.json")).unwrap()).unwrap();
    assert_eq!(only_doc.get("observers").unwrap().as_u64(), Some(1));
    assert_eq!(only_doc.get("omitted").unwrap().as_u64(), Some(2));
    assert_eq!(
        std::fs::read(&full_json_path).unwrap(),
        full_json_before,
        "selection census must leave stores/census untouched"
    );

    // 3. Repeat `--only a.cpp` reuses its selection store without re-parsing.
    let (r, out) = w.run(&["--only", "a.cpp"]);
    assert_eq!(r, Ok(true), "{out}");
    assert_eq!(w.parse_calls(), 2, "repeat --only must reuse its own selection store");
    assert!(out.contains("observers 1\nomitted 2\n"), "{out}");

    // 4. Repeat full census still reuses `stores/census` without re-parsing!
    w.seed(&all_shards);
    let (r, out) = w.run(&[]);
    assert_eq!(r, Ok(true), "{out}");
    assert_eq!(w.parse_calls(), 2, "full census must still reuse untouched stores/census");

    // 5. `--limit 2` without `--store` gets its own store and reports 1 omitted.
    w.seed(&[
        shard_with(obs[0].id, Status::Ok, vec![("digest.indeterminate", 0)]),
        shard_with(obs[1].id, Status::Ok, vec![("digest.indeterminate", 0)]),
    ]);
    let (r, out) = w.run(&["--limit", "2"]);
    assert_eq!(r, Ok(true), "{out}");
    assert_eq!(w.parse_calls(), 3);
    assert!(out.contains("observers 2\nomitted 1\n"), "{out}");
    let limit_opts =
        xtask::invariants::Options { limit: Some(("2".to_owned(), 2)), ..Default::default() };
    let limit_store = census::default_store(&layout, &limit_opts);
    assert_ne!(limit_store, only_store);
    assert_ne!(limit_store, w.f.path("corpora/toy/stores/census"));
    assert!(limit_store.join("census.json").is_file());
    let (r, _) = w.run(&["--limit", "2"]);
    assert_eq!(r, Ok(true));
    assert_eq!(w.parse_calls(), 3, "repeat --limit must reuse its own selection store");
    assert_eq!(std::fs::read(&full_json_path).unwrap(), full_json_before);
}

// §9.5: `--store` deletion guard refuses `<corpus>/src`, a subdir of `stores/`
// without `census.json`, and a nested subdir under `stores/` (exit 2, without
// deleting anything), while allowing removal of the default store and a direct
// child of `stores/` that has `census.json`.
#[test]
fn s9_5_store_deletion_guard() {
    let w = World::new("census-store-guard");
    let shards: Vec<Shard> = w
        .plan
        .observers
        .iter()
        .map(|o| shard_with(o.id, Status::Ok, vec![("digest.indeterminate", 0)]))
        .collect();
    w.seed(&shards);

    let bin = env!("CARGO_BIN_EXE_xtask");
    let run_bin = |store: &Path| {
        Command::new(bin)
            .args([
                "census",
                "toy",
                "--slip",
                w.f.path("bin/slip").to_str().unwrap(),
                "--store",
                store.to_str().unwrap(),
            ])
            .env("CARGO_MANIFEST_DIR", w.f.path("repo/xtask"))
            .env("SLIP_CORPUS_DIR", w.f.path("corpora"))
            .output()
            .unwrap()
    };

    // 1. Refuse `<corpus>/src` without deleting its files.
    let src_dir = w.f.path("corpora/toy/src");
    let src_keep = src_dir.join("keep.cpp");
    std::fs::write(&src_keep, "int keep() { return 0; }\n").unwrap();
    let out = run_bin(&src_dir);
    assert_eq!(out.status.code(), Some(2), "stderr={}", String::from_utf8_lossy(&out.stderr));
    assert!(src_keep.is_file(), "<corpus>/src must never be deleted");

    // 2. Refuse a direct child of `stores/` that has no `census.json`.
    let other_dir = w.f.path("corpora/toy/stores/a1");
    std::fs::create_dir_all(&other_dir).unwrap();
    let other_keep = other_dir.join("keep.txt");
    std::fs::write(&other_keep, "keep\n").unwrap();
    let out = run_bin(&other_dir);
    assert_eq!(out.status.code(), Some(2), "stderr={}", String::from_utf8_lossy(&out.stderr));
    assert!(other_keep.is_file(), "stores/a1 without census.json must not be deleted");

    // 3. Refuse a nested directory under `stores/` even when it contains `census.json`.
    let nested_dir = w.f.path("corpora/toy/stores/nested/sub");
    std::fs::create_dir_all(&nested_dir).unwrap();
    std::fs::write(nested_dir.join("census.json"), "{}").unwrap();
    let nested_keep = nested_dir.join("keep.txt");
    std::fs::write(&nested_keep, "keep\n").unwrap();
    let out = run_bin(&nested_dir);
    assert_eq!(out.status.code(), Some(2), "stderr={}", String::from_utf8_lossy(&out.stderr));
    assert!(nested_keep.is_file(), "nested directory under stores/ must not be deleted");

    // 4. Allow deleting and re-parsing a direct child of `stores/` that has `census.json`.
    let custom = w.f.path("corpora/toy/stores/custom");
    let (r, _) = w.run(&["--store", custom.to_str().unwrap()]);
    assert_eq!(r, Ok(true));
    assert!(custom.join("census.json").is_file());
    w.seed(&[shard_with(w.plan.observers[0].id, Status::Ok, vec![("digest.indeterminate", 0)])]);
    let (r, out) = w.run(&["--store", custom.to_str().unwrap(), "--only", "a.cpp"]);
    assert_eq!(r, Ok(true), "{out}");
    assert!(out.ends_with("census: clean (1 observers) (selection)\n"), "{out}");

    // 5. Allow deleting the default store `<corpus>/stores/census` even without `census.json`.
    let default_store = w.f.path("corpora/toy/stores/census");
    std::fs::create_dir_all(&default_store).unwrap();
    std::fs::write(default_store.join("partial"), "x").unwrap();
    w.seed(&shards);
    let (r, out) = w.run(&[]);
    assert_eq!(r, Ok(true), "{out}");
    assert!(!default_store.join("partial").exists());
    assert!(default_store.join("census.json").is_file());
}

// §9.5: stand-in counters (matching the counter names of a known cause,
// `digest.indeterminate.unsupported` and
// `digest.cause.unsupported.exprkind.cxxreflectexpr`; `xtask` is Kiln-free
// and does not run the real `slip` binary), non-ok shard, `--only` selection
// with `omitted`, and binary exit statuses (0 clean, 1 not clean, 2
// command/usage error).
#[test]
fn s9_5_stand_in_counters_non_ok_selection_and_exit_codes() {
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

// §9.5: usage errors rejected by `census::parse_args`.
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
}
