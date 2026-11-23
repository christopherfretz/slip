//! WP-112 item 6: is the names report usable on a real relocation?
//!
//! `measure` runs, in process and one observer at a time, what
//! `spec/harness.md` §10.2 runs for one case of `bench/demo1/`: the plan of
//! the corpus's compilation database, the map task over the selection, the
//! case's tree (a tree of hard links to the corpus's roots, the case's
//! `base` and then its `edit.txt` applied, each edited file written as a
//! new file), its `declared.txt` resolved as `slip declare` resolves it
//! (`slip_cli::declare` over the records of the case's `observers`), and
//! the verify task over the selection. It counts what the item asks:
//! the units with a names entry, the notes, and the verdicts beside them.
//!
//! `slip verify` (WP-053) and `cargo xtask demo` are not on this branch's
//! `main`, so this file is the one command. It is ignored by default: it
//! reads the LLVM corpus (`$SLIP_CORPUS_DIR/llvm`, made by `cargo xtask
//! corpus`) and parses each selected translation unit three to five times.
//! Run it with:
//!
//! ```text
//! cargo test -p slip_cli --test names_relocation -- --ignored --nocapture
//! ```
//!
//! (`SLIP_NAMES_CASES` picks the cases, default `01-relocate`, comma
//! separated.) `measure` itself is run on every `cargo test` over a corpus
//! made here, so that the command is known to work before it meets LLVM.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use slip_cli::declare::{finish, needs, read, resolve};
use slip_cli::image::{Image, RootMap, Side};
use slip_cli::verify_job::{Params, verify};
use slip_cli::worker::MapTask;
use slip_digest::ReadOnly;
use slip_digest::records::{Record, records};
use slip_exec::store::{Access, Store};
use slip_exec::worker::{Job, Task};
use slip_model::declared::Declared;
use slip_model::formats::content_id;
use slip_model::keys::RuleTable;
use slip_model::transform::Ownership;
use slip_model::verdict::VerdictKind;
use slip_model::{Digest, Observer, Phase, Plan, Status};

/// What one case gives.
#[derive(Debug, Default, PartialEq, Eq)]
struct Counts {
    /// Observers selected, and those whose map shard is `ok`.
    observers: usize,
    mapped: usize,
    /// Names entries summed over the map shards, and distinct keys.
    entries: usize,
    keys_with_entry: usize,
    /// Notes summed over the verify shards, and distinct keys (what the
    /// report counts, `spec/verify.md` §7.1).
    notes: usize,
    noted_keys: usize,
    /// Verify shards by verdict name.
    verdicts: BTreeMap<String, usize>,
    /// The noted units' names, each once, sorted.
    names: Vec<String>,
    /// The findings of the verify shards (`<kind> <name>`), each once,
    /// sorted, and the reasons of `unverifiable` verdicts.
    findings: Vec<String>,
}

/// `demo.toml`'s three keys (read as the harness's line reader reads them:
/// `key = "v"` or `key = ["a", "b"]`).
struct Demo {
    corpus: String,
    roots: Vec<String>,
    only: Vec<String>,
}

fn strings(v: &str) -> Vec<String> {
    v.trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .split(',')
        .map(|s| s.trim().trim_matches('"').to_owned())
        .filter(|s| !s.is_empty())
        .collect()
}

fn demo(text: &str) -> Demo {
    let mut d = Demo { corpus: String::new(), roots: Vec::new(), only: Vec::new() };
    for line in text.lines().map(str::trim).filter(|l| !l.starts_with('#')) {
        let Some((k, v)) = line.split_once('=') else { continue };
        match k.trim() {
            "corpus" => v.trim().trim_matches('"').clone_into(&mut d.corpus),
            "roots" => d.roots = strings(v),
            "only" => d.only = strings(v),
            _ => {}
        }
    }
    d
}

/// A tree of hard links to `from` at `to` (directories made, files linked).
/// A file that cannot be linked is an error: a copy of a corpus's root is
/// not what this command should make by accident.
fn link_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap_or_else(|e| panic!("{}: {e}", from.display())) {
        let e = e.unwrap();
        let (src, dst) = (e.path(), to.join(e.file_name()));
        let t = e.file_type().unwrap();
        if t.is_dir() {
            link_tree(&src, &dst);
        } else if t.is_file() {
            std::fs::hard_link(&src, &dst)
                .unwrap_or_else(|e| panic!("cannot link {}: {e}", src.display()));
        } else if t.is_symlink() {
            std::os::unix::fs::symlink(std::fs::read_link(&src).unwrap(), &dst).unwrap();
        }
    }
}

/// Applies `edit.txt` (`spec/harness.md` §10.1) to the tree `tree` (paths
/// relative to the corpus's `src/`), each edited file written as a new
/// file.
fn apply_edit(tree: &Path, text: &str) {
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    let mut file: Option<PathBuf> = None;
    let mut i = 0;
    let mut entries = 0;
    while i < lines.len() {
        let line = lines[i].trim_end_matches('\n');
        i += 1;
        if let Some(p) = line.strip_prefix("file ") {
            file = Some(tree.join(p.trim()));
            assert!(file.as_ref().unwrap().is_file(), "edit.txt {i}: no file {p}");
        } else if line == "replace" || line == "replace all" {
            let all = line == "replace all";
            let (mut old, mut new) = (String::new(), String::new());
            while lines[i].trim_end_matches('\n') != "with" {
                old.push_str(lines[i]);
                i += 1;
            }
            i += 1;
            while lines[i].trim_end_matches('\n') != "end" {
                new.push_str(lines[i]);
                i += 1;
            }
            i += 1;
            let path =
                file.as_ref().unwrap_or_else(|| panic!("edit.txt {i}: replace without file"));
            let text = std::fs::read_to_string(path).unwrap();
            let n = text.matches(&old).count();
            assert!(n == 1 || (all && n > 0), "edit.txt {i}: old text found {n} times");
            let out = if all { text.replace(&old, &new) } else { text.replacen(&old, &new, 1) };
            std::fs::remove_file(path).unwrap();
            std::fs::write(path, out).unwrap();
            entries += 1;
        } else if let Some(p) = line.strip_prefix("create ") {
            let path = tree.join(p.trim());
            assert!(!path.exists(), "edit.txt {i}: {p} exists");
            let mut body = String::new();
            while lines[i].trim_end_matches('\n') != "end" {
                body.push_str(lines[i]);
                i += 1;
            }
            i += 1;
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, body).unwrap();
            entries += 1;
        } else {
            assert!(line.trim().is_empty(), "edit.txt {i}: `{line}`");
        }
    }
    assert!(entries > 0, "edit.txt has no entry");
}

fn bytes(p: &Path) -> Vec<u8> {
    p.to_str().unwrap().as_bytes().to_vec()
}

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    needle.is_empty() || hay.windows(needle.len()).any(|w| w == needle)
}

/// The plan of `db`, published in `store` by `slip plan`.
fn plan(db: &Path, store: &Path) -> Plan {
    let args: Vec<std::ffi::OsString> = ["plan", "--db", db.to_str().unwrap(), "--store"]
        .iter()
        .map(Into::into)
        .chain([store.as_os_str().to_owned(), "-j".into(), "1".into()])
        .collect();
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let _ = slip_cli::run(&args, &mut out, &mut err);
    // `run` replaces the panic report with its own line: put Rust's back.
    drop(std::panic::take_hook());
    let out = String::from_utf8(out).unwrap();
    let id = out
        .lines()
        .find_map(|l| l.strip_prefix("plan "))
        .unwrap_or_else(|| panic!("slip plan: {out}{}", String::from_utf8_lossy(&err)));
    let store = Store::open(store, Access::Shared).unwrap();
    let ids = store.find_plan(id).unwrap();
    Plan::from_bytes(&store.read_plan(&ids[0]).unwrap().unwrap()).unwrap()
}

const PLAN: Digest = Digest([7; 32]);

fn job(store: &Store, o: &Observer, phase: Phase, round: u64) -> Job {
    Job {
        store: bytes(store.root()),
        plan: PLAN,
        phase,
        round,
        observer: o.clone(),
        transform: None,
        post: None,
    }
}

/// The records of `o`'s image `side` under `rules`.
fn records_of(image: &Image, o: &Observer, side: Side, rules: &RuleTable) -> Vec<Record> {
    let (shard, r) =
        image.parse(o, side, Phase::Verify, 1, |i| records(i, rules, ReadOnly::NO_FILE)).unwrap();
    assert_eq!(shard.outcome.status, Status::Ok, "{:?}", shard.outcome);
    r.unwrap().unwrap()
}

/// `slip declare`'s resolution of `text` over the `declarers`' images.
fn declare(text: &str, image: &Image, declarers: &[&Observer]) -> Declared {
    let lines = read(text).unwrap_or_else(|e| panic!("declared.txt: {e}"));
    let n = needs(&lines);
    let empty = RuleTable::empty();
    let all = |side, rules: &RuleTable| -> Vec<Record> {
        declarers.iter().flat_map(|o| records_of(image, o, side, rules)).collect()
    };
    let pre = all(Side::Pre, &empty);
    let post = if n.post { all(Side::Post, &empty) } else { Vec::new() };
    let resolved = resolve(&lines, &pre, &post).unwrap_or_else(|e| panic!("declared.txt: {e}"));
    let mapped = if n.post_mapped { all(Side::Post, &resolved.rules) } else { Vec::new() };
    finish(&lines, resolved, &mapped).unwrap_or_else(|e| panic!("declared.txt: {e}"))
}

/// The case's tree at `tree` (hard links to the roots of `src`, its
/// base's edits and then its own applied) and the image mapping each root
/// to it.
fn case_image(demo_dir: &Path, d: &Demo, case_dir: &Path, src: &Path, tree: &Path) -> Image {
    for root in &d.roots {
        link_tree(&src.join(root), &tree.join(root));
    }
    let mut chain = vec![case_dir.to_path_buf()];
    while let Ok(b) = std::fs::read_to_string(chain.last().unwrap().join("base")) {
        chain.push(demo_dir.join("cases").join(b.trim()));
    }
    for dir in chain.iter().rev() {
        if let Ok(e) = std::fs::read_to_string(dir.join("edit.txt")) {
            apply_edit(tree, &e);
        }
    }
    let maps = d
        .roots
        .iter()
        .map(|r| RootMap { subject: bytes(&src.join(r)), post: bytes(&tree.join(r)) })
        .collect();
    Image::new(maps).unwrap()
}

/// One case of a demo directory over the corpus at `corpus` (`src/` and
/// `build/compile_commands.json`), with its work in `work` (removed first).
fn measure(demo_dir: &Path, case: &str, corpus: &Path, work: &Path) -> Counts {
    let d = demo(&std::fs::read_to_string(demo_dir.join("demo.toml")).unwrap());
    let src = corpus.join("src");
    let _ = std::fs::remove_dir_all(work);
    std::fs::create_dir_all(work).unwrap();
    let store_dir = work.join("store");
    let plan = plan(&corpus.join("build").join("compile_commands.json"), &store_dir);
    let store = Store::open(&store_dir, Access::Shared).unwrap();
    let selected: Vec<&Observer> = plan
        .observers
        .iter()
        .filter(|o| d.only.iter().any(|s| contains(&o.main.path, s.as_bytes())))
        .collect();
    let mut c = Counts { observers: selected.len(), ..Counts::default() };

    // The map run.
    let mut keys = std::collections::BTreeSet::new();
    let mut ok: Vec<&Observer> = Vec::new();
    for o in &selected {
        let j = job(&store, o, Phase::Map, 0);
        let shard = MapTask::new().run(&j).unwrap();
        store.publish_shard(&PLAN, Phase::Map, 0, &o.id, &shard.to_bytes()).unwrap();
        if shard.outcome.status == Status::Ok {
            c.mapped += 1;
            let names = shard.names.as_ref().expect("an ok map shard has names");
            c.entries += names.units.len();
            keys.extend(names.units.iter().map(|e| e.key));
            ok.push(*o);
        }
    }
    c.keys_with_entry = keys.len();

    let case_dir = demo_dir.join("cases").join(case);
    let image = case_image(demo_dir, &d, &case_dir, &src, &work.join("tree"));

    // The declared changes, resolved in the case's observers.
    let declared = match std::fs::read_to_string(case_dir.join("declared.txt")) {
        Ok(text) if text.lines().any(|l| !l.trim().is_empty()) => {
            let subs: Vec<String> = std::fs::read_to_string(case_dir.join("observers"))
                .map(|t| {
                    t.lines().map(str::trim).filter(|l| !l.is_empty()).map(String::from).collect()
                })
                .unwrap_or_default();
            let declarers: Vec<&Observer> = if subs.is_empty() {
                ok.iter().take(1).copied().collect()
            } else {
                ok.iter()
                    .filter(|o| subs.iter().any(|s| contains(&o.main.path, s.as_bytes())))
                    .copied()
                    .collect()
            };
            declare(&text, &image, &declarers)
        }
        _ => Declared::none(),
    };
    // The run's files (`spec/edits.md` §2.1): the roots owned, as the
    // demo's subject is; nothing read-only.
    let declared = if declared.rules().is_empty() {
        declared
    } else {
        let owned = d.roots.iter().map(|r| bytes(&src.join(r))).collect();
        declared.with_files(Some(Ownership { owned, readonly: Vec::new() }))
    };
    let declared_id = match declared.validate() {
        Ok(()) => content_id(&declared.to_bytes()).unwrap(),
        Err(_) => Digest([0; 32]),
    };
    println!("  declared rules {}", declared.rules().rules().len());
    let params = Params { pre: 0, declared, declared_id, image, limit: 100_000, locate: 64 };

    // The verify run.
    let mut noted = BTreeMap::new();
    for o in &selected {
        let v = verify(&store, &job(&store, o, Phase::Verify, 1), &params).unwrap();
        let kind = v.verdict.as_ref().map_or("none", |x| match x.kind {
            VerdictKind::Verified => "verified",
            VerdictKind::Mismatch => "mismatch",
            VerdictKind::Unverifiable => "unverifiable",
        });
        *c.verdicts.entry(kind.to_owned()).or_default() += 1;
        if let Some(x) = &v.verdict {
            c.findings.extend(x.findings.iter().map(|f| format!("{:?} {}", f.kind, f.name)));
            if !x.reason.is_empty() {
                c.findings.push(format!("reason {}", x.reason));
            }
        }
        if let Some(n) = &v.names {
            c.notes += n.notes.len();
            for note in &n.notes {
                noted.entry(note.key).or_insert_with(|| note.name.clone());
            }
        }
    }
    c.findings.sort();
    c.findings.dedup();
    c.noted_keys = noted.len();
    let mut names: Vec<String> = noted.into_values().collect();
    names.sort();
    c.names = names;
    c
}

fn print(case: &str, c: &Counts) {
    println!("case {case}");
    println!("  observers {} selected, {} mapped ok", c.observers, c.mapped);
    println!(
        "  names entries {} over the map shards, {} distinct keys",
        c.entries, c.keys_with_entry
    );
    println!("  notes {} over the verify shards, {} distinct keys", c.notes, c.noted_keys);
    println!("  verdicts {:?}", c.verdicts);
    for n in &c.names {
        println!("  note {n}");
    }
    for f in c.findings.iter().take(50) {
        println!("  finding {f}");
    }
}

/// The command of item 6 (see the module's comment).
#[test]
#[ignore = "reads the LLVM corpus; run by hand (WP-112 item 6)"]
fn demo1_names_report() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let demo_dir = root.join("bench/demo1");
    let d = demo(&std::fs::read_to_string(demo_dir.join("demo.toml")).unwrap());
    let corpora =
        std::env::var_os("SLIP_CORPUS_DIR").map_or_else(|| root.join(".corpus"), PathBuf::from);
    let corpus = corpora.join(&d.corpus);
    let cases = std::env::var("SLIP_NAMES_CASES").unwrap_or_else(|_| "01-relocate".to_owned());
    for case in cases.split(',').map(str::trim).filter(|c| !c.is_empty()) {
        let work = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("wp112-demo1-{case}"));
        let c = measure(&demo_dir, case, &corpus, &work);
        print(case, &c);
        let _ = std::fs::remove_dir_all(&work);
    }
}

/// `measure` over a corpus made here: a namespace relocated in a header,
/// declared; one function of it passes its pretty name on (a note, the
/// declared change), one outside it takes a parameter of the moved
/// namespace's type (a note: its string spells the new name), and one
/// passes on a string that does not change (no note).
#[test]
fn measure_on_a_small_corpus() {
    let base = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("wp112-small-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let corpus = base.join("corpus");
    let src = corpus.join("src");
    let write = |p: &Path, t: &str| {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, t).unwrap();
    };
    write(
        &src.join("p/include/h.h"),
        "void take(const char *);\nnamespace lib { namespace detail {\nstruct V {};\n\
         inline void f(V) { take(__PRETTY_FUNCTION__); }\n} }\n\
         inline void g(lib::detail::V) { take(__PRETTY_FUNCTION__); }\n\
         inline void k() { take(__func__); }\n",
    );
    write(
        &src.join("p/lib/a.cpp"),
        "#include \"h.h\"\nvoid use() { f(lib::detail::V()); g({}); k(); }\n",
    );
    write(&src.join("p/lib/b.cpp"), "#include \"h.h\"\nvoid other() { k(); }\n");
    let dir = src.join("p/lib");
    let db = format!(
        "[{}]",
        ["a.cpp", "b.cpp"]
            .iter()
            .map(|f| format!(
                "{{\"directory\": \"{d}\", \"file\": \"{d}/{f}\", \"arguments\": [\"c++\", \"-std=c++20\", \"-I{i}\", \"-c\", \"{d}/{f}\"]}}",
                d = dir.display(),
                i = src.join("p/include").display()
            ))
            .collect::<Vec<_>>()
            .join(",")
    );
    write(&corpus.join("build/compile_commands.json"), &db);
    let demo_dir = base.join("demo");
    write(
        &demo_dir.join("demo.toml"),
        "corpus = \"small\"\nroots = [\"p\"]\nonly = [\"/p/lib/\"]\n",
    );
    let case = demo_dir.join("cases/01-relocate");
    write(
        &case.join("edit.txt"),
        "file p/include/h.h\nreplace\nnamespace lib { namespace detail {\nwith\nnamespace lib { namespace impl {\nend\n\
         replace\ninline void g(lib::detail::V) { take(__PRETTY_FUNCTION__); }\nwith\ninline void g(lib::impl::V) { take(__PRETTY_FUNCTION__); }\nend\n\
         file p/lib/a.cpp\nreplace\nvoid use() { f(lib::detail::V()); g({}); k(); }\nwith\nvoid use() { f(lib::impl::V()); g({}); k(); }\nend\n",
    );
    write(&case.join("declared.txt"), "relocate lib::detail => lib :: impl\n");
    write(&case.join("observers"), "/p/lib/a.cpp\n");

    let c = measure(&demo_dir, "01-relocate", &corpus, &base.join("work"));
    print("small", &c);
    assert_eq!((c.observers, c.mapped), (2, 2));
    // a.cpp: f, g, k; b.cpp: k.
    // Both translation units include the header: f, g and k in each.
    assert_eq!((c.entries, c.keys_with_entry), (6, 3));
    assert_eq!(c.verdicts, BTreeMap::from([("verified".to_owned(), 2)]));
    assert_eq!(c.noted_keys, 2, "{:?}", c.names);
    // Each translation unit notes both (the header's inline functions are
    // units of both); the report counts each key once.
    assert_eq!(c.notes, 4);
    assert!(c.names.iter().any(|n| n.starts_with("g(")), "{:?}", c.names);
    let _ = std::fs::remove_dir_all(&base);
}
