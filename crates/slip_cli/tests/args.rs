//! `spec/exec.md` §8: the command line, parsed without starting a process.

use std::ffi::OsString;
use std::path::PathBuf;

use slip_cli::args::{
    self, DEFAULT_STORE, DEFAULT_WORKER_JOBS, DEFAULT_WORKER_STACK, Db, Invocation, ParseArgs,
    PlanArgs, PlanSource, UsageError, WorkerArgs, parse_size,
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
    };
    assert_eq!(separate, expected);
    assert_eq!(joined, expected);

    let defaults = parse_args(&["parse", "--db", "c.json"]);
    assert_eq!(
        defaults,
        ParseArgs {
            source: PlanSource::Db(vec![db(None, "c.json")]),
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
    );

    assert_eq!(
        parse(&["__worker", "--worker-jobs", "3", "--worker-stack=16M"]),
        Ok(Invocation::Worker(WorkerArgs { worker_jobs: 3, worker_stack: 16 << 20 }))
    );
    assert_eq!(
        parse(&["__worker"]),
        Ok(Invocation::Worker(WorkerArgs {
            worker_jobs: DEFAULT_WORKER_JOBS,
            worker_stack: DEFAULT_WORKER_STACK
        }))
    );
}

// §8: a value is the next argument even when it starts with `-`; `--`
// ends the options.
#[test]
fn s8_values_and_double_dash() {
    let a = parse_args(&["parse", "--db", "c.json", "--only", "-fno"]);
    assert_eq!(a.only, vec![b"-fno".to_vec()]);
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
    assert_eq!(a.only, vec![b"a".to_vec(), b"b".to_vec()]);
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
    assert_eq!((a.memory, a.observer_timeout), (Some(0), Some(0)));
    assert_eq!(parse_args(&["parse", "--db", "c.json", "--limit", "0"]).limit, Some(0));
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
    assert_eq!(a.store, PathBuf::from(raw(b"st\xff")));
    assert_eq!(a.only, vec![b"\xfe".to_vec()]);
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
        "slip --version",
        "slip --help",
        "--worker-stack",
    ] {
        assert!(args::USAGE.contains(word), "{word}");
    }
    assert!(!args::USAGE.contains("__worker") && !args::USAGE.contains("__panic"));
}
