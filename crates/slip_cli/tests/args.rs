//! `spec/exec.md` §8: the command line, parsed without starting a process.

use std::ffi::OsString;
use std::path::PathBuf;

use slip_cli::args::{
    self, DEFAULT_FINDINGS, DEFAULT_LOCATE, DEFAULT_STORE, DEFAULT_WORKER_JOBS,
    DEFAULT_WORKER_STACK, Db, DeclareArgs, ImageArgs, Invocation, ParseArgs, PlanArgs, PlanSource,
    RunArgs, UnitsArgs, UsageError, VerifyArgs, WorkerArgs, parse_size,
};

fn os(words: &[&str]) -> Vec<OsString> {
    words.iter().map(OsString::from).collect()
}

fn parse(words: &[&str]) -> Result<Invocation, UsageError> {
    args::parse(&os(words))
}

fn plan(words: &[&str]) -> PlanArgs {
    match parse(words) {
        Ok(Invocation::Plan(a)) => a,
        other => panic!("{words:?}: {other:?}"),
    }
}

fn parse_args(words: &[&str]) -> ParseArgs {
    match parse(words) {
        Ok(Invocation::Parse(a)) => a,
        other => panic!("{words:?}: {other:?}"),
    }
}

fn usage_error(words: &[&str]) -> String {
    match parse(words) {
        Err(UsageError(line)) => {
            assert!(!line.contains('\n'), "one line: {line:?}");
            line
        }
        other => panic!("{words:?}: expected a usage error, got {other:?}"),
    }
}

/// The options of `RunArgs` when none is given.
fn default_run() -> RunArgs {
    RunArgs {
        store: DEFAULT_STORE.into(),
        jobs: None,
        only: vec![],
        limit: None,
        memory: None,
        observer_memory: None,
        observer_timeout: None,
        estimate_base: None,
        estimate_per_byte: None,
        worker_jobs: DEFAULT_WORKER_JOBS,
        worker_stack: DEFAULT_WORKER_STACK,
    }
}

fn db(name: Option<&str>, path: &str) -> Db {
    Db { name: name.map(str::to_owned), path: PathBuf::from(path) }
}

// §8: the commands and `--version`.
#[test]
fn s8_commands() {
    assert_eq!(parse(&["--version"]), Ok(Invocation::Version));
    assert_eq!(parse(&["--help"]), Ok(Invocation::Help));
    assert_eq!(parse(&["-h"]), Ok(Invocation::Help));
    assert_eq!(parse(&["dump", "x.plan"]), Ok(Invocation::Dump { file: "x.plan".into() }));
    assert_eq!(parse(&["dump", "--", "-x"]), Ok(Invocation::Dump { file: "-x".into() }));
    assert_eq!(parse(&["__panic"]), Ok(Invocation::Panic));
    assert_eq!(
        plan(&["plan", "--db", "c.json"]),
        PlanArgs { dbs: vec![db(None, "c.json")], store: DEFAULT_STORE.into(), jobs: None }
    );
}

// §8: every option of `plan` and `parse` in each of its spellings.
#[test]
fn s8_option_spellings() {
    for words in [
        &["plan", "--db", "c.json", "--store", "x", "-j", "4"][..],
        &["plan", "--db=c.json", "--store=x", "-j4"],
        &["plan", "-j", "4", "--store", "x", "--db", "c.json"],
    ] {
        let a = plan(words);
        assert_eq!(a.dbs, vec![db(None, "c.json")], "{words:?}");
        assert_eq!(a.store, PathBuf::from("x"), "{words:?}");
        assert_eq!(a.jobs, Some(4), "{words:?}");
    }

    let separate = parse_args(&[
        "parse",
        "--plan",
        "abcd",
        "--store",
        "s",
        "-j",
        "3",
        "--only",
        "lib/",
        "--limit",
        "7",
        "--memory",
        "8G",
        "--observer-memory",
        "2G",
        "--observer-timeout",
        "60",
        "--estimate-base",
        "256M",
        "--estimate-per-byte",
        "32",
        "--worker-jobs",
        "2",
        "--worker-stack",
        "64M",
    ]);
    let joined = parse_args(&[
        "parse",
        "--plan=abcd",
        "--store=s",
        "-j3",
        "--only=lib/",
        "--limit=7",
        "--memory=8G",
        "--observer-memory=2G",
        "--observer-timeout=60",
        "--estimate-base=256M",
        "--estimate-per-byte=32",
        "--worker-jobs=2",
        "--worker-stack=64M",
    ]);
    let expected = ParseArgs {
        source: PlanSource::Plan("abcd".into()),
        run: RunArgs {
            store: "s".into(),
            jobs: Some(3),
            only: vec![b"lib/".to_vec()],
            limit: Some(7),
            memory: Some(8 << 30),
            observer_memory: Some(2 << 30),
            observer_timeout: Some(60),
            estimate_base: Some(256 << 20),
            estimate_per_byte: Some(32),
            worker_jobs: 2,
            worker_stack: 64 << 20,
        },
    };
    assert_eq!(separate, expected);
    assert_eq!(joined, expected);

    let defaults = parse_args(&["parse", "--db", "c.json"]);
    assert_eq!(
        defaults,
        ParseArgs { source: PlanSource::Db(vec![db(None, "c.json")]), run: default_run() }
    );
}

// §8.4: the worker's options `--findings` and `--locate` (`limit` and
// `locate` of spec/verify.md §5; defaults 64 and 16, 0 allowed), and no
// other: `--declared`, `--root` and `--post` reach a verify job in the
// job (`Job.post`), never on a worker's command line.
#[test]
fn s8_4_worker_options() {
    assert_eq!((DEFAULT_FINDINGS, DEFAULT_LOCATE), (64, 16));
    assert_eq!(
        parse(&["__worker", "--worker-jobs", "3", "--worker-stack=16M"]),
        Ok(Invocation::Worker(WorkerArgs {
            worker_jobs: 3,
            worker_stack: 16 << 20,
            findings: DEFAULT_FINDINGS,
            locate: DEFAULT_LOCATE,
        }))
    );
    assert_eq!(
        parse(&["__worker"]),
        Ok(Invocation::Worker(WorkerArgs {
            worker_jobs: DEFAULT_WORKER_JOBS,
            worker_stack: DEFAULT_WORKER_STACK,
            findings: DEFAULT_FINDINGS,
            locate: DEFAULT_LOCATE,
        }))
    );
    assert_eq!(
        parse(&["__worker", "--findings", "3", "--locate=0", "--worker-jobs", "2"]),
        Ok(Invocation::Worker(WorkerArgs {
            worker_jobs: 2,
            worker_stack: DEFAULT_WORKER_STACK,
            findings: 3,
            locate: 0,
        }))
    );
    assert_eq!(
        parse(&["__worker", "--findings=0", "--locate", "100"]),
        Ok(Invocation::Worker(WorkerArgs {
            worker_jobs: DEFAULT_WORKER_JOBS,
            worker_stack: DEFAULT_WORKER_STACK,
            findings: 0,
            locate: 100,
        }))
    );
    for words in [
        &["__worker", "--declared", "d.decl"][..],
        &["__worker", "--root", "/s"],
        &["__worker", "--post", "/p"],
        &["__worker", "--findings", "x"],
        &["__worker", "--locate", "-1"],
        &["__worker", "--findings", "1", "--findings", "2"],
        &["__worker", "--locate", "1", "--locate=1"],
    ] {
        let line = usage_error(words);
        assert!(!line.is_empty(), "{words:?}");
    }
    assert!(usage_error(&["__worker", "--root", "/s"]).contains("--root"));
    assert!(
        usage_error(&["__worker", "--findings", "1", "--findings", "2"]).contains("more than once")
    );
    assert!(usage_error(&["__worker", "--findings", "x"]).contains("not a number"));
}

// §8: a value is the next argument even when it starts with `-`; `--`
// ends the options.
#[test]
fn s8_values_and_double_dash() {
    let a = parse_args(&["parse", "--db", "c.json", "--only", "-fno"]);
    assert_eq!(a.run.only, vec![b"-fno".to_vec()]);
    assert_eq!(parse(&["dump", "--", "--store"]), Ok(Invocation::Dump { file: "--store".into() }));
    // After `--` an option name is an argument, which `plan` does not take.
    let line = usage_error(&["plan", "--db", "c.json", "--", "--store", "x"]);
    assert!(line.contains("unexpected argument `--store`"), "{line}");
}

// §8: `--db` and `--only` repeat; a single option given twice is a usage
// error.
#[test]
fn s8_repetition() {
    let a = plan(&["plan", "--db", "a=x.json", "--db=b=y.json"]);
    assert_eq!(a.dbs, vec![db(Some("a"), "x.json"), db(Some("b"), "y.json")]);
    let a = parse_args(&["parse", "--db", "c.json", "--only", "a", "--only=b"]);
    assert_eq!(a.run.only, vec![b"a".to_vec(), b"b".to_vec()]);
    for words in [
        &["plan", "--db", "c.json", "--store", "x", "--store", "y"][..],
        &["plan", "--db", "c.json", "-j1", "-j", "2"],
        &["parse", "--db", "c.json", "--limit", "1", "--limit=2"],
        &["parse", "--plan", "abcd", "--plan", "abce"],
        &["parse", "--db", "c.json", "--memory", "1G", "--memory", "1G"],
        &["__worker", "--worker-jobs", "1", "--worker-jobs", "1"],
    ] {
        let line = usage_error(words);
        assert!(line.contains("more than once"), "{words:?}: {line}");
    }
}

// §8.5 status 2: unknown command or option, missing or malformed argument.
#[test]
fn s8_usage_errors() {
    assert!(usage_error(&[]).contains("no command"));
    assert!(usage_error(&["frobnicate"]).contains("unknown command `frobnicate`"));
    assert!(usage_error(&["--help", "x"]).contains("no arguments"));
    assert!(usage_error(&["-h", "x"]).contains("no arguments"));
    assert!(usage_error(&["help"]).contains("unknown command"));
    assert!(usage_error(&["--version", "x"]).contains("no arguments"));
    assert!(
        usage_error(&["plan", "--db", "c.json", "--bogus"]).contains("unknown option `--bogus`")
    );
    assert!(usage_error(&["plan", "--db", "c.json", "--bogus=1"]).contains("unknown option"));
    assert!(usage_error(&["plan", "--db", "c.json", "--only", "x"]).contains("unknown option"));
    assert!(usage_error(&["dump", "-j", "1", "f"]).contains("unknown option"));
    assert!(usage_error(&["plan", "--db"]).contains("`--db` needs a value"));
    assert!(usage_error(&["plan", "--db", "c.json", "-j"]).contains("`-j` needs a value"));
    assert!(usage_error(&["plan"]).contains("needs `--db`"));
    assert!(usage_error(&["dump"]).contains("needs a file"));
    assert!(usage_error(&["dump", "a", "b"]).contains("one file"));
    assert!(usage_error(&["plan", "--db", "c.json", "extra"]).contains("unexpected argument"));
}

// §8.10: `resource --write <dir>` in each spelling; without `--write`,
// with it twice, with a positional or another option.
#[test]
fn s8_10_resource() {
    for words in [&["resource", "--write", "out"][..], &["resource", "--write=out"]] {
        assert_eq!(parse(words), Ok(Invocation::Resource { write: "out".into() }), "{words:?}");
    }
    assert_eq!(
        parse(&["resource", "--write", "-x"]),
        Ok(Invocation::Resource { write: "-x".into() })
    );
    assert!(usage_error(&["resource"]).contains("`resource` needs `--write`"));
    assert!(usage_error(&["resource", "--write"]).contains("`--write` needs a value"));
    assert!(usage_error(&["resource", "--write", "a", "--write", "b"]).contains("more than once"));
    assert!(usage_error(&["resource", "--write", "a", "b"]).contains("unexpected argument"));
    assert!(usage_error(&["resource", "--write", "a", "-j", "1"]).contains("unknown option"));
}

// §8: malformed numbers and sizes.
#[test]
fn s8_malformed_numbers_and_sizes() {
    for words in [
        &["plan", "--db", "c.json", "-j", "x"][..],
        &["plan", "--db", "c.json", "-j0"],
        &["plan", "--db", "c.json", "-j=4"],
        &["plan", "--db", "c.json", "-j", "-1"],
        &["plan", "--db", "c.json", "-j", "+1"],
        &["plan", "--db", "c.json", "-j", "99999999999999999999"],
        &["parse", "--db", "c.json", "--limit", "1.5"],
        &["parse", "--db", "c.json", "--memory", "1Q"],
        &["parse", "--db", "c.json", "--memory", "16E"],
        &["parse", "--db", "c.json", "--memory", "17179869184G"],
        &["parse", "--db", "c.json", "--observer-memory", "0"],
        &["parse", "--db", "c.json", "--observer-timeout", "1s"],
        &["parse", "--db", "c.json", "--estimate-base", ""],
        &["parse", "--db", "c.json", "--estimate-per-byte", "1K"],
        &["parse", "--db", "c.json", "--worker-jobs", "0"],
        &["parse", "--db", "c.json", "--worker-stack", "0"],
        &["__worker", "--worker-stack", "lots"],
    ] {
        let line = usage_error(words);
        assert!(
            line.contains("not a number")
                || line.contains("not a size")
                || line.contains("at least"),
            "{words:?}: {line}"
        );
    }
    // Zero is a value where it means something: no admission, no timeout,
    // an empty selection.
    let a = parse_args(&["parse", "--db", "c.json", "--memory", "0", "--observer-timeout", "0"]);
    assert_eq!((a.run.memory, a.run.observer_timeout), (Some(0), Some(0)));
    assert_eq!(parse_args(&["parse", "--db", "c.json", "--limit", "0"]).run.limit, Some(0));
}

// §8.2: exactly one of `--plan` and `--db`.
#[test]
fn s8_2_plan_or_db() {
    assert!(usage_error(&["parse", "--plan", "abcd", "--db", "c.json"]).contains("both"));
    assert!(usage_error(&["parse"]).contains("needs `--plan` or `--db`"));
    assert!(usage_error(&["parse", "--store", "s"]).contains("needs `--plan` or `--db`"));
}

// §8: several databases need names, and distinct ones.
#[test]
fn s8_database_names() {
    let line = usage_error(&["plan", "--db", "a.json", "--db", "b.json"]);
    assert!(line.contains("need names"), "{line}");
    let line = usage_error(&["plan", "--db", "x=a.json", "--db", "b.json"]);
    assert!(line.contains("need names") && line.contains("b.json"), "{line}");
    let line = usage_error(&["parse", "--db", "x=a.json", "--db", "x=b.json"]);
    assert!(line.contains("`x`") && line.contains("more than once"), "{line}");
}

// §8: the `--db name=path` split rule.
#[test]
fn s8_db_split_rule() {
    let one = |value: &str| plan(&["plan", "--db", value]).dbs.remove(0);
    assert_eq!(one("debug=build/cc.json"), db(Some("debug"), "build/cc.json"));
    assert_eq!(one("a.b-c_1=x"), db(Some("a.b-c_1"), "x"));
    // Split at the first `=` only: the path may contain `=`.
    assert_eq!(one("rel=dir=1/cc.json"), db(Some("rel"), "dir=1/cc.json"));
    // What precedes the `=` is not a name: the whole value is the path.
    assert_eq!(one("=x.json"), db(None, "=x.json"));
    assert_eq!(one("dir/a=b.json"), db(None, "dir/a=b.json"));
    assert_eq!(one("my db=x.json"), db(None, "my db=x.json"));
    assert_eq!(one("c.json"), db(None, "c.json"));
    // `--db=name=path`: the option's `=` first, then the value's rule.
    assert_eq!(plan(&["plan", "--db=n=p=q"]).dbs, vec![db(Some("n"), "p=q")]);
}

// §8: paths and `--only` values are byte strings, not required to be UTF-8.
#[cfg(unix)]
#[test]
fn s8_byte_strings() {
    use std::os::unix::ffi::OsStrExt;
    let raw = |b: &[u8]| std::ffi::OsStr::from_bytes(b).to_owned();
    let a = args::parse(&[
        "parse".into(),
        "--db".into(),
        raw(b"n=caf\xe9.json"),
        raw(b"--store=st\xff"),
        "--only".into(),
        raw(b"\xfe"),
    ])
    .unwrap();
    let Invocation::Parse(a) = a else { panic!("{a:?}") };
    assert_eq!(
        a.source,
        PlanSource::Db(vec![Db {
            name: Some("n".into()),
            path: PathBuf::from(raw(b"caf\xe9.json"))
        }])
    );
    assert_eq!(a.run.store, PathBuf::from(raw(b"st\xff")));
    assert_eq!(a.run.only, vec![b"\xfe".to_vec()]);
}

// §8 `<size>`: decimal, suffixes K M G T in powers of 1024, zero; `None` on
// overflow and junk.
#[test]
fn s8_parse_size() {
    assert_eq!(parse_size("0"), Some(0));
    assert_eq!(parse_size("0K"), Some(0));
    assert_eq!(parse_size("512"), Some(512));
    assert_eq!(parse_size("1K"), Some(1024));
    assert_eq!(parse_size("3M"), Some(3 << 20));
    assert_eq!(parse_size("2G"), Some(2 << 30));
    assert_eq!(parse_size("5T"), Some(5 << 40));
    assert_eq!(parse_size("007"), Some(7));
    assert_eq!(parse_size("18446744073709551615"), Some(u64::MAX));
    assert_eq!(parse_size("16777215T"), Some(16_777_215 << 40));
    for junk in [
        "",
        "K",
        "1k",
        "1m",
        "1KB",
        "1 K",
        " 1",
        "1.5G",
        "-1",
        "+1",
        "1E",
        "0x10",
        "1KK",
        "18446744073709551616",
        "16777216T",
        "17179869184G",
    ] {
        assert_eq!(parse_size(junk), None, "{junk:?}");
    }
}

// §8.5: the usage text lists the commands but not the hidden ones.
#[test]
fn s8_usage_text() {
    for word in [
        "slip plan",
        "slip parse",
        "slip dump",
        "slip resource --write",
        "slip verify --plan",
        "slip declare --plan",
        "slip units --plan",
        "--post-image",
        "--strict-names",
        "--declared <file>",
        "--match <substring>",
        "slip --version",
        "slip --help",
        "--worker-stack",
    ] {
        assert!(args::USAGE.contains(word), "{word}");
    }
    assert!(!args::USAGE.contains("__worker") && !args::USAGE.contains("__panic"));
}

fn verify_args(words: &[&str]) -> VerifyArgs {
    match parse(words) {
        Ok(Invocation::Verify(a)) => a,
        other => panic!("{words:?}: {other:?}"),
    }
}

fn declare_args(words: &[&str]) -> DeclareArgs {
    match parse(words) {
        Ok(Invocation::Declare(a)) => a,
        other => panic!("{words:?}: {other:?}"),
    }
}

fn units_args(words: &[&str]) -> UnitsArgs {
    match parse(words) {
        Ok(Invocation::Units(a)) => a,
        other => panic!("{words:?}: {other:?}"),
    }
}

fn image(pairs: &[(&str, &str)]) -> ImageArgs {
    ImageArgs {
        roots: pairs.iter().map(|(r, _)| PathBuf::from(r)).collect(),
        posts: pairs.iter().map(|(_, p)| PathBuf::from(p)).collect(),
    }
}

// §8.7: every option of `verify`, its defaults, and the options it shares
// with `parse` (`RunArgs`) parsed as `parse` parses them.
#[test]
fn s8_7_verify_options() {
    let all = verify_args(&[
        "verify",
        "--plan",
        "abcd",
        "--round=3",
        "--declared",
        "d.decl",
        "--root",
        "/s",
        "--post=/p",
        "--root=/t",
        "--post",
        "/q",
        "--findings",
        "5",
        "--locate=0",
        "--report",
        "r.json",
        "--strict-names",
        "--store=s",
        "-j3",
        "--only",
        "lib/",
        "--only=x",
        "--limit=7",
        "--memory=8G",
        "--observer-memory=2G",
        "--observer-timeout=60",
        "--estimate-base=256M",
        "--estimate-per-byte=32",
        "--worker-jobs=2",
        "--worker-stack=64M",
    ]);
    assert_eq!(
        all,
        VerifyArgs {
            plan: "abcd".into(),
            round: 3,
            declared: Some("d.decl".into()),
            image: image(&[("/s", "/p"), ("/t", "/q")]),
            findings: 5,
            locate: 0,
            report: Some("r.json".into()),
            strict_names: true,
            run: RunArgs {
                store: "s".into(),
                jobs: Some(3),
                only: vec![b"lib/".to_vec(), b"x".to_vec()],
                limit: Some(7),
                memory: Some(8 << 30),
                observer_memory: Some(2 << 30),
                observer_timeout: Some(60),
                estimate_base: Some(256 << 20),
                estimate_per_byte: Some(32),
                worker_jobs: 2,
                worker_stack: 64 << 20,
            },
        }
    );
    assert_eq!(
        verify_args(&["verify", "--plan", "p.plan"]),
        VerifyArgs {
            plan: "p.plan".into(),
            round: 0,
            declared: None,
            image: ImageArgs::default(),
            findings: DEFAULT_FINDINGS,
            locate: DEFAULT_LOCATE,
            report: None,
            strict_names: false,
            run: default_run(),
        }
    );
    // `--round`, `--findings`, `--locate` may be 0.
    let z = verify_args(&["verify", "--plan", "p", "--round", "0", "--findings", "0"]);
    assert_eq!((z.round, z.findings), (0, 0));
}

// §8.7: `verify` needs `--plan`, has no `--db`, and refuses what is not its
// own; single options given twice; malformed numbers.
#[test]
fn s8_7_verify_usage_errors() {
    assert_eq!(usage_error(&["verify"]), "`verify` needs `--plan`");
    assert_eq!(usage_error(&["verify", "--store", "s"]), "`verify` needs `--plan`");
    assert!(usage_error(&["verify", "--plan", "p", "--db", "c.json"]).contains("unknown option"));
    assert!(usage_error(&["verify", "--plan", "p", "--from", "x"]).contains("unknown option"));
    assert!(usage_error(&["verify", "--plan", "p", "--post-image"]).contains("unknown option"));
    assert!(usage_error(&["verify", "--plan", "p", "extra"]).contains("unexpected argument"));
    // `--strict-names` takes no value, is `verify`'s alone, and is given once.
    assert_eq!(
        usage_error(&["verify", "--plan", "p", "--strict-names=1"]),
        "`--strict-names` takes no value"
    );
    assert!(verify_args(&["verify", "--strict-names", "--plan", "p"]).strict_names);
    assert!(
        usage_error(&["verify", "--plan", "p", "--strict-names", "--strict-names"])
            .contains("more than once")
    );
    assert!(usage_error(&["units", "--plan", "p", "--strict-names"]).contains("unknown option"));
    for twice in ["--round", "--declared", "--findings", "--locate", "--report", "--plan"] {
        let line = usage_error(&["verify", "--plan", "p", twice, "1", twice, "1"]);
        assert!(line.contains("more than once"), "{twice}: {line}");
    }
    for (opt, bad) in [("--round", "x"), ("--findings", "-1"), ("--locate", "1K"), ("-j", "0")] {
        let line = usage_error(&["verify", "--plan", "p", opt, bad]);
        assert!(line.contains("not a number") || line.contains("at least"), "{opt}: {line}");
    }
}

// §8 "Images": the k-th `--root` and the k-th `--post` are one root map;
// different numbers of the two are a usage error, for each command that
// takes them.
#[test]
fn s8_unpaired_root_and_post() {
    for command in [&["verify", "--plan", "p"][..], &["units", "--plan", "p"]] {
        let mut words = command.to_vec();
        words.extend(["--root", "/s"]);
        assert_eq!(
            usage_error(&words),
            "`--root` and `--post` come in pairs: 1 `--root` and 0 `--post` given",
            "{command:?}"
        );
        let mut words = command.to_vec();
        words.extend(["--post", "/p", "--root", "/s", "--post", "/q"]);
        assert_eq!(
            usage_error(&words),
            "`--root` and `--post` come in pairs: 1 `--root` and 2 `--post` given",
            "{command:?}"
        );
    }
    let line =
        usage_error(&["declare", "--plan", "p", "--from", "f", "--out", "o", "--post", "/p"]);
    assert_eq!(line, "`--root` and `--post` come in pairs: 0 `--root` and 1 `--post` given");
    // Paired in the order given, whatever the interleaving.
    let a = verify_args(&["verify", "--plan", "p", "--post", "/p", "--root", "/s"]);
    assert_eq!(a.image, image(&[("/s", "/p")]));
}

// §8.8: the options of `declare`; `--plan`, `--from` and `--out` required;
// no option of the pool.
#[test]
fn s8_8_declare_options() {
    assert_eq!(
        declare_args(&[
            "declare",
            "--plan",
            "abcd",
            "--from",
            "t.txt",
            "--out=d.decl",
            "--owned",
            "/s/a",
            "--readonly=/s/a/dep",
            "--owned",
            "/s/b",
            "--root",
            "/s",
            "--post",
            "/p",
            "--store",
            "s",
            "--only",
            "a",
            "--only",
            "b",
            "--limit",
            "2",
        ]),
        DeclareArgs {
            plan: "abcd".into(),
            from: "t.txt".into(),
            out: "d.decl".into(),
            owned: vec!["/s/a".into(), "/s/b".into()],
            readonly: vec!["/s/a/dep".into()],
            image: image(&[("/s", "/p")]),
            store: "s".into(),
            only: vec![b"a".to_vec(), b"b".to_vec()],
            limit: Some(2),
        }
    );
    let d = declare_args(&["declare", "--plan", "p", "--from", "t", "--out", "o"]);
    assert_eq!(
        (d.store, d.image, d.limit),
        (PathBuf::from(DEFAULT_STORE), ImageArgs::default(), None)
    );
    assert!(d.owned.is_empty() && d.readonly.is_empty());
    // `--readonly` carves out of what `--owned` gives (`spec/edits.md` §2.1).
    assert_eq!(
        usage_error(&["declare", "--plan", "p", "--from", "t", "--out", "o", "--readonly", "/d"]),
        "`--readonly` needs `--owned`"
    );
    assert_eq!(usage_error(&["declare", "--from", "t", "--out", "o"]), "`declare` needs `--plan`");
    assert_eq!(usage_error(&["declare", "--plan", "p", "--out", "o"]), "`declare` needs `--from`");
    assert_eq!(usage_error(&["declare", "--plan", "p", "--from", "t"]), "`declare` needs `--out`");
    for other in ["-j", "--memory", "--declared", "--report", "--match", "--round"] {
        let line =
            usage_error(&["declare", "--plan", "p", "--from", "t", "--out", "o", other, "1"]);
        assert!(line.contains("unknown option"), "{other}: {line}");
    }
    let line = usage_error(&["declare", "--plan", "p", "--from", "t", "--from", "u", "--out", "o"]);
    assert!(line.contains("more than once"), "{line}");
}

// §8.9: the options of `units`; `--post-image` takes no value; `--match`
// repeats.
#[test]
fn s8_9_units_options() {
    assert_eq!(
        units_args(&[
            "units",
            "--plan",
            "abcd",
            "--post-image",
            "--root",
            "/s",
            "--post",
            "/p",
            "--store",
            "s",
            "--only",
            "a",
            "--limit",
            "1",
            "--match",
            "f(",
            "--match=g",
        ]),
        UnitsArgs {
            plan: "abcd".into(),
            post_image: true,
            image: image(&[("/s", "/p")]),
            store: "s".into(),
            only: vec![b"a".to_vec()],
            limit: Some(1),
            matches: vec![b"f(".to_vec(), b"g".to_vec()],
        }
    );
    let u = units_args(&["units", "--plan", "p"]);
    assert!(!u.post_image && u.matches.is_empty() && u.image == ImageArgs::default());
    // The flag before another option: it takes nothing from it.
    let u = units_args(&["units", "--post-image", "--plan", "p"]);
    assert!(u.post_image);
    assert_eq!(u.plan, OsString::from("p"));
    assert_eq!(usage_error(&["units"]), "`units` needs `--plan`");
    assert_eq!(
        usage_error(&["units", "--plan", "p", "--post-image=1"]),
        "`--post-image` takes no value"
    );
    assert!(
        usage_error(&["units", "--plan", "p", "--post-image", "--post-image"])
            .contains("more than once")
    );
    for other in ["-j", "--from", "--declared", "--worker-jobs"] {
        let line = usage_error(&["units", "--plan", "p", other, "1"]);
        assert!(line.contains("unknown option"), "{other}: {line}");
    }
    // `--post-image` is no option of the other commands.
    assert!(usage_error(&["parse", "--db", "c", "--post-image"]).contains("unknown option"));
}

/// `spec/edits.md` §10.1 (`docs/wp/WP-139.md` item 4): `--dependent`
/// takes `offer` or `refuse`, and nothing else, as the usage error says.
/// No command of this version takes the option (`map` and `rename` are
/// `docs/wp/WP-076.md`'s); its value is parsed as theirs will be.
#[test]
fn s10_1_dependent_values() {
    use slip_model::transform::Dependent;
    assert_eq!(args::parse_dependent("offer".as_ref()), Ok(Dependent::Offer));
    assert_eq!(args::parse_dependent("refuse".as_ref()), Ok(Dependent::Refuse));
    for bad in ["", "Offer", "refused", "offer "] {
        let e = args::parse_dependent(bad.as_ref()).unwrap_err();
        assert!(e.0.contains("is not `offer` or `refuse`"), "{bad:?}: {e:?}");
    }
}
