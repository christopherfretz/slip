//! `spec/model.md` §6–§10: the plan, shard and run record in binary and
//! JSON form. Each test names the section it covers.

use std::fs;
use std::path::Path;

use slip_model::container::{TRAILER_LEN, Writer};
use slip_model::formats::{
    content_id, decode_observer, decode_phase, dump, encode_observer, encode_phase, undump,
};
use slip_model::hash::content_digest;
use slip_model::json::{self, Value};
use slip_model::wire::{Decoder, Encoder, ErrorKind};
use slip_model::{
    Command, Configuration, Counter, Digest, End, EnvVar, Excluded, FileRef, Job, Language,
    Observer, Origin, Outcome, Phase, Plan, Reason, Run, RunRecord, Shard, Status,
};

// ---------------------------------------------------------------------------
// Helpers

fn d(n: u8) -> Digest {
    Digest([n; 32])
}

fn file(path: &[u8], size: u64, n: u8) -> FileRef {
    FileRef { path: path.to_vec(), size, content: d(n) }
}

fn command(dir: &str, argv: &[&[u8]]) -> Command {
    Command {
        directory: dir.as_bytes().to_vec(),
        argv: argv.iter().map(|a| a.to_vec()).collect(),
        env: Vec::new(),
    }
}

fn observer(command: Command, cost_hint: u64, origins: &[(u64, u64)]) -> Observer {
    Observer {
        id: command.observer_id(),
        command,
        language: Language::Cxx,
        main: file(b"/src/a.cc", cost_hint, 7),
        cost_hint,
        origins: origins.iter().map(|&(c, e)| Origin { configuration: c, entry: e }).collect(),
    }
}

/// Puts observers in plan order (§7).
fn plan_order(observers: &mut [Observer]) {
    observers.sort_by(|a, b| b.cost_hint.cmp(&a.cost_hint).then(a.id.cmp(&b.id)));
}

/// A representative plan: two configurations, three observers (one with a
/// non-UTF-8 main file, one with a cost above 2^53, two with equal cost),
/// environment variables, and exclusions with a non-UTF-8 file.
fn sample_plan() -> Plan {
    let mut with_env = command("/build", &[b"clang++", b"-c", b"b.cc"]);
    with_env.env = vec![
        EnvVar { name: b"CPATH".to_vec(), value: b"/inc".to_vec() },
        EnvVar { name: b"SDKROOT".to_vec(), value: b"\xfe".to_vec() },
    ];
    let mut odd = observer(command("/build", &[b"cc", b"-c", b"\xff.c"]), 100, &[(1, 0)]);
    odd.language = Language::C;
    odd.main = file(b"/src/\xff.c", 100, 9);
    let mut observers = vec![
        observer(command("/build", &[b"clang++", b"-c", b"a.cc"]), 100, &[(0, 0), (1, 2)]),
        observer(with_env, (1 << 53) + 1, &[(0, 1)]),
        odd,
        observer(command("/other", &[b"g++", b"-c", b"x.cc"]), u64::MAX, &[(0, 5)]),
    ];
    plan_order(&mut observers);
    Plan {
        tool_slip: "0.0.0".to_owned(),
        tool_kiln: "1.3.0".to_owned(),
        configurations: vec![
            Configuration {
                name: "debug".to_owned(),
                database: file(b"/build/compile_commands.json", 4096, 1),
            },
            Configuration {
                name: "release".to_owned(),
                database: file(b"/rel/compile_commands.json", 1 << 53, 2),
            },
        ],
        observers,
        excluded: vec![
            Excluded {
                configuration: 0,
                entry: 2,
                file: b"/src/link".to_vec(),
                reason: Reason::NotACompile,
                detail: String::new(),
            },
            Excluded {
                configuration: 0,
                entry: 3,
                file: b"/src/\x80.cc".to_vec(),
                reason: Reason::UnsupportedFlag,
                detail: "-fno-such-flag".to_owned(),
            },
            Excluded {
                configuration: 1,
                entry: 1,
                file: b"y.f90".to_vec(),
                reason: Reason::UnsupportedLanguage,
                detail: "fortran".to_owned(),
            },
        ],
    }
}

fn sample_shard() -> Shard {
    Shard {
        observer: d(0x33),
        phase: Phase::Map,
        round: 1 << 60,
        outcome: Outcome {
            status: Status::Diagnosed,
            errors: 2,
            warnings: 0,
            message: "a.cc:1:1: error: unknown type name 'x'".to_owned(),
        },
        inputs: Some(vec![
            file(b"/src/a.cc", 10, 1),
            file(b"/src/a.h", 20, 2),
            file(b"/src/\xffb.h", 30, 3),
        ]),
        counters: vec![
            Counter { name: "inputs.bytes".to_owned(), value: 60 },
            Counter { name: "inputs.files".to_owned(), value: 3 },
            Counter { name: "pp.tokens".to_owned(), value: u64::MAX },
        ],
    }
}

fn failed_shard() -> Shard {
    Shard {
        observer: d(0x34),
        phase: Phase::Verify,
        round: 0,
        outcome: Outcome {
            status: Status::Failed,
            errors: 0,
            warnings: 0,
            message: "worker killed: memory cap".to_owned(),
        },
        inputs: None,
        counters: Vec::new(),
    }
}

fn sample_run() -> RunRecord {
    RunRecord {
        run: Run {
            tool_slip: "0.0.0".to_owned(),
            tool_kiln: "1.3.0".to_owned(),
            plan: d(0x44),
            phase: Phase::Map,
            started_unix_ns: 1_763_700_000_000_000_000,
            wall_ns: 12_345_678_901,
            cores: 64,
            workers_max: 63,
            memory_budget: 0,
            peak_rss_sum: (1 << 53) - 1,
        },
        jobs: vec![
            Job {
                observer: d(0x33),
                attempt: 1,
                end: End::Completed,
                shard: Some(d(0x55)),
                queued_ns: 0,
                wall_ns: 1_000_000,
                cpu_ns: 900_000,
                peak_rss: 1 << 30,
                estimate: 1 << 29,
            },
            Job {
                observer: d(0x66),
                attempt: 2,
                end: End::KilledTime,
                shard: None,
                queued_ns: 5,
                wall_ns: 1 << 53,
                cpu_ns: 0,
                peak_rss: 0,
                estimate: u64::MAX,
            },
        ],
    }
}

/// Bytes from hexadecimal fragments (whitespace ignored).
fn hex(parts: &[&str]) -> Vec<u8> {
    let text: String = parts.concat().chars().filter(|c| !c.is_whitespace()).collect();
    assert!(text.len().is_multiple_of(2), "odd hex in test");
    (0..text.len()).step_by(2).map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap()).collect()
}

fn rep(byte: &str, n: usize) -> String {
    byte.repeat(n)
}

/// Recomputes the trailer of a file whose body was patched.
fn reseal(mut bytes: Vec<u8>) -> Vec<u8> {
    let body = bytes.len() - TRAILER_LEN;
    let t = content_digest(&bytes[..body]);
    bytes[body..].copy_from_slice(t.as_bytes());
    bytes
}

/// Sets byte `at` to `to`, checking it held `from`, and reseals.
fn patch(bytes: &[u8], at: usize, from: u8, to: u8) -> Vec<u8> {
    let mut b = bytes.to_vec();
    assert_eq!(b[at], from, "patch: byte {at} is not what the test expects");
    b[at] = to;
    reseal(b)
}

fn sealed(kind: [u8; 4], sections: &[(u64, Vec<u8>)]) -> Vec<u8> {
    let mut w = Writer::new(kind, 1);
    for (id, p) in sections {
        w.section(*id, p);
    }
    w.finish()
}

// A second, independent encoder of the §7–§9 records, used to build files
// that break an invariant (`to_bytes` debug-asserts validity) and to
// cross-check `to_bytes`.

fn raw_file_ref(e: &mut Encoder, f: &FileRef) {
    e.bytes(&f.path).uv(f.size).digest(&f.content);
}

fn raw_plan_sections(p: &Plan) -> Vec<(u64, Vec<u8>)> {
    let mut s1 = Encoder::new();
    s1.str(&p.tool_slip).str(&p.tool_kiln);
    let mut s2 = Encoder::new();
    s2.count(p.configurations.len());
    for c in &p.configurations {
        s2.str(&c.name);
        raw_file_ref(&mut s2, &c.database);
    }
    let mut s3 = Encoder::new();
    s3.count(p.observers.len());
    for o in &p.observers {
        s3.digest(&o.id).bytes(&o.command.directory).count(o.command.argv.len());
        for a in &o.command.argv {
            s3.bytes(a);
        }
        s3.count(o.command.env.len());
        for v in &o.command.env {
            s3.bytes(&v.name).bytes(&v.value);
        }
        s3.uv(o.language as u64);
        raw_file_ref(&mut s3, &o.main);
        s3.uv(o.cost_hint).count(o.origins.len());
        for g in &o.origins {
            s3.uv(g.configuration).uv(g.entry);
        }
    }
    let mut s4 = Encoder::new();
    s4.count(p.excluded.len());
    for x in &p.excluded {
        s4.uv(x.configuration).uv(x.entry).bytes(&x.file).uv(x.reason as u64).str(&x.detail);
    }
    vec![(1, s1.into_bytes()), (2, s2.into_bytes()), (3, s3.into_bytes()), (4, s4.into_bytes())]
}

fn raw_plan(p: &Plan) -> Vec<u8> {
    sealed(*b"PLAN", &raw_plan_sections(p))
}

fn raw_shard_sections(s: &Shard) -> Vec<(u64, Vec<u8>)> {
    let mut s1 = Encoder::new();
    s1.digest(&s.observer).uv(s.phase as u64).uv(s.round);
    let mut s2 = Encoder::new();
    s2.uv(s.outcome.status as u64)
        .uv(s.outcome.errors)
        .uv(s.outcome.warnings)
        .str(&s.outcome.message);
    let mut out = vec![(1, s1.into_bytes()), (2, s2.into_bytes())];
    if let Some(inputs) = &s.inputs {
        let mut s3 = Encoder::new();
        s3.count(inputs.len());
        for f in inputs {
            raw_file_ref(&mut s3, f);
        }
        out.push((3, s3.into_bytes()));
    }
    let mut s4 = Encoder::new();
    s4.count(s.counters.len());
    for c in &s.counters {
        s4.str(&c.name).uv(c.value);
    }
    out.push((4, s4.into_bytes()));
    out
}

fn raw_shard(s: &Shard) -> Vec<u8> {
    sealed(*b"SHRD", &raw_shard_sections(s))
}

fn raw_run(r: &RunRecord) -> Vec<u8> {
    let x = &r.run;
    let mut s1 = Encoder::new();
    s1.str(&x.tool_slip)
        .str(&x.tool_kiln)
        .digest(&x.plan)
        .uv(x.phase as u64)
        .uv(x.started_unix_ns)
        .uv(x.wall_ns)
        .uv(x.cores)
        .uv(x.workers_max)
        .uv(x.memory_budget)
        .uv(x.peak_rss_sum);
    let mut s2 = Encoder::new();
    s2.count(r.jobs.len());
    for j in &r.jobs {
        s2.digest(&j.observer).uv(j.attempt).uv(j.end as u64);
        match &j.shard {
            None => s2.u8(0),
            Some(d) => s2.u8(1).digest(d),
        };
        s2.uv(j.queued_ns).uv(j.wall_ns).uv(j.cpu_ns).uv(j.peak_rss).uv(j.estimate);
    }
    sealed(*b"RUNR", &[(1, s1.into_bytes()), (2, s2.into_bytes())])
}

fn malformed_at(result: Result<impl std::fmt::Debug, slip_model::wire::Error>) -> usize {
    let e = result.expect_err("expected Malformed");
    assert_eq!(e.kind, ErrorKind::Malformed, "{e}");
    e.offset
}

fn kind_of_error<T: std::fmt::Debug>(r: Result<T, slip_model::wire::Error>) -> ErrorKind {
    r.expect_err("expected an error").kind
}

fn parse(text: &str) -> Value {
    json::parse(text.as_bytes()).unwrap()
}

/// Replaces the member at `path` (keys and indices) of a JSON value.
fn set(v: &mut Value, path: &[&str], new: Value) {
    let mut cur = v;
    for step in path {
        cur = match cur {
            Value::Object(m) => &mut m.iter_mut().find(|(k, _)| k == step).unwrap().1,
            Value::Array(a) => &mut a[step.parse::<usize>().unwrap()],
            _ => panic!("set: no {step}"),
        };
    }
    *cur = new;
}

fn members<'v>(v: &'v mut Value, path: &[&str]) -> &'v mut Vec<(String, Value)> {
    let mut cur = v;
    for step in path {
        cur = match cur {
            Value::Object(m) => &mut m.iter_mut().find(|(k, _)| k == step).unwrap().1,
            Value::Array(a) => &mut a[step.parse::<usize>().unwrap()],
            _ => panic!("members: no {step}"),
        };
    }
    match cur {
        Value::Object(m) => m,
        _ => panic!("members: not an object"),
    }
}

// ---------------------------------------------------------------------------
// §7, §8, §9: byte-exact encodings written out by hand

/// A one-observer plan: tool "s"/"k"; configuration "a" (database "/d",
/// size 1, content 11…); observer `cc` in "/" (main "/m", size 2, content
/// 22…), cost 3, origin (0, 0); excluded (0, 1, "x", `not_a_compile`, "").
fn small_plan() -> Plan {
    let cmd = command("/", &[b"cc"]);
    Plan {
        tool_slip: "s".to_owned(),
        tool_kiln: "k".to_owned(),
        configurations: vec![Configuration {
            name: "a".to_owned(),
            database: file(b"/d", 1, 0x11),
        }],
        observers: vec![Observer {
            id: cmd.observer_id(),
            command: cmd,
            language: Language::C,
            main: file(b"/m", 2, 0x22),
            cost_hint: 3,
            origins: vec![Origin { configuration: 0, entry: 0 }],
        }],
        excluded: vec![Excluded {
            configuration: 0,
            entry: 1,
            file: b"x".to_vec(),
            reason: Reason::NotACompile,
            detail: String::new(),
        }],
    }
}

/// `H("slip.observer.v1", 01 2f 01 02 63 63 00)`: the id of `cc` in "/".
const SMALL_OBSERVER_ID: &str = "112055dffe433489c8f0464593c633506997b3e837d21c61ee80710b9ce3ce45";

fn small_plan_bytes() -> Vec<u8> {
    hex(&[
        "53 4c 49 50 1a 0a 00 00  50 4c 41 4e  01 00 00 00", // magic, PLAN, version 1
        "01 04  01 73  01 6b",                               // tool: "s", "k"
        "02 27  01",                                         // configurations: 1
        "01 61  02 2f 64  01",                               // name "a", path "/d", size 1
        &rep("11", 32),                                      // content
        "03 51  01",                                         // observers: 1
        SMALL_OBSERVER_ID,                                   // id
        "01 2f  01 02 63 63  00",                            // directory "/", argv ["cc"], env []
        "01",                                                // language c
        "02 2f 6d  02",                                      // main "/m", size 2
        &rep("22", 32),                                      // content
        "03  01 00 00",                                      // cost 3, origins [(0, 0)]
        "04 07  01  00 01  01 78  01  00", // excluded: (0, 1, "x", not_a_compile, "")
        "00",                              // end
        "108e606ea9675d87ece4dbe2acc8013c1b0c7c2a51f40982eebf77bf5120b397",
    ])
}

/// Observer 33…, map, round 0; diagnosed, 2 errors, 3 warnings, "e";
/// inputs ["/m" size 2 content 22…]; counters [pp.tokens = 300].
fn small_shard() -> Shard {
    Shard {
        observer: d(0x33),
        phase: Phase::Map,
        round: 0,
        outcome: Outcome {
            status: Status::Diagnosed,
            errors: 2,
            warnings: 3,
            message: "e".to_owned(),
        },
        inputs: Some(vec![file(b"/m", 2, 0x22)]),
        counters: vec![Counter { name: "pp.tokens".to_owned(), value: 300 }],
    }
}

fn small_shard_bytes() -> Vec<u8> {
    hex(&[
        "53 4c 49 50 1a 0a 00 00  53 48 52 44  01 00 00 00", // magic, SHRD, version 1
        "01 22",                                             // header
        &rep("33", 32),                                      // observer
        "01 00",                                             // map, round 0
        "02 05  01 02 03  01 65",                            // outcome: diagnosed, 2, 3, "e"
        "03 25  01  02 2f 6d  02",                           // inputs: ["/m", size 2
        &rep("22", 32),                                      //   content]
        "04 0d  01  09 70 70 2e 74 6f 6b 65 6e 73  ac 02",   // counters: [pp.tokens = 300]
        "00",                                                // end
        "6f3a5f463fe6e2b8ba0d60e3a1be66d46c457b303cac5cb9df5e1d476d7be290",
    ])
}

fn small_run() -> RunRecord {
    RunRecord {
        run: Run {
            tool_slip: "s".to_owned(),
            tool_kiln: "k".to_owned(),
            plan: d(0x44),
            phase: Phase::Verify,
            started_unix_ns: 1 << 53,
            wall_ns: 1,
            cores: 2,
            workers_max: 1,
            memory_budget: 0,
            peak_rss_sum: 128,
        },
        jobs: vec![
            Job {
                observer: d(0x33),
                attempt: 1,
                end: End::Completed,
                shard: Some(d(0x55)),
                queued_ns: 0,
                wall_ns: 5,
                cpu_ns: 4,
                peak_rss: 0,
                estimate: 6,
            },
            Job {
                observer: d(0x66),
                attempt: 2,
                end: End::KilledMemory,
                shard: None,
                queued_ns: u64::MAX,
                wall_ns: 0,
                cpu_ns: 0,
                peak_rss: 0,
                estimate: 0,
            },
        ],
    }
}

fn small_run_bytes() -> Vec<u8> {
    hex(&[
        "53 4c 49 50 1a 0a 00 00  52 55 4e 52  01 00 00 00", // magic, RUNR, version 1
        "01 33  01 73  01 6b",                               // run: "s", "k"
        &rep("44", 32),                                      // plan
        "02",                                                // verify
        "80 80 80 80 80 80 80 10",                           // started_unix_ns 2^53
        "01 02 01 00  80 01", // wall 1, cores 2, workers 1, budget 0, peak 128
        "02 7a  02",          // jobs: 2
        &rep("33", 32),       // observer
        "01 00  01",          // attempt 1, completed, shard present
        &rep("55", 32),       // shard
        "00 05 04 00 06",     // queued 0, wall 5, cpu 4, peak 0, estimate 6
        &rep("66", 32),       // observer
        "02 02  00",          // attempt 2, killed_memory, no shard
        "ff ff ff ff ff ff ff ff ff 01", // queued u64::MAX
        "00 00 00 00",        // wall, cpu, peak, estimate 0
        "00",                 // end
        "3ed3528596a187f09a307abf1469b0a94940ae98d64a79731d97720bae538ed8",
    ])
}

// spec/model.md §7, §5: the plan's bytes, trailer included.
#[test]
fn plan_bytes_exact() {
    let bytes = small_plan_bytes();
    assert_eq!(small_plan().to_bytes(), bytes);
    assert_eq!(Plan::from_bytes(&bytes).unwrap(), small_plan());
}

// spec/model.md §8, §5: the shard's bytes, trailer included.
#[test]
fn shard_bytes_exact() {
    let bytes = small_shard_bytes();
    assert_eq!(small_shard().to_bytes(), bytes);
    assert_eq!(Shard::from_bytes(&bytes).unwrap(), small_shard());
}

// spec/model.md §9, §5: the run record's bytes, trailer included.
#[test]
fn run_record_bytes_exact() {
    let bytes = small_run_bytes();
    assert_eq!(small_run().to_bytes(), bytes);
    assert_eq!(RunRecord::from_bytes(&bytes).unwrap(), small_run());
}

// ---------------------------------------------------------------------------
// §7: ObserverId

// spec/model.md §3, §7: ObserverId of a fixed command, and its sensitivity
// to every field.
#[test]
fn observer_id_fixed_and_sensitive() {
    let base = Command {
        directory: b"/build".to_vec(),
        argv: vec![b"clang++".to_vec(), b"-c".to_vec(), b"a.cc".to_vec()],
        env: vec![EnvVar { name: b"CPATH".to_vec(), value: b"/inc".to_vec() }],
    };
    assert_eq!(
        base.observer_id().to_string(),
        "5af092436ee2a53d1769c2ce8fa2d64fce4b8ca3cf275e97bda1235fb017e528"
    );
    let id = command("/", &[b"cc"]).observer_id();
    assert_eq!(id.to_string(), SMALL_OBSERVER_ID);

    let mut variants = Vec::new();
    let mut c = base.clone();
    c.directory = b"/build2".to_vec();
    variants.push(("directory", c));
    let mut c = base.clone();
    c.argv[2] = b"b.cc".to_vec();
    variants.push(("an argument", c));
    let mut c = base.clone();
    c.argv.swap(1, 2);
    variants.push(("argument order", c));
    let mut c = base.clone();
    c.argv.push(Vec::new());
    variants.push(("an empty argument added", c));
    let mut c = base.clone();
    c.env[0].value = b"/inc2".to_vec();
    variants.push(("an environment value", c));
    let mut c = base.clone();
    c.env[0].name = b"CPLUS_INCLUDE_PATH".to_vec();
    variants.push(("an environment name", c));
    let mut c = base.clone();
    c.env.clear();
    variants.push(("the environment removed", c));
    // Moving bytes between fields must change the id (the encoding is
    // length-prefixed).
    let mut c = base.clone();
    c.argv = vec![b"clang++-c".to_vec(), b"a.cc".to_vec()];
    variants.push(("arguments merged", c));
    let mut seen = vec![base.observer_id()];
    for (what, c) in variants {
        let id = c.observer_id();
        assert!(!seen.contains(&id), "changing {what} did not change the id");
        seen.push(id);
    }
}

// ---------------------------------------------------------------------------
// §10 and §7–§9: round trips

fn round_trip_binary_and_json<T>(
    value: &T,
    to_bytes: impl Fn(&T) -> Vec<u8>,
    from_bytes: impl Fn(&[u8]) -> Result<T, slip_model::wire::Error>,
    to_json: impl Fn(&T) -> Value,
    from_json: impl Fn(&Value) -> Result<T, slip_model::formats::JsonFormError>,
) where
    T: PartialEq + std::fmt::Debug,
{
    let bytes = to_bytes(value);
    let back = from_bytes(&bytes).unwrap();
    assert_eq!(&back, value);
    assert_eq!(to_bytes(&back), bytes, "binary → value → binary");
    // Through JSON text, as `slip dump` writes it.
    let text = json::write_pretty(&to_json(value)).unwrap();
    let from_text = from_json(&parse(&text)).unwrap();
    assert_eq!(&from_text, value);
    assert_eq!(to_bytes(&from_text), bytes, "binary → JSON → binary");
    assert_eq!(dump(&bytes).unwrap(), text);
    assert_eq!(undump(&parse(&text)).unwrap(), bytes, "dump → undump");
    assert_eq!(content_id(&bytes).unwrap().as_bytes()[..], bytes[bytes.len() - 32..]);
}

// spec/model.md §7, §10: plans, including non-UTF-8 paths, values at and
// above 2^53, an empty plan.
#[test]
fn plan_round_trips() {
    let p = sample_plan();
    assert_eq!(p.to_bytes(), raw_plan(&p), "to_bytes agrees with the test's encoder");
    round_trip_binary_and_json(
        &p,
        Plan::to_bytes,
        Plan::from_bytes,
        Plan::to_json,
        Plan::from_json,
    );
    round_trip_binary_and_json(
        &small_plan(),
        Plan::to_bytes,
        Plan::from_bytes,
        Plan::to_json,
        Plan::from_json,
    );
    let empty = Plan {
        tool_slip: String::new(),
        tool_kiln: String::new(),
        configurations: Vec::new(),
        observers: Vec::new(),
        excluded: Vec::new(),
    };
    round_trip_binary_and_json(
        &empty,
        Plan::to_bytes,
        Plan::from_bytes,
        Plan::to_json,
        Plan::from_json,
    );
}

// spec/model.md §8, §10: shards, with inputs present, empty and absent.
#[test]
fn shard_round_trips() {
    let mut shards = vec![sample_shard(), failed_shard(), small_shard()];
    let mut empty_inputs = sample_shard();
    empty_inputs.inputs = Some(Vec::new());
    shards.push(empty_inputs);
    let mut no_inputs = sample_shard();
    no_inputs.inputs = None;
    shards.push(no_inputs);
    let mut ok = sample_shard();
    ok.outcome = Outcome { status: Status::Ok, errors: 0, warnings: 7, message: String::new() };
    shards.push(ok);
    let mut at_limit = sample_shard();
    at_limit.outcome.message = "é".repeat(2048);
    shards.push(at_limit);
    for s in &shards {
        assert_eq!(s.to_bytes(), raw_shard(s));
        round_trip_binary_and_json(
            s,
            Shard::to_bytes,
            Shard::from_bytes,
            Shard::to_json,
            Shard::from_json,
        );
    }
    // Absent and empty inputs are different files.
    assert_ne!(shards[3].to_bytes(), shards[4].to_bytes());
}

// spec/model.md §9, §10: run records, with `opt` absent and present.
#[test]
fn run_record_round_trips() {
    for r in [sample_run(), small_run()] {
        assert_eq!(r.to_bytes(), raw_run(&r));
        round_trip_binary_and_json(
            &r,
            RunRecord::to_bytes,
            RunRecord::from_bytes,
            RunRecord::to_json,
            RunRecord::from_json,
        );
    }
}

// spec/model.md §10: the JSON forms of uv (2^53 boundary), bytes (UTF-8 or
// hex), digest, enumerations and opt.
#[test]
fn json_scalar_forms() {
    let r = sample_run();
    let v = r.to_json();
    let run = v.get("sections").unwrap().get("run").unwrap();
    assert_eq!(run.get("peak_rss_sum"), Some(&Value::Int((1 << 53) - 1)));
    assert_eq!(run.get("started_unix_ns"), Some(&Value::Str("1763700000000000000".to_owned())));
    assert_eq!(run.get("phase"), Some(&Value::Str("map".to_owned())));
    assert_eq!(run.get("plan"), Some(&Value::Str("44".repeat(32))));
    let jobs = v.get("sections").unwrap().get("jobs").unwrap().as_array().unwrap();
    assert_eq!(jobs[0].get("shard"), Some(&Value::Str("55".repeat(32))));
    assert_eq!(jobs[1].get("shard"), Some(&Value::Null));
    assert_eq!(jobs[1].get("wall_ns"), Some(&Value::Str("9007199254740992".to_owned())));
    assert_eq!(jobs[1].get("estimate"), Some(&Value::Str("18446744073709551615".to_owned())));
    assert_eq!(jobs[1].get("end"), Some(&Value::Str("killed_time".to_owned())));

    let s = sample_shard().to_json();
    let inputs = s.get("sections").unwrap().get("inputs").unwrap().as_array().unwrap();
    assert_eq!(inputs[0].get("path"), Some(&Value::Str("/src/a.cc".to_owned())));
    assert_eq!(inputs[2].get("path"), Some(&parse(r#"{"hex": "2f7372632fff622e68"}"#)));
    // An absent optional section is not rendered.
    assert_eq!(failed_shard().to_json().get("sections").unwrap().get("inputs"), None);
}

// spec/model.md §5 rule 2, §10: an unknown ignorable section is skipped by
// the typed reader, rendered as "#<id>", and kept by dump → undump.
#[test]
fn unknown_ignorable_section() {
    for (kind, mut sections, plain) in [
        (*b"PLAN", raw_plan_sections(&sample_plan()), sample_plan().to_bytes()),
        (*b"SHRD", raw_shard_sections(&sample_shard()), sample_shard().to_bytes()),
    ] {
        sections.push((64, b"\x00\xffhello".to_vec()));
        sections.push((1000, Vec::new()));
        let bytes = sealed(kind, &sections);
        let text = dump(&bytes).unwrap();
        assert!(text.contains("\"#64\": {\n      \"hex\": \"00ff68656c6c6f\"\n    }"), "{text}");
        assert!(text.contains("\"#1000\": {\n      \"hex\": \"\"\n    }"), "{text}");
        assert_eq!(undump(&parse(&text)).unwrap(), bytes);
        if kind == *b"PLAN" {
            let p = Plan::from_bytes(&bytes).unwrap();
            assert_eq!(p, sample_plan());
            assert_eq!(p.to_bytes(), plain, "the typed value does not keep the section");
            assert_eq!(Plan::from_json(&parse(&text)).unwrap(), sample_plan());
        } else {
            let s = Shard::from_bytes(&bytes).unwrap();
            assert_eq!(s, sample_shard());
            assert_eq!(s.to_bytes(), plain);
            assert_eq!(Shard::from_json(&parse(&text)).unwrap(), sample_shard());
        }
    }
}

// ---------------------------------------------------------------------------
// §10: golden pretty JSON

fn golden(name: &str, actual: &str) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden").join(name);
    if std::env::var_os("SLIP_BLESS").is_some() {
        fs::write(&path, actual).unwrap();
        eprintln!("blessed {}", path.display());
        return;
    }
    let expected = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{}: {e}; run with SLIP_BLESS=1 to create", path.display()));
    assert_eq!(actual, expected, "{name} differs from its golden; SLIP_BLESS=1 rewrites it");
}

// spec/model.md §10: the pretty rendering of a plan, pinned.
#[test]
fn golden_plan() {
    let bytes = sample_plan().to_bytes();
    let text = dump(&bytes).unwrap();
    golden("plan.json", &text);
    assert_eq!(undump(&parse(&text)).unwrap(), bytes);
}

// spec/model.md §10: the pretty rendering of a shard with an unknown
// ignorable section, pinned.
#[test]
fn golden_shard() {
    let mut sections = raw_shard_sections(&sample_shard());
    sections.push((70, vec![1, 2, 3]));
    let bytes = sealed(*b"SHRD", &sections);
    let text = dump(&bytes).unwrap();
    golden("shard.json", &text);
    assert_eq!(undump(&parse(&text)).unwrap(), bytes);
}

// spec/model.md §10: the pretty rendering of a run record, pinned.
#[test]
fn golden_run_record() {
    let bytes = sample_run().to_bytes();
    let text = dump(&bytes).unwrap();
    golden("run.json", &text);
    assert_eq!(undump(&parse(&text)).unwrap(), bytes);
}

// ---------------------------------------------------------------------------
// Invariants: each rejected by from_bytes (Malformed), validate and from_json

fn plan_rejected(p: &Plan, path: &str) -> usize {
    let offset = malformed_at(Plan::from_bytes(&raw_plan(p)));
    let v = p.validate().expect_err("validate accepted an invalid plan");
    assert!(v.starts_with(&format!("{path}: ")), "validate: {v}");
    let j = Plan::from_json(&p.to_json()).expect_err("from_json accepted an invalid plan");
    assert_eq!(j.path, path, "{j}");
    assert!(undump(&p.to_json()).is_err());
    offset
}

fn shard_rejected(s: &Shard, path: &str) -> usize {
    let offset = malformed_at(Shard::from_bytes(&raw_shard(s)));
    let v = s.validate().expect_err("validate accepted an invalid shard");
    assert!(v.starts_with(&format!("{path}: ")), "validate: {v}");
    let j = Shard::from_json(&s.to_json()).expect_err("from_json accepted an invalid shard");
    assert_eq!(j.path, path, "{j}");
    offset
}

// spec/model.md §7: configuration names are unique.
#[test]
fn invariant_configuration_names_unique() {
    let mut p = sample_plan();
    p.configurations[1].name = "debug".to_owned();
    plan_rejected(&p, "sections.configurations[1].name");
}

// spec/model.md §7 "Plan order": cost_hint descending.
#[test]
fn invariant_plan_order_cost() {
    let mut p = sample_plan();
    p.observers.swap(0, 1);
    plan_rejected(&p, "sections.observers[1]");
}

// spec/model.md §7 "Plan order": equal cost_hint, id ascending.
#[test]
fn invariant_plan_order_tie_break() {
    let mut p = sample_plan();
    let n = p.observers.len();
    assert_eq!(p.observers[n - 2].cost_hint, p.observers[n - 1].cost_hint);
    p.observers.swap(n - 2, n - 1);
    plan_rejected(&p, &format!("sections.observers[{}]", n - 1));
}

// spec/model.md §7 "Accounting invariant": id equals ObserverId(command).
#[test]
fn invariant_observer_id_matches_command() {
    let mut p = small_plan();
    p.observers[0].id = d(0);
    let at = plan_rejected(&p, "sections.observers[0].id");
    assert_eq!(at, 66, "the observer's first byte");
}

// spec/model.md §7 "Accounting invariant": observer ids are unique.
#[test]
fn invariant_observer_ids_unique() {
    let mut p = sample_plan();
    let mut twin = p.observers[0].clone();
    twin.cost_hint = 1;
    twin.origins = vec![Origin { configuration: 1, entry: 9 }];
    p.observers.push(twin);
    let n = p.observers.len() - 1;
    plan_rejected(&p, &format!("sections.observers[{n}].id"));
}

// spec/model.md §7: Command.env is sorted by name (strictly).
#[test]
fn invariant_env_sorted() {
    for names in [[&b"SDKROOT"[..], b"CPATH"], [b"CPATH", b"CPATH"]] {
        let mut p = small_plan();
        let o = &mut p.observers[0];
        o.command.env =
            names.iter().map(|n| EnvVar { name: n.to_vec(), value: Vec::new() }).collect();
        o.id = o.command.observer_id();
        plan_rejected(&p, "sections.observers[0].command.env[1]");
    }
}

// spec/model.md §7: an observer has at least one origin.
#[test]
fn invariant_observer_has_an_origin() {
    let mut p = small_plan();
    p.observers[0].origins.clear();
    let at = plan_rejected(&p, "sections.observers[0].origins");
    assert_eq!(at, 143, "the origins count");
}

// spec/model.md §7, §2: origins sorted strictly by (configuration, entry).
#[test]
fn invariant_origins_sorted() {
    for origins in [[(1, 2), (0, 0)], [(0, 1), (0, 0)], [(0, 0), (0, 0)]] {
        let mut p = sample_plan();
        let i = p.observers.iter().position(|o| o.origins.len() == 2).unwrap();
        p.observers[i].origins =
            origins.iter().map(|&(c, e)| Origin { configuration: c, entry: e }).collect();
        plan_rejected(&p, &format!("sections.observers[{i}].origins[1]"));
    }
}

// spec/model.md §7: origin configuration indices are in range.
#[test]
fn invariant_origin_configuration_in_range() {
    let p = {
        let mut p = small_plan();
        p.observers[0].origins[0].configuration = 1;
        p
    };
    assert_eq!(plan_rejected(&p, "sections.observers[0].origins[0]"), 144);
    // The same, by patching the hand-written file.
    assert_eq!(malformed_at(Plan::from_bytes(&patch(&small_plan_bytes(), 144, 0, 1))), 144);
}

// spec/model.md §7: excluded configuration indices are in range.
#[test]
fn invariant_excluded_configuration_in_range() {
    let mut p = sample_plan();
    p.excluded[2].configuration = 2;
    plan_rejected(&p, "sections.excluded[2]");
}

// spec/model.md §7, §2: excluded sorted strictly by (configuration, entry).
#[test]
fn invariant_excluded_sorted() {
    let mut p = sample_plan();
    p.excluded.swap(1, 2);
    plan_rejected(&p, "sections.excluded[2]");
    let mut p = sample_plan();
    p.excluded[1].entry = 2;
    plan_rejected(&p, "sections.excluded[1]");
}

// spec/model.md §7 "Accounting invariant": no (configuration, entry) pair
// twice, between observers or between an observer and an exclusion.
#[test]
fn invariant_no_pair_twice() {
    let mut p = sample_plan();
    let (i, j) = (0, 1);
    p.observers[j].origins = p.observers[i].origins.clone();
    plan_rejected(&p, &format!("sections.observers[{j}].origins[0]"));

    let mut p = small_plan();
    p.excluded[0].entry = 0;
    assert_eq!(plan_rejected(&p, "sections.excluded[0]"), 149);
    assert_eq!(malformed_at(Plan::from_bytes(&patch(&small_plan_bytes(), 150, 1, 0))), 149);
}

// spec/model.md §8: a failed shard has no inputs section.
#[test]
fn invariant_failed_shard_has_no_inputs() {
    let mut s = failed_shard();
    s.inputs = Some(Vec::new());
    shard_rejected(&s, "sections.inputs");
}

// spec/model.md §8: a failed shard has an empty counters list.
#[test]
fn invariant_failed_shard_has_no_counters() {
    let mut s = failed_shard();
    s.counters = vec![Counter { name: "pp.tokens".to_owned(), value: 0 }];
    shard_rejected(&s, "sections.counters[0]");
}

// spec/model.md §8: inputs sorted strictly by path.
#[test]
fn invariant_inputs_sorted_by_path() {
    let mut s = sample_shard();
    s.inputs.as_mut().unwrap().swap(0, 1);
    shard_rejected(&s, "sections.inputs[1]");
    let mut s = sample_shard();
    let inputs = s.inputs.as_mut().unwrap();
    inputs[1].path = inputs[0].path.clone();
    shard_rejected(&s, "sections.inputs[1]");
}

// spec/model.md §8: counters sorted strictly by name.
#[test]
fn invariant_counters_sorted() {
    let mut s = sample_shard();
    s.counters.swap(0, 1);
    shard_rejected(&s, "sections.counters[1]");
    let mut s = sample_shard();
    s.counters[1].name = s.counters[0].name.clone();
    shard_rejected(&s, "sections.counters[1]");
}

// spec/model.md §8: counter names match [a-z0-9_]+(\.[a-z0-9_]+)*.
#[test]
fn invariant_counter_names() {
    for bad in ["", ".", "a.", ".a", "a..b", "A", "pp.Tokens", "a-b", "a b", "é"] {
        let mut s = small_shard();
        s.counters[0].name = bad.to_owned();
        let at = shard_rejected(&s, "sections.counters[0].name");
        assert_eq!(at, 101, "{bad:?}: the counter's first byte");
    }
    for good in ["a", "0", "_", "pp.tokens", "a_b.c_9.d", "x.y.z"] {
        let mut s = small_shard();
        s.counters[0].name = good.to_owned();
        assert_eq!(s.validate(), Ok(()), "{good:?}");
        assert_eq!(Shard::from_bytes(&raw_shard(&s)).unwrap(), s);
    }
}

// spec/model.md §8: Outcome.message is empty for ok.
#[test]
fn invariant_message_empty_for_ok() {
    let mut s = small_shard();
    s.outcome.status = Status::Ok;
    let at = shard_rejected(&s, "sections.outcome.message");
    assert_eq!(at, 57, "the message field");
    assert_eq!(malformed_at(Shard::from_bytes(&patch(&small_shard_bytes(), 54, 1, 0))), 57);
}

// spec/model.md §8: Outcome.message is at most 4096 bytes.
#[test]
fn invariant_message_at_most_4096_bytes() {
    let mut s = sample_shard();
    s.outcome.message = "x".repeat(4096);
    assert_eq!(s.validate(), Ok(()));
    s.outcome.message.push('x');
    shard_rejected(&s, "sections.outcome.message");
    s.outcome.status = Status::Failed;
    s.inputs = None;
    s.counters.clear();
    shard_rejected(&s, "sections.outcome.message");
}

// ---------------------------------------------------------------------------
// §1 rule 6, §2, §5: structural rejections

// spec/model.md §1 rule 6: every unlisted enumeration value is Malformed,
// at the value's first byte.
#[test]
fn unlisted_enumeration_values() {
    let plan = small_plan_bytes();
    for (at, from, to) in [(105, 1, 0), (105, 1, 5), (153, 1, 0), (153, 1, 7)] {
        assert_eq!(malformed_at(Plan::from_bytes(&patch(&plan, at, from, to))), at);
    }
    let shard = small_shard_bytes();
    for (at, from, to) in [(50, 1, 0), (50, 1, 3), (54, 1, 4)] {
        assert_eq!(malformed_at(Shard::from_bytes(&patch(&shard, at, from, to))), at);
    }
    let run = small_run_bytes();
    for (at, from, to) in [(54, 2, 0), (54, 2, 3), (105, 0, 5), (177, 2, 0x7f)] {
        assert_eq!(malformed_at(RunRecord::from_bytes(&patch(&run, at, from, to))), at);
    }
}

// spec/model.md §2 `opt<T>`: a tag other than 0 or 1 is Malformed.
#[test]
fn opt_tag_must_be_0_or_1() {
    let run = small_run_bytes();
    assert_eq!(malformed_at(RunRecord::from_bytes(&patch(&run, 178, 0, 2))), 178);
}

// spec/model.md §5 rule 6: required sections must be present.
#[test]
fn required_sections() {
    let all = raw_plan_sections(&small_plan());
    for missing in 1..=4u64 {
        let sections: Vec<_> = all.iter().filter(|s| s.0 != missing).cloned().collect();
        let bytes = sealed(*b"PLAN", &sections);
        assert_eq!(malformed_at(Plan::from_bytes(&bytes)), bytes.len() - 33, "plan {missing}");
    }
    let all = raw_shard_sections(&small_shard());
    for missing in [1u64, 2, 4] {
        let sections: Vec<_> = all.iter().filter(|s| s.0 != missing).cloned().collect();
        let bytes = sealed(*b"SHRD", &sections);
        assert_eq!(malformed_at(Shard::from_bytes(&bytes)), bytes.len() - 33, "shard {missing}");
    }
    let sections: Vec<_> = all.iter().filter(|s| s.0 != 3).cloned().collect();
    assert!(Shard::from_bytes(&sealed(*b"SHRD", &sections)).unwrap().inputs.is_none());
}

// spec/model.md §5 rule 2, §8: an unknown critical section, including the
// reserved shard sections 16–20, is UnknownSection.
#[test]
fn unknown_critical_sections() {
    let mut sections = raw_plan_sections(&small_plan());
    sections.push((5, Vec::new()));
    let e = Plan::from_bytes(&sealed(*b"PLAN", &sections)).unwrap_err();
    assert_eq!(e.kind, ErrorKind::UnknownSection);
    for id in [5u64, 16, 17, 18, 19, 20, 63] {
        let mut sections = raw_shard_sections(&small_shard());
        sections.push((id, Vec::new()));
        let bytes = sealed(*b"SHRD", &sections);
        assert_eq!(kind_of_error(Shard::from_bytes(&bytes)), ErrorKind::UnknownSection, "{id}");
        assert_eq!(kind_of_error(dump(&bytes)), ErrorKind::UnknownSection);
        assert_eq!(kind_of_error(content_id(&bytes)), ErrorKind::UnknownSection);
    }
}

// spec/model.md §5 rule 3: a section's payload is consumed exactly.
#[test]
fn trailing_bytes_in_a_section() {
    let mut sections = raw_shard_sections(&small_shard());
    sections[0].1.push(0);
    let bytes = sealed(*b"SHRD", &sections);
    assert_eq!(malformed_at(Shard::from_bytes(&bytes)), 18 + 34);
    let mut sections = raw_plan_sections(&small_plan());
    sections[3].1.push(0);
    assert_eq!(kind_of_error(Plan::from_bytes(&sealed(*b"PLAN", &sections))), ErrorKind::Malformed);
}

// spec/model.md §5 rule 5: kind and version are checked; dump and
// content_id dispatch on the kind.
#[test]
fn kind_and_version() {
    let shard = small_shard_bytes();
    assert_eq!(kind_of_error(Plan::from_bytes(&shard)), ErrorKind::BadMagic);
    assert_eq!(kind_of_error(RunRecord::from_bytes(&shard)), ErrorKind::BadMagic);
    let mut other = shard.clone();
    other[8..12].copy_from_slice(b"XXXX");
    assert_eq!(kind_of_error(dump(&reseal(other.clone()))), ErrorKind::BadMagic);
    assert_eq!(kind_of_error(content_id(&reseal(other))), ErrorKind::BadMagic);
    assert_eq!(kind_of_error(dump(b"not a file")), ErrorKind::BadMagic);
    assert_eq!(kind_of_error(dump(&[])), ErrorKind::Truncated);
    let v2 = patch(&shard, 12, 1, 2);
    assert_eq!(kind_of_error(Shard::from_bytes(&v2)), ErrorKind::UnsupportedVersion);
    assert_eq!(kind_of_error(dump(&v2)), ErrorKind::UnsupportedVersion);
    let mut corrupt = shard.clone();
    corrupt[20] ^= 1;
    assert_eq!(kind_of_error(content_id(&corrupt)), ErrorKind::Corrupt);
    assert_eq!(content_id(&shard).unwrap().as_bytes()[..], shard[shard.len() - 32..]);
    assert_eq!(
        content_id(&small_plan_bytes()).unwrap(),
        content_digest(&small_plan_bytes()[..156])
    );
    // content_id validates the content, not only the trailer.
    let bad = patch(&small_plan_bytes(), 144, 0, 1);
    assert_eq!(kind_of_error(content_id(&bad)), ErrorKind::Malformed);
}

// ---------------------------------------------------------------------------
// §10: reading JSON

fn json_error_plan(v: &Value) -> (String, String) {
    let e = Plan::from_json(v).expect_err("from_json accepted it");
    assert_eq!(undump(v).expect_err("undump accepted it"), e);
    (e.path, e.message)
}

// spec/model.md §10: an unknown member is an error naming its path.
#[test]
fn json_unknown_member() {
    let mut v = sample_plan().to_json();
    members(&mut v, &["sections", "observers", "2", "main"]).push(("mode".to_owned(), Value::Null));
    assert_eq!(
        json_error_plan(&v),
        ("sections.observers[2].main.mode".into(), "unknown member".into())
    );
    let mut v = sample_plan().to_json();
    members(&mut v, &["sections"]).push(("extra".to_owned(), Value::Null));
    assert_eq!(json_error_plan(&v).0, "sections.extra");
    let mut v = sample_plan().to_json();
    members(&mut v, &[]).push(("trailer".to_owned(), Value::Null));
    assert_eq!(json_error_plan(&v).0, "trailer");
    // "#<id>" names only an ignorable id, written canonically.
    for key in ["#5", "#63", "#064", "#", "#x"] {
        let mut v = sample_plan().to_json();
        members(&mut v, &["sections"]).push((key.to_owned(), parse(r#"{"hex": ""}"#)));
        assert_eq!(json_error_plan(&v), (format!("sections.{key}"), "unknown member".into()));
    }
}

// spec/model.md §10: a missing member is an error naming its path.
#[test]
fn json_missing_member() {
    let mut v = sample_plan().to_json();
    members(&mut v, &["sections", "observers", "3", "main"]).retain(|(k, _)| k != "size");
    assert_eq!(
        json_error_plan(&v),
        ("sections.observers[3].main.size".into(), "missing member".into())
    );
    let mut v = sample_plan().to_json();
    members(&mut v, &["sections"]).retain(|(k, _)| k != "excluded");
    assert_eq!(json_error_plan(&v).0, "sections.excluded");
    let mut v = sample_run().to_json();
    members(&mut v, &["sections", "jobs", "1"]).retain(|(k, _)| k != "shard");
    let e = RunRecord::from_json(&v).unwrap_err();
    assert_eq!((e.path.as_str(), e.message.as_str()), ("sections.jobs[1].shard", "missing member"));
}

// spec/model.md §10: a value of the wrong type is an error naming its path.
#[test]
fn json_wrong_type() {
    let cases: Vec<(&[&str], Value, &str)> = vec![
        (
            &["sections", "observers", "3", "main"],
            Value::Str("x".into()),
            "sections.observers[3].main",
        ),
        (
            &["sections", "observers", "0", "cost_hint"],
            Value::Float(1.0),
            "sections.observers[0].cost_hint",
        ),
        (
            &["sections", "observers", "0", "cost_hint"],
            Value::Int(-1),
            "sections.observers[0].cost_hint",
        ),
        (&["sections", "observers", "0", "origins"], parse("{}"), "sections.observers[0].origins"),
        (
            &["sections", "observers", "0", "command", "argv", "1"],
            Value::Int(1),
            "sections.observers[0].command.argv[1]",
        ),
        (
            &["sections", "configurations", "0", "name"],
            parse(r#"{"hex": "ff"}"#),
            "sections.configurations[0].name",
        ),
        (&["sections", "tool"], Value::Array(Vec::new()), "sections.tool"),
        (&["sections"], Value::Null, "sections"),
        (&["version"], Value::Str("1".into()), "version"),
    ];
    for (path, new, expected) in cases {
        let mut v = sample_plan().to_json();
        set(&mut v, path, new);
        let (at, message) = json_error_plan(&v);
        assert_eq!(at, expected, "{message}");
    }
    let e = Plan::from_json(&Value::Array(Vec::new())).unwrap_err();
    assert_eq!(e.path, "");
    assert_eq!(e.to_string(), "at the top level: wrong type: expected an object");
}

// spec/model.md §10: only the canonical rendering of a value is accepted
// (see spec/landed/model/WP-006.md "Reading JSON").
#[test]
fn json_non_canonical_forms_rejected() {
    let at = &["sections", "observers", "0", "cost_hint"][..];
    for (new, ok) in [
        (Value::Int((1 << 53) - 1), true),
        (Value::Int(1 << 53), false),
        (Value::Str("9007199254740992".into()), true),
        (Value::Str("9007199254740991".into()), false),
        (Value::Str("09007199254740992".into()), false),
        (Value::Str("+9007199254740992".into()), false),
        (Value::Str("18446744073709551616".into()), false),
        (Value::Str(String::new()), false),
    ] {
        let mut v = sample_plan().to_json();
        set(&mut v, at, new.clone());
        let r = Plan::from_json(&v);
        // Changing the cost can only break plan order, which names the
        // observer, not the field.
        let field_error = r.as_ref().is_err_and(|e| e.path == "sections.observers[0].cost_hint");
        assert_eq!(!field_error, ok, "{new:?}: {r:?}");
    }
    // Bytes: hex form only for invalid UTF-8, lower-case, even length.
    let path = &["sections", "excluded", "0", "file"][..];
    for (new, expected) in [
        (r#"{"hex": "2f"}"#, "sections.excluded[0].file.hex"),
        (r#"{"hex": "FF"}"#, "sections.excluded[0].file.hex"),
        (r#"{"hex": "f"}"#, "sections.excluded[0].file.hex"),
        (r#"{"hex": "ff", "x": 1}"#, "sections.excluded[0].file.x"),
        ("{}", "sections.excluded[0].file.hex"),
    ] {
        let mut v = sample_plan().to_json();
        set(&mut v, path, parse(new));
        assert_eq!(json_error_plan(&v).0, expected, "{new}");
    }
    let mut v = sample_plan().to_json();
    set(&mut v, path, parse(r#"{"hex": "ff"}"#));
    assert_eq!(Plan::from_json(&v).unwrap().excluded[0].file, b"\xff");
    // Digests: 64 lower-case digits.
    let mut v = sample_plan().to_json();
    set(&mut v, &["sections", "observers", "0", "id"], Value::Str("AB".repeat(32)));
    assert_eq!(json_error_plan(&v).0, "sections.observers[0].id");
    // Enumerations: the spec's names.
    let mut v = sample_plan().to_json();
    set(&mut v, &["sections", "observers", "0", "language"], Value::Str("c++".into()));
    assert_eq!(json_error_plan(&v).0, "sections.observers[0].language");
}

// spec/model.md §10: members are in field order, sections in id order
// (spec/landed/model/WP-006.md "Reading JSON").
#[test]
fn json_member_order() {
    let mut v = sample_plan().to_json();
    members(&mut v, &["sections", "observers", "0", "main"]).swap(0, 1);
    assert_eq!(
        json_error_plan(&v),
        ("sections.observers[0].main.path".into(), "member out of order".into())
    );
    let mut v = sample_plan().to_json();
    members(&mut v, &["sections"]).swap(2, 3);
    assert_eq!(json_error_plan(&v).0, "sections.observers");
    let mut v = sample_shard().to_json();
    let m = members(&mut v, &["sections"]);
    m.push(("#80".to_owned(), parse(r#"{"hex": ""}"#)));
    m.push(("#70".to_owned(), parse(r#"{"hex": ""}"#)));
    assert_eq!(Shard::from_json(&v).unwrap_err().path, "sections.#70");
    let mut v = sample_shard().to_json();
    members(&mut v, &["sections"]).insert(0, ("#70".to_owned(), parse(r#"{"hex": ""}"#)));
    assert_eq!(Shard::from_json(&v).unwrap_err().path, "sections.header");
}

// spec/model.md §10: kind and version members.
#[test]
fn json_kind_and_version() {
    let v = sample_shard().to_json();
    assert_eq!(Plan::from_json(&v).unwrap_err().path, "kind");
    assert_eq!(RunRecord::from_json(&v).unwrap_err().path, "kind");
    for version in [0, 2] {
        let mut v = sample_shard().to_json();
        set(&mut v, &["version"], Value::Int(version));
        let e = Shard::from_json(&v).unwrap_err();
        assert_eq!(e.path, "version", "{e}");
        assert_eq!(undump(&v).unwrap_err(), e);
    }
    let mut v = sample_shard().to_json();
    set(&mut v, &["kind"], Value::Str("XXXX".into()));
    assert_eq!(undump(&v).unwrap_err().path, "kind");
    let mut v = sample_run().to_json();
    set(&mut v, &["sections", "jobs", "0", "shard"], Value::Int(0));
    assert_eq!(RunRecord::from_json(&v).unwrap_err().path, "sections.jobs[0].shard");
}

// ---------------------------------------------------------------------------
// Record codecs shared with spec/exec.md §5.2

// spec/model.md §7, spec/exec.md §5.2: an Observer record round-trips and
// is the same bytes as in the plan.
#[test]
fn observer_record_round_trip() {
    let p = sample_plan();
    for o in &p.observers {
        let mut e = Encoder::new();
        encode_observer(o, &mut e);
        let bytes = e.into_bytes();
        let mut dec = Decoder::new(&bytes);
        assert_eq!(&decode_observer(&mut dec).unwrap(), o);
        dec.finish().unwrap();
    }
    // The plan's observers section is the count followed by these records.
    let mut e = Encoder::new();
    e.count(1);
    encode_observer(&small_plan().observers[0], &mut e);
    assert_eq!(e.as_bytes(), &small_plan_bytes()[65..146]);
    // The record is validated on its own.
    let mut bad = small_plan().observers[0].clone();
    bad.id = d(1);
    let mut e = Encoder::new();
    encode_observer(&bad, &mut e);
    let bytes = e.into_bytes();
    let err = decode_observer(&mut Decoder::with_base(&bytes, 100)).unwrap_err();
    assert_eq!((err.kind, err.offset), (ErrorKind::Malformed, 100));
}

// spec/model.md §8, spec/exec.md §5.2: a Phase round-trips; unlisted
// values are Malformed.
#[test]
fn phase_round_trip() {
    for (phase, byte) in [(Phase::Map, 1u8), (Phase::Verify, 2)] {
        let mut e = Encoder::new();
        encode_phase(phase, &mut e);
        assert_eq!(e.as_bytes(), [byte]);
        assert_eq!(decode_phase(&mut Decoder::new(&[byte])).unwrap(), phase);
    }
    for bad in [0u8, 3, 0x7f] {
        assert_eq!(kind_of_error(decode_phase(&mut Decoder::new(&[bad]))), ErrorKind::Malformed);
    }
}

// §8: `Outcome.message` is at most 4096 bytes, cut at a character boundary.
#[test]
fn model_8_clip_message_cuts_at_a_character_boundary() {
    use slip_model::formats::{MESSAGE_MAX, clip_message};
    assert_eq!(MESSAGE_MAX, 4096);
    assert_eq!(clip_message(""), "");
    let exact = "a".repeat(MESSAGE_MAX);
    assert_eq!(clip_message(&exact), exact);
    let long = "a".repeat(MESSAGE_MAX + 10);
    assert_eq!(clip_message(&long), exact);
    // A three-byte character straddling the limit is dropped whole.
    let straddle = format!("{}\u{20ac}tail", "a".repeat(MESSAGE_MAX - 1));
    assert_eq!(clip_message(&straddle), "a".repeat(MESSAGE_MAX - 1));
    // One that ends exactly at the limit is kept.
    let fits = format!("{}\u{20ac}tail", "a".repeat(MESSAGE_MAX - 3));
    assert_eq!(clip_message(&fits).len(), MESSAGE_MAX);
}
