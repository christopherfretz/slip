//! WP-118 (`spec/landed/verify/WP-118.md` §8, "`crates/slip_cli/tests/`"):
//! the third class of file, **rebuilt**, through the verify task and the
//! run's report with real parses (T-cli-1, 2, 3, 5, 6), and through the
//! executable (T-cli-3 (a), T-cli-4, T-cli-7).
//!
//! The verify-task tests lay out, under one directory of their own: the
//! subject `s/` (owned), a build directory `b/` (said nothing about unless
//! a test declares a file of it), their post trees `ps/` and `pb/`, other
//! directories as a test needs them (`d/`, `opt/`), and a store. Rules are
//! `relocate old => :: fresh` (the rule `fresh -> old`, kind namespace)
//! unless a test says otherwise.
//!
//! A parse changes the process's current directory: every test that
//! parses holds [`LOCK`], and every path is absolute.

#![cfg(target_os = "linux")]

mod common;

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command as Process, ExitStatus, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use slip_cli::image::{Image, RootMap};
use slip_cli::verify_job::{Params, verify};
use slip_cli::worker::MapTask;
use slip_exec::store::{Access, Object, Store};
use slip_exec::worker::{Job, Task};
use slip_model::declared::Declared;
use slip_model::formats::content_id;
use slip_model::hash::content_digest;
use slip_model::keys::{KeyKind, Name, Parent, Rule, RuleTable};
use slip_model::overlay::{Content, Overlay, OverlayFile};
use slip_model::transform::Ownership;
use slip_model::verdict::VerdictKind;
use slip_model::{Command, Digest, FileRef, Language, Observer, Origin, Phase, Shard};
use slip_verify::run::{Header, MapShard, Report, RunVerdict};

static LOCK: Mutex<()> = Mutex::new(());

const PLAN: Digest = Digest([7; 32]);

fn bytes(p: &Path) -> Vec<u8> {
    p.to_str().unwrap().as_bytes().to_vec()
}

fn write(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

/// The trees of one test, removed on drop; the lock is held while it
/// lives.
struct Tree {
    base: PathBuf,
    store: Store,
    _lock: MutexGuard<'static, ()>,
}

impl Drop for Tree {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.base);
    }
}

/// How the post-image is given.
enum Form<'a> {
    /// `s -> ps` and `b -> pb`.
    TwoRoots,
    /// `s -> ps` only: `b/` is the same in both images.
    SubjectRoot,
    /// An overlay of these files (relative paths), whose post bytes are
    /// those the test wrote under `ps/`, `pb/` (or, for a file of
    /// another directory, the subject's bytes).
    Overlay(&'a [&'a str]),
}

impl Tree {
    /// Each `(rel, text)` of `pre` is written at `rel` (`s/…`, `b/…`,
    /// `d/…`, `opt/…`); one under `s/` or `b/` is also written to its post
    /// tree (`ps/…`, `pb/…`), and then each of `post` (`s/…` or `b/…`, its
    /// post tree's counterpart written). `{base}` in a text is replaced by
    /// the tree's directory.
    fn new(name: &str, pre: &[(&str, &str)], post: &[(&str, &str)]) -> Tree {
        static N: AtomicUsize = AtomicUsize::new(0);
        let lock = LOCK.lock().unwrap_or_else(PoisonError::into_inner);
        let n = N.fetch_add(1, Ordering::SeqCst);
        let base = common::work_root().join(format!("wp118-{name}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        let base = std::fs::canonicalize(&base).unwrap();
        for d in ["s", "b", "ps", "pb"] {
            std::fs::create_dir_all(base.join(d)).unwrap();
        }
        let t = Tree {
            store: Store::open(&base.join("store"), Access::Shared).unwrap(),
            base,
            _lock: lock,
        };
        let base = t.base.to_str().unwrap().to_owned();
        for (rel, text) in pre {
            let text = text.replace("{base}", &base);
            write(&t.path(rel), &text);
            if let Some(p) = t.post_of(rel) {
                write(&p, &text);
            }
        }
        for (rel, text) in post {
            let text = text.replace("{base}", &base);
            write(&t.post_of(rel).expect("a post file under s/ or b/"), &text);
        }
        t
    }

    fn path(&self, rel: &str) -> PathBuf {
        self.base.join(rel)
    }

    fn b(&self, rel: &str) -> Vec<u8> {
        bytes(&self.path(rel))
    }

    fn post_of(&self, rel: &str) -> Option<PathBuf> {
        if let Some(r) = rel.strip_prefix("s/") {
            Some(self.base.join("ps").join(r))
        } else {
            rel.strip_prefix("b/").map(|r| self.base.join("pb").join(r))
        }
    }

    /// The observer of `main` with `flags` (`{base}` replaced by the
    /// base directory), its command's `directory` being `directory`.
    fn observer(&self, main: &str, flags: &[&str], directory: &str) -> Observer {
        let base = self.base.to_str().unwrap();
        let mut argv: Vec<Vec<u8>> = vec![b"c++".to_vec(), b"-std=c++20".to_vec()];
        argv.extend(flags.iter().map(|f| f.replace("{base}", base).into_bytes()));
        argv.push(self.b(main));
        let command = Command { directory: self.b(directory), argv, env: Vec::new() };
        Observer {
            id: command.observer_id(),
            command,
            language: Language::Cxx,
            main: FileRef { path: self.b(main), size: 0, content: Digest([0; 32]) },
            cost_hint: 1,
            origins: vec![Origin { configuration: 0, entry: 0 }],
        }
    }

    fn job(&self, observer: &Observer, phase: Phase, round: u64) -> Job {
        Job {
            store: bytes(self.store.root()),
            plan: PLAN,
            phase,
            round,
            observer: observer.clone(),
            transform: None,
            post: None,
        }
    }

    /// The map shard of `observer`, made by the map task and published.
    fn map(&self, observer: &Observer) {
        let job = self.job(observer, Phase::Map, 0);
        let shard = MapTask::new().run(&job).unwrap();
        assert_eq!(shard.outcome.status, slip_model::Status::Ok, "{:?}", shard.outcome);
        self.store.publish_shard(&PLAN, Phase::Map, 0, &observer.id, &shard.to_bytes()).unwrap();
    }

    fn image(&self, form: &Form<'_>) -> Image {
        let map = |s: &str, p: &str| RootMap { subject: self.b(s), post: self.b(p) };
        match form {
            Form::TwoRoots => Image::new(vec![map("s", "ps"), map("b", "pb")]).unwrap(),
            Form::SubjectRoot => Image::new(vec![map("s", "ps")]).unwrap(),
            Form::Overlay(files) => {
                let mut entries: Vec<OverlayFile> = files
                    .iter()
                    .map(|rel| {
                        let pre = std::fs::read(self.path(rel)).unwrap();
                        let post = self.post_of(rel).map_or_else(
                            || pre.clone(),
                            |p| std::fs::read(p).unwrap_or_else(|_| pre.clone()),
                        );
                        self.store.publish_blob(&post).unwrap();
                        OverlayFile { path: self.b(rel), pre: content(&pre), post: content(&post) }
                    })
                    .collect();
                entries.sort_by(|a, b| a.path.cmp(&b.path));
                let overlay =
                    Overlay { edits: Digest([0x0e; 32]), files: entries, spellings: Vec::new() };
                overlay.validate().unwrap();
                let id = self.store.publish_object(Object::Overlay, &overlay.to_bytes()).unwrap();
                Image::overlay(id, Arc::new(overlay))
            }
        }
    }

    /// The run's files: `s` owned, and the lists given (relative paths).
    fn files(&self, readonly: &[&str], rebuilt: &[&str]) -> Ownership {
        let list = |l: &[&str]| {
            let mut v: Vec<Vec<u8>> = l.iter().map(|r| self.b(r)).collect();
            v.sort();
            v
        };
        Ownership { owned: vec![self.b("s")], readonly: list(readonly), rebuilt: list(rebuilt) }
    }

    fn params(&self, declared: Declared, form: &Form<'_>) -> Params {
        let declared_id = content_id(&declared.to_bytes()).unwrap();
        Params { pre: 0, declared, declared_id, image: self.image(form), limit: 64, locate: 16 }
    }

    /// The verify task's shard of `observer`, round 1, validated.
    fn verify(&self, observer: &Observer, params: &Params) -> Shard {
        let shard = verify(&self.store, &self.job(observer, Phase::Verify, 1), params).unwrap();
        shard.validate().unwrap();
        shard
    }

    fn stored_map(&self, observer: &Observer) -> MapShard {
        let bytes = self.store.read_shard(&PLAN, Phase::Map, 0, &observer.id).unwrap().unwrap();
        let shard = Shard::from_bytes(&bytes).unwrap();
        MapShard::Read { status: shard.outcome.status, id: content_id(&bytes).unwrap() }
    }
}

fn content(b: &[u8]) -> Content {
    Content { size: b.len() as u64, content: content_digest(b) }
}

fn ident(s: &str) -> Name {
    Name::Identifier(s.as_bytes().to_vec())
}

/// `relocate old => :: fresh`: the rule `fresh -> old`, kind namespace.
fn old_to_fresh() -> Declared {
    let rule = Rule {
        parent: Parent::Root,
        kind: KeyKind::Namespace,
        name: ident("fresh"),
        disc: None,
        to_parent: Parent::Root,
        to_name: ident("old"),
    };
    Declared::new(vec![rule], vec![], vec![], vec![]).unwrap()
}

/// The declared changes of `slip declare` text `text`, resolved over the
/// records of the one-file snippets `pre` and `post` (every file owned
/// there): for `kept` lines and per-entity rules, whose keys the snippet
/// gives.
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
    let (r, errors) = slip_kiln::snippet::parse_with(&snippet, &extra, |p| {
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

/// What a run said: per observer its verdict kind and reason, the run's
/// verdict, and the report's text and JSON.
struct Ran {
    kinds: Vec<(VerdictKind, String)>,
    verdict: RunVerdict,
    text: String,
    json: String,
}

/// Maps every observer of `observers` (`(main, flags, directory)`), then
/// verifies each with `declared` (its files set) through `form`, and folds
/// the run's report.
fn run(
    t: &Tree,
    declared: Declared,
    files: Ownership,
    form: &Form<'_>,
    observers: &[(&str, &[&str], &str)],
) -> Ran {
    let params = t.params(declared.with_files(Some(files)), form);
    let header = Header {
        plan: PLAN,
        round: 1,
        declared: params.declared_id,
        rules: params.declared.rules_id(),
        image: match form {
            Form::Overlay(_) => Vec::new(),
            _ => params.image.maps().iter().map(|m| (m.subject.clone(), m.post.clone())).collect(),
        },
        limit: params.limit as u64,
        locate: params.locate as u64,
        plan_observers: observers.len() as u64,
        run: b"runs/0001.run".to_vec(),
    };
    let mut report = Report::new(header, &params.declared);
    if let Some((id, o)) = params.image.overlay_of() {
        report.overlay(id, o);
    }
    let mut kinds = Vec::new();
    let os: Vec<Observer> = observers.iter().map(|(m, f, d)| t.observer(m, f, d)).collect();
    for o in &os {
        t.map(o);
    }
    for (i, o) in os.iter().enumerate() {
        let v = t.verify(o, &params);
        let verdict = v.verdict.as_ref().expect("a verdict");
        kinds.push((verdict.kind, verdict.reason.clone()));
        report.add_at(i as u64, &o.id, &o.main.path, Some(&v), Some(t.stored_map(o)));
        report.directory_at(i as u64, &o.command.directory);
    }
    let json = slip_model::json::write_compact(&report.to_json()).unwrap();
    Ran { kinds, verdict: report.verdict(), text: report.to_text(), json }
}

fn lines_with<'a>(text: &'a str, prefix: &str) -> Vec<&'a str> {
    text.lines().filter(|l| l.starts_with(prefix)).collect()
}

const LIB_PRE: &str = "#pragma once\nnamespace old { int f(); }\n";
const LIB_POST: &str = "#pragma once\nnamespace fresh { int f(); }\n";
const GEN_PRE: &str = "#pragma once\nnamespace old { struct G {}; }\n";
const GEN_POST: &str = "#pragma once\nnamespace fresh { struct G {}; }\n";
const MAIN_PRE: &str =
    "#include \"lib.h\"\n#include \"gen.h\"\nint main() { old::G g; (void)g; return old::f(); }\n";
const MAIN_POST: &str = "#include \"lib.h\"\n#include \"gen.h\"\nint main() { fresh::G g; (void)g; return fresh::f(); }\n";
const MAIN_POST_G_LEFT: &str = "#include \"lib.h\"\n#include \"gen.h\"\nint main() { old::G g; (void)g; return fresh::f(); }\n";

const INCLUDE_B: &[&str] = &["-I{base}/b"];

/// T-cli-1 (R6.1): the tree form end to end, a generated header in a build
/// directory given as a second pair of roots. Declared rebuilt: `verified`
/// and the `rebuilt … relied on` line; left out: a `mismatch`, no such
/// line, and no hint (a read-only file that only declares is not hinted).
#[test]
fn t_cli_1_tree_form_with_a_build_root() {
    let pre = [("s/lib.h", LIB_PRE), ("s/main.cpp", MAIN_PRE), ("b/gen.h", GEN_PRE)];
    let post = [("s/lib.h", LIB_POST), ("s/main.cpp", MAIN_POST), ("b/gen.h", GEN_POST)];
    let observers: &[(&str, &[&str], &str)] = &[("s/main.cpp", INCLUDE_B, "s")];

    let t = Tree::new("t1", &pre, &post);
    let ran = run(&t, old_to_fresh(), t.files(&[], &["b/gen.h"]), &Form::TwoRoots, observers);
    assert_eq!(ran.kinds[0].0, VerdictKind::Verified, "{:?}\n{}", ran.kinds, ran.text);
    assert_eq!(ran.verdict, RunVerdict::Verified, "{}", ran.text);
    let genp = t.path("b/gen.h").display().to_string();
    assert_eq!(
        lines_with(&ran.text, "rebuilt "),
        [format!("rebuilt {genp} relied on by 1 observer(s)")],
        "{}",
        ran.text
    );
    assert!(ran.json.contains("\"rebuilt\":"), "{}", ran.json);
    drop(t);

    let t = Tree::new("t1-undeclared", &pre, &post);
    let ran = run(&t, old_to_fresh(), t.files(&[], &[]), &Form::TwoRoots, observers);
    assert_eq!(ran.kinds[0].0, VerdictKind::Mismatch, "{:?}\n{}", ran.kinds, ran.text);
    assert!(lines_with(&ran.text, "rebuilt").is_empty(), "{}", ran.text);
    assert!(lines_with(&ran.text, "hint:").is_empty(), "{}", ran.text);
    // P0: no list and no hint, no `"rebuilt"` member.
    assert!(!ran.json.contains("\"rebuilt\""), "{}", ran.json);
}

/// T-cli-2 (R6.1, its two failures): the build did not regenerate the file
/// (stale, with the `rebuilt file … still declares` line), and the build
/// regenerated it with another change (a difference).
#[test]
fn t_cli_2_not_regenerated_and_regenerated_differently() {
    let pre = [("s/lib.h", LIB_PRE), ("s/main.cpp", MAIN_PRE), ("b/gen.h", GEN_PRE)];
    let observers: &[(&str, &[&str], &str)] = &[("s/main.cpp", INCLUDE_B, "s")];

    let post = [("s/lib.h", LIB_POST), ("s/main.cpp", MAIN_POST_G_LEFT)];
    let t = Tree::new("t2-stale", &pre, &post);
    let ran = run(&t, old_to_fresh(), t.files(&[], &["b/gen.h"]), &Form::TwoRoots, observers);
    assert_ne!(ran.verdict, RunVerdict::Verified, "{}", ran.text);
    assert_eq!(ran.kinds[0].0, VerdictKind::Verified, "{:?}", ran.kinds);
    assert!(lines_with(&ran.text, "stale rule 0").len() == 1, "{}", ran.text);
    let genp = t.path("b/gen.h").display().to_string();
    let file = lines_with(&ran.text, "rebuilt file ");
    assert_eq!(file.len(), 1, "{}", ran.text);
    assert!(
        file[0].starts_with(&format!("rebuilt file {genp} still declares "))
            && file[0].ends_with(" under the name rule 0 says was left (not regenerated)"),
        "{}",
        ran.text
    );
    drop(t);

    let other = "#pragma once\nnamespace fresh { struct G { int x; }; }\n";
    let post = [("s/lib.h", LIB_POST), ("s/main.cpp", MAIN_POST), ("b/gen.h", other)];
    let t = Tree::new("t2-differs", &pre, &post);
    let ran = run(&t, old_to_fresh(), t.files(&[], &["b/gen.h"]), &Form::TwoRoots, observers);
    assert_eq!(ran.kinds[0].0, VerdictKind::Mismatch, "{:?}\n{}", ran.kinds, ran.text);
}

/// T-cli-3 (b), A8: the overlay form, a frozen rebuilt file that still
/// declares `old::G` after the run's members of `old` moved: `unverifiable`
/// with RB's first reason. Undeclared, the same run is a `mismatch`
/// (never `verified` either way). Under the ruling on R6.4 (a namespace is
/// never `left`), RB fires through the member `G`'s row: `left 1`, and
/// the opening `namespace old {` is the `left_kept 1` of accounting.
#[test]
fn t_cli_3b_overlay_frozen_rebuilt_file_still_declares() {
    let pre = [("s/lib.h", LIB_PRE), ("s/main.cpp", MAIN_PRE), ("b/gen.h", GEN_PRE)];
    let post = [("s/lib.h", LIB_POST), ("s/main.cpp", MAIN_POST_G_LEFT)];
    let observers: &[(&str, &[&str], &str)] = &[("s/main.cpp", INCLUDE_B, "s")];
    let t = Tree::new("t3b", &pre, &post);
    let form = Form::Overlay(&["s/lib.h", "s/main.cpp"]);
    let ran = run(&t, old_to_fresh(), t.files(&[], &["b/gen.h"]), &form, observers);
    let genp = t.path("b/gen.h").display().to_string();
    assert_eq!(ran.kinds[0].0, VerdictKind::Unverifiable, "{:?}", ran.kinds);
    assert_eq!(
        ran.kinds[0].1,
        format!(
            "post-image: {genp} is rebuilt and still declares an entity under the name rule 0 \
             says was left; an overlay holds no regenerated file (verify a post tree: slip \
             verify --root … --post …)"
        )
    );
    assert!(ran.json.contains("\"left\":1,\"left_kept\":1}"), "{}", ran.json);
}

/// T-cli-3 (c): the tree form with the build directory under no root: the
/// rebuilt file is frozen, RB's second reason.
#[test]
fn t_cli_3c_tree_form_build_directory_under_no_root() {
    let pre = [("s/lib.h", LIB_PRE), ("s/main.cpp", MAIN_PRE), ("b/gen.h", GEN_PRE)];
    let post = [("s/lib.h", LIB_POST), ("s/main.cpp", MAIN_POST_G_LEFT)];
    let observers: &[(&str, &[&str], &str)] = &[("s/main.cpp", INCLUDE_B, "s")];
    let t = Tree::new("t3c", &pre, &post);
    let ran = run(&t, old_to_fresh(), t.files(&[], &["b/gen.h"]), &Form::SubjectRoot, observers);
    let genp = t.path("b/gen.h").display().to_string();
    assert_eq!(ran.kinds[0].0, VerdictKind::Unverifiable, "{:?}", ran.kinds);
    assert_eq!(
        ran.kinds[0].1,
        format!(
            "post-image: {genp} is rebuilt, is under no root of the image, and still declares an \
             entity under the name rule 0 says was left; give its directory as a root (--root \
             <dir> --post <regenerated dir>)"
        )
    );
}

/// T-cli-3 (d), A8b: an owned header opens `old` around an `#include` of a
/// rebuilt fragment and is edited to open `fresh`: the fragment, frozen,
/// declares `G`, which a rule gave its key (`mapped 1 left 0`): `verified`
/// through an overlay, RB does not fire.
#[test]
fn t_cli_3d_fragment_in_a_moved_block() {
    let x_pre = "#pragma once\nnamespace old {\n#include \"XGen.inc\"\n}\n";
    let x_post = "#pragma once\nnamespace fresh {\n#include \"XGen.inc\"\n}\n";
    let main_pre = "#include \"x.h\"\nint main() { old::G g; (void)g; return 0; }\n";
    let main_post = "#include \"x.h\"\nint main() { fresh::G g; (void)g; return 0; }\n";
    let pre = [("s/x.h", x_pre), ("s/main.cpp", main_pre), ("b/XGen.inc", "struct G {};\n")];
    let post = [("s/x.h", x_post), ("s/main.cpp", main_post)];
    let observers: &[(&str, &[&str], &str)] = &[("s/main.cpp", INCLUDE_B, "s")];
    let t = Tree::new("t3d", &pre, &post);
    let form = Form::Overlay(&["s/main.cpp", "s/x.h"]);
    let ran = run(&t, old_to_fresh(), t.files(&[], &["b/XGen.inc"]), &form, observers);
    assert_eq!(ran.kinds[0].0, VerdictKind::Verified, "{:?}\n{}", ran.kinds, ran.text);
    assert_eq!(ran.verdict, RunVerdict::Verified, "{}", ran.text);
    assert!(ran.json.contains("\"mapped\":1,\"left\":0"), "{}", ran.json);
}

/// T-cli-3 (e): a kept entity in a frozen rebuilt file: not `unverifiable`
/// for it (`left_kept` is not `left`). By the ruling on R6.4 (merge
/// review) the rebuilt file's own `namespace old {` is never `left`
/// either: the row is `left 0 left_kept 2` (the namespace and `K`), RB
/// does not fire, and the observer verifies.
#[test]
fn t_cli_3e_kept_entity_in_a_frozen_rebuilt_file() {
    let genp = "#pragma once\nnamespace old { struct K {}; }\n";
    let main_pre = "#include \"lib.h\"\n#include \"gen.h\"\nint main() { old::K k; (void)k; return old::f(); }\n";
    let main_post = "#include \"lib.h\"\n#include \"gen.h\"\nint main() { old::K k; (void)k; return fresh::f(); }\n";
    let pre = [("s/lib.h", LIB_PRE), ("s/main.cpp", main_pre), ("b/gen.h", genp)];
    let post = [("s/lib.h", LIB_POST), ("s/main.cpp", main_post)];
    let observers: &[(&str, &[&str], &str)] = &[("s/main.cpp", INCLUDE_B, "s")];
    let declared = declared_text(
        "relocate old => :: fresh\nkept old::K\n",
        "namespace old { int f(); struct K {}; }\n",
        "namespace fresh { int f(); }\nnamespace old { struct K {}; }\n",
    );
    let t = Tree::new("t3e", &pre, &post);
    let form = Form::Overlay(&["s/lib.h", "s/main.cpp"]);
    let ran = run(&t, declared, t.files(&[], &["b/gen.h"]), &form, observers);
    assert!(ran.json.contains("\"left\":0,\"left_kept\":2}"), "{}", ran.json);
    assert_eq!(ran.kinds[0].0, VerdictKind::Verified, "{:?}\n{}", ran.kinds, ran.text);
}

/// T-cli-3 (f), A13: an overlay made by hand whose `files` name a rebuilt
/// path: O3.
#[test]
fn t_cli_3f_overlay_naming_a_rebuilt_file() {
    let pre = [("s/lib.h", LIB_PRE), ("s/main.cpp", MAIN_PRE), ("b/gen.h", GEN_PRE)];
    let post = [("s/lib.h", LIB_POST), ("s/main.cpp", MAIN_POST), ("b/gen.h", GEN_POST)];
    let observers: &[(&str, &[&str], &str)] = &[("s/main.cpp", INCLUDE_B, "s")];
    let t = Tree::new("t3f", &pre, &post);
    let form = Form::Overlay(&["b/gen.h", "s/lib.h", "s/main.cpp"]);
    let ran = run(&t, old_to_fresh(), t.files(&[], &["b/gen.h"]), &form, observers);
    let genp = t.path("b/gen.h").display().to_string();
    assert_eq!(
        ran.kinds[0],
        (
            VerdictKind::Unverifiable,
            format!("post-image: the overlay names {genp}, a rebuilt file")
        )
    );
}

// Absolute, so that each image's unity file includes that image's `a.cpp`
// (a relative `../ps/a.cpp` would leave the post root, G6 (iii)).
const UNITY_PRE: &str = "#include \"{base}/s/a.cpp\"\n";
const UNITY_POST: &str = "#include \"{base}/ps/a.cpp\"\n";
const A_PRE: &str = "#include \"lib.h\"\nint use() { return old::f(); }\n";
const A_POST: &str = "#include \"lib.h\"\nint use() { return fresh::f(); }\n";

/// T-cli-5 (R1.3): the hint. A unity-build main file in the build
/// directory (the command's `directory`), read-only, includes owned code:
/// a `mismatch`, and one `hint:` line naming the file. No hint line when
/// the file is under a `--readonly` entry, when the command's directory is
/// the subject (the file is under no build directory of the observer),
/// when the observer verified, or when the file is declared rebuilt.
#[test]
fn t_cli_5_the_hint() {
    let pre = [("s/lib.h", LIB_PRE), ("s/a.cpp", A_PRE), ("b/unity.cpp", UNITY_PRE)];
    let post = [("s/lib.h", LIB_POST), ("s/a.cpp", A_POST), ("b/unity.cpp", UNITY_POST)];
    let in_b: &[(&str, &[&str], &str)] = &[("b/unity.cpp", &[], "b")];

    let t = Tree::new("t5", &pre, &post);
    let ran = run(&t, old_to_fresh(), t.files(&[], &[]), &Form::TwoRoots, in_b);
    assert_eq!(ran.kinds[0].0, VerdictKind::Mismatch, "{:?}\n{}", ran.kinds, ran.text);
    let (unity, b) = (t.path("b/unity.cpp").display().to_string(), t.path("b"));
    assert_eq!(
        lines_with(&ran.text, "hint:"),
        [format!(
            "hint: {unity} is read-only, is under the build directory {}, and makes code of the \
             run read-only in 1 observer(s) (first {unity}); if the build writes it again from \
             source, pass --rebuilt {unity} (only if no binary outside this run was compiled \
             against it)",
            b.display()
        )],
        "{}",
        ran.text
    );
    assert!(ran.json.contains("\"hints\":[{\"file\":"), "{}", ran.json);
    drop(t);

    // Under a `--readonly` entry: the user has spoken about it.
    let t = Tree::new("t5-readonly", &pre, &post);
    let ran = run(&t, old_to_fresh(), t.files(&["b/unity.cpp"], &[]), &Form::TwoRoots, in_b);
    assert_eq!(ran.kinds[0].0, VerdictKind::Mismatch, "{:?}", ran.kinds);
    assert!(lines_with(&ran.text, "hint:").is_empty(), "{}", ran.text);
    drop(t);

    // The command's directory is the subject: no build directory holds it.
    let t = Tree::new("t5-directory", &pre, &post);
    let in_s: &[(&str, &[&str], &str)] = &[("b/unity.cpp", &[], "s")];
    let ran = run(&t, old_to_fresh(), t.files(&[], &[]), &Form::TwoRoots, in_s);
    assert_eq!(ran.kinds[0].0, VerdictKind::Mismatch, "{:?}", ran.kinds);
    assert!(lines_with(&ran.text, "hint:").is_empty(), "{}", ran.text);
    drop(t);

    // Declared rebuilt: verified, nothing hinted.
    let t = Tree::new("t5-rebuilt", &pre, &post);
    let ran = run(&t, old_to_fresh(), t.files(&[], &["b/unity.cpp"]), &Form::TwoRoots, in_b);
    assert_eq!(ran.kinds[0].0, VerdictKind::Verified, "{:?}\n{}", ran.kinds, ran.text);
    assert!(lines_with(&ran.text, "hint:").is_empty(), "{}", ran.text);
    drop(t);

    // A verified observer: the cut cost nothing, no line (the shard and
    // the report's JSON still have the hint).
    let quiet = "#include \"lib.h\"\nint use() { return 1; }\n";
    let pre =
        [("s/lib.h", "#pragma once\nint g();\n"), ("s/a.cpp", quiet), ("b/unity.cpp", UNITY_PRE)];
    let post = [("b/unity.cpp", UNITY_POST)];
    let t = Tree::new("t5-verified", &pre, &post);
    let ran = run(&t, old_to_fresh(), t.files(&[], &[]), &Form::TwoRoots, in_b);
    assert_eq!(ran.kinds[0].0, VerdictKind::Verified, "{:?}\n{}", ran.kinds, ran.text);
    assert!(lines_with(&ran.text, "hint:").is_empty(), "{}", ran.text);
    assert!(ran.json.contains("\"hints\":[{\"file\":"), "{}", ran.json);
}

/// T-cli-6, A2: a file under a rebuilt directory that is a symbolic link
/// to a file no entry covers is read-only (by its real path): its entity
/// keeps its name, the observer verifies, and the entry is unused.
#[test]
fn t_cli_6_a2_link_under_a_rebuilt_directory() {
    let dep = "#pragma once\nnamespace old { struct D {}; }\n";
    let main_pre = "#include \"lib.h\"\n#include \"gen/dep.h\"\nint main() { old::D d; (void)d; return old::f(); }\n";
    let main_post = "#include \"lib.h\"\n#include \"gen/dep.h\"\nint main() { old::D d; (void)d; return fresh::f(); }\n";
    let pre = [("s/lib.h", LIB_PRE), ("s/main.cpp", main_pre), ("opt/dep.h", dep)];
    let post = [("s/lib.h", LIB_POST), ("s/main.cpp", main_post)];
    let t = Tree::new("t6-a2", &pre, &post);
    for root in ["b", "pb"] {
        let genp = t.path(&format!("{root}/gen"));
        std::fs::create_dir_all(&genp).unwrap();
        std::os::unix::fs::symlink(t.path("opt/dep.h"), genp.join("dep.h")).unwrap();
    }
    let observers: &[(&str, &[&str], &str)] = &[("s/main.cpp", INCLUDE_B, "s")];
    let ran = run(&t, old_to_fresh(), t.files(&[], &["b/gen"]), &Form::TwoRoots, observers);
    assert_eq!(ran.kinds[0].0, VerdictKind::Verified, "{:?}\n{}", ran.kinds, ran.text);
    assert_eq!(ran.verdict, RunVerdict::Verified, "{}", ran.text);
    let genp = t.path("b/gen").display().to_string();
    assert_eq!(lines_with(&ran.text, "unused rebuilt"), [format!("unused rebuilt {genp}")]);
}

/// T-cli-6, A6: a rebuilt `gen.h` regenerated to declare `fresh::G`, also
/// included by a dependency's source (a read-only main file of the plan):
/// that observer reads `G` as a dependency entity and is a `mismatch`; the
/// owned observer verifies.
#[test]
fn t_cli_6_a6_dependency_includes_the_rebuilt_file() {
    let dep = "#include \"gen.h\"\nint dep() { return 0; }\n";
    let pre =
        [("s/lib.h", LIB_PRE), ("s/main.cpp", MAIN_PRE), ("b/gen.h", GEN_PRE), ("d/dep.cpp", dep)];
    let post = [("s/lib.h", LIB_POST), ("s/main.cpp", MAIN_POST), ("b/gen.h", GEN_POST)];
    let t = Tree::new("t6-a6", &pre, &post);
    let observers: &[(&str, &[&str], &str)] =
        &[("s/main.cpp", INCLUDE_B, "s"), ("d/dep.cpp", INCLUDE_B, "d")];
    let ran = run(&t, old_to_fresh(), t.files(&[], &["b/gen.h"]), &Form::TwoRoots, observers);
    assert_eq!(ran.kinds[0].0, VerdictKind::Verified, "{:?}\n{}", ran.kinds, ran.text);
    // In `dep.cpp`'s observer `gen.h` is the same rebuilt file, but its main
    // file is read-only: `G` is a dependency entity whose name moved.
    assert_eq!(ran.kinds[1].0, VerdictKind::Mismatch, "{:?}\n{}", ran.kinds, ran.text);
    assert_ne!(ran.verdict, RunVerdict::Verified, "{}", ran.text);
}

/// T-cli-6, A6b: the dependency includes an installed copy of the
/// generated header (another file, read-only, not regenerated); with a
/// rule that names the entity, that observer is stale and the run is not
/// verified.
#[test]
fn t_cli_6_a6b_installed_copy_is_stale_under_an_entity_rule() {
    let dep = "#include \"gen.h\"\nint dep() { old::G g; (void)g; return 0; }\n";
    let main_pre = "#include \"gen.h\"\nint main() { old::G g; (void)g; return 0; }\n";
    let main_post = "#include \"gen.h\"\nint main() { fresh::G g; (void)g; return 0; }\n";
    // Both namespaces in both images, as the rule's declared pre and post
    // say (a namespace that appears or vanishes is a difference).
    let gen_pre = "#pragma once\nnamespace old { struct G {}; }\nnamespace fresh {}\n";
    let gen_post = "#pragma once\nnamespace fresh { struct G {}; }\nnamespace old {}\n";
    let pre = [
        ("s/main.cpp", main_pre),
        ("b/gen.h", gen_pre),
        ("opt/gen.h", gen_pre),
        ("d/dep.cpp", dep),
    ];
    let post = [("s/main.cpp", main_post), ("b/gen.h", gen_post)];
    let declared = declared_text(
        "relocate old::G => fresh :: G\n",
        "namespace old { struct G {}; }\nnamespace fresh {}\n",
        "namespace fresh { struct G {}; }\nnamespace old {}\n",
    );
    let t = Tree::new("t6-a6b", &pre, &post);
    let observers: &[(&str, &[&str], &str)] =
        &[("s/main.cpp", INCLUDE_B, "s"), ("d/dep.cpp", &["-I{base}/opt"], "d")];
    let ran = run(&t, declared, t.files(&[], &["b/gen.h"]), &Form::TwoRoots, observers);
    assert_eq!(ran.kinds[0].0, VerdictKind::Verified, "{:?}\n{}", ran.kinds, ran.text);
    assert_eq!(ran.kinds[1].0, VerdictKind::Verified, "{:?}\n{}", ran.kinds, ran.text);
    assert_eq!(lines_with(&ran.text, "stale rule 0").len(), 1, "{}", ran.text);
    assert_ne!(ran.verdict, RunVerdict::Verified, "{}", ran.text);
}

/// A7 (R3.3), the precompiled-header observer the snippet cannot make (it
/// refuses `-include`): a build-written `pch.h` that includes owned
/// `lib.h`, given with `-include`, beside an ordinary observer of the same
/// header. Declared rebuilt, both verify (the key of `old::f` is the
/// rule's in both); undeclared, the `-include` observer reads `f` as a
/// dependency entity and is a `mismatch` while the ordinary one verifies.
#[test]
fn a7_precompiled_header_observer() {
    let pch = "#pragma once\n#include \"lib.h\"\n";
    let other_pre = "int other() { return old::f(); }\n";
    let other_post = "int other() { return fresh::f(); }\n";
    let main_pre = "#include \"lib.h\"\nint main() { return old::f(); }\n";
    let main_post = "#include \"lib.h\"\nint main() { return fresh::f(); }\n";
    let pre = [
        ("s/lib.h", LIB_PRE),
        ("s/main.cpp", main_pre),
        ("s/other.cpp", other_pre),
        ("b/pch.h", pch),
    ];
    let post = [("s/lib.h", LIB_POST), ("s/main.cpp", main_post), ("s/other.cpp", other_post)];
    let observers: &[(&str, &[&str], &str)] = &[
        ("s/main.cpp", &[], "s"),
        ("s/other.cpp", &["-I{base}/s", "-include", "{base}/b/pch.h"], "b"),
    ];

    let t = Tree::new("a7-pch", &pre, &post);
    let ran = run(&t, old_to_fresh(), t.files(&[], &["b/pch.h"]), &Form::TwoRoots, observers);
    assert_eq!(ran.kinds[0].0, VerdictKind::Verified, "{:?}\n{}", ran.kinds, ran.text);
    assert_eq!(ran.kinds[1].0, VerdictKind::Verified, "{:?}\n{}", ran.kinds, ran.text);
    assert_eq!(ran.verdict, RunVerdict::Verified, "{}", ran.text);
    let pchp = t.path("b/pch.h").display().to_string();
    assert_eq!(
        lines_with(&ran.text, "rebuilt "),
        [format!("rebuilt {pchp} relied on by 1 observer(s)")],
        "{}",
        ran.text
    );
    drop(t);

    let t = Tree::new("a7-pch-undeclared", &pre, &post);
    let ran = run(&t, old_to_fresh(), t.files(&[], &[]), &Form::TwoRoots, observers);
    assert_eq!(ran.kinds[0].0, VerdictKind::Verified, "{:?}\n{}", ran.kinds, ran.text);
    assert_eq!(ran.kinds[1].0, VerdictKind::Mismatch, "{:?}\n{}", ran.kinds, ran.text);
    // The cut is under the command's directory: the hint names `pch.h`.
    let pchp = t.path("b/pch.h").display().to_string();
    assert_eq!(lines_with(&ran.text, "hint:").len(), 1, "{}", ran.text);
    assert!(
        lines_with(&ran.text, "hint:")[0].starts_with(&format!("hint: {pchp} ")),
        "{}",
        ran.text
    );
}

/// T-116 (pending): a definition written in a rebuilt file counts as
/// defined in owned code (`docs/wp/WP-116.md`, "Out of scope").
#[test]
#[ignore = "needs: WP-116"]
fn t_116_definition_in_a_rebuilt_file_is_owned_code() {
    unimplemented!("WP-116's run-level check does not exist yet");
}

// The executable.

const SLIP: &str = env!("CARGO_BIN_EXE_slip");
const BOUND: Duration = Duration::from_mins(3);

/// A tree for the executable, removed on drop.
struct Cli {
    root: PathBuf,
}

impl Drop for Cli {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[derive(Debug)]
struct Out {
    status: ExitStatus,
    stdout: String,
    stderr: String,
}

impl Cli {
    fn new(name: &str) -> Cli {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let root = common::work_root().join(format!("wp118-cli-{}-{n}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        Cli { root: std::fs::canonicalize(&root).unwrap() }
    }

    fn path(&self, rel: &str) -> PathBuf {
        self.root.join(rel)
    }

    fn write(&self, rel: &str, text: &str) {
        write(&self.path(rel), text);
    }

    fn read(&self, rel: &str) -> String {
        std::fs::read_to_string(self.path(rel)).unwrap()
    }

    /// `cdb.json` compiling each of `units` as C++17 from the root.
    fn db(&self, units: &[&str]) {
        let dir = self.root.to_str().unwrap();
        let items: Vec<String> = units
            .iter()
            .map(|u| {
                format!(
                    "{{\"directory\": \"{dir}\", \"file\": \"{u}\", \"command\": \"clang++ \
                     -std=c++17 -c {u}\"}}"
                )
            })
            .collect();
        self.write("cdb.json", &format!("[\n{}\n]\n", items.join(",\n")));
    }

    fn slip(&self, args: &[&str]) -> Out {
        let mut cmd = Process::new(SLIP);
        cmd.args(args).current_dir(&self.root);
        cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
        let mut child = cmd.spawn().expect("start slip");
        let stdout = reader(child.stdout.take().unwrap());
        let stderr = reader(child.stderr.take().unwrap());
        let status = wait_bounded(&mut child, BOUND);
        Out { status, stdout: stdout.join().unwrap(), stderr: stderr.join().unwrap() }
    }

    fn plan(&self) -> String {
        let out = self.slip(&["plan", "--db", "cdb.json", "--store", "store"]);
        assert_eq!(out.status.code(), Some(0), "{out:?}");
        out.stdout.lines().find_map(|l| l.strip_prefix("plan ")).unwrap().to_owned()
    }
}

fn reader(mut r: impl Read + Send + 'static) -> std::thread::JoinHandle<String> {
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = r.read_to_end(&mut bytes);
        String::from_utf8_lossy(&bytes).into_owned()
    })
}

fn wait_bounded(child: &mut Child, bound: Duration) -> ExitStatus {
    let deadline = Instant::now() + bound;
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            return status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("slip did not finish within {bound:?}");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

const H: &str = "#pragma once\nint f(int);\n";

/// T-cli-3 (a): Demo 2's shape through `slip rename`: a rebuilt `.inc` in
/// the build directory that only includes an owned header, entered first.
/// Declared rebuilt, the rename of `f` verifies through its overlay (the
/// `.inc` is frozen and verifies as it stands); undeclared, the header is
/// read-only for that inclusion and the run is `not_verified`, with the
/// hint naming the `.inc`.
#[test]
fn t_cli_3a_rename_with_a_rebuilt_inc() {
    let setup = |name: &str| {
        let t = Cli::new(name);
        t.write("src/h.h", H);
        t.write("src/a.cpp", "#include \"../b/X.inc\"\nint g() { return f(2); }\n");
        t.write("b/X.inc", "#include \"../src/h.h\"\n");
        t.db(&["src/a.cpp"]);
        t
    };
    let rename = |t: &Cli, extra: &[&str]| {
        let plan = t.plan();
        let mut args = vec![
            "rename", "--plan", &plan, "--from", "f", "--to", "renamed", "--owned", "src",
            "--store", "store", "-j", "2",
        ];
        args.extend_from_slice(extra);
        t.slip(&args)
    };

    let t = setup("t3a");
    let out = rename(&t, &["--rebuilt", "b/X.inc"]);
    assert!(out.stdout.lines().any(|l| l == "state verified"), "{out:?}");
    let inc = t.path("b/X.inc").display().to_string();
    assert!(
        out.stdout.lines().any(|l| l == format!("rebuilt {inc} relied on by 1 observer(s)")),
        "{out:?}"
    );
    drop(t);

    let t = setup("t3a-undeclared");
    let out = rename(&t, &[]);
    assert!(out.stdout.lines().any(|l| l == "state not_verified"), "{out:?}");
    assert!(
        out.stdout
            .lines()
            .any(|l| l.starts_with(&format!("hint: {} is read-only", t.path("b/X.inc").display()))),
        "{out:?}"
    );
}

/// T-cli-4: `slip rename` of an entity a rebuilt file names: the set is
/// refused (`site_refused`), the count per file, nothing written.
#[test]
fn t_cli_4_rename_refused_in_a_rebuilt_file() {
    let t = Cli::new("t4");
    t.write("src/h.h", H);
    t.write(
        "src/a.cpp",
        "#include \"h.h\"\n#include \"../b/gen/use.h\"\nint g() { return f(2) + u(); }\n",
    );
    t.write("b/gen/use.h", "#pragma once\ninline int u() { return f(1); }\n");
    t.db(&["src/a.cpp"]);
    let plan = t.plan();
    let out = t.slip(&[
        "rename",
        "--plan",
        &plan,
        "--from",
        "f",
        "--to",
        "renamed",
        "--owned",
        "src",
        "--rebuilt",
        "b/gen/use.h",
        "--store",
        "store",
        "-j",
        "2",
        "--write",
    ]);
    let lines: Vec<&str> = out.stdout.lines().collect();
    assert!(lines.contains(&"state refused"), "{out:?}");
    assert!(lines.contains(&"refused site_refused 1"), "{out:?}");
    let use_h = t.path("b/gen/use.h").display().to_string();
    assert!(
        lines.contains(
            &format!(
                "rebuilt {use_h}: 1 name(s) the build would have to change; slip rename cannot \
                 make this change (verify a post tree, spec/verify.md §5.1)"
            )
            .as_str()
        ),
        "{out:?}"
    );
    assert!(out.stdout.contains("refusal site_refused rebuilt: "), "{out:?}");
    assert_eq!(t.read("src/h.h"), H);
    assert_eq!(t.read("b/gen/use.h"), "#pragma once\ninline int u() { return f(1); }\n");
}

/// T-cli-7: the command line. A1b (one path in two lists, also through a
/// link), A1d (`--rebuilt /`), `--rebuilt` without `--owned`, an entry
/// that does not exist, and `declare`'s warning.
#[test]
fn t_cli_7_the_command_line() {
    let t = Cli::new("t7");
    t.write("src/h.h", H);
    t.write("src/a.cpp", "#include \"h.h\"\nint g() { return f(2); }\n");
    t.write("b/gen/x.h", "int x();\n");
    std::os::unix::fs::symlink(t.path("b/gen"), t.path("b/link")).unwrap();
    t.db(&["src/a.cpp"]);
    let plan = t.plan();
    let map = |extra: &[&str]| {
        let mut args = vec![
            "map", "--plan", &plan, "--from", "f", "--to", "renamed", "--owned", "src", "--store",
            "store",
        ];
        args.extend_from_slice(extra);
        t.slip(&args)
    };
    let genp = t.path("b/gen").display().to_string();

    let out = map(&["--readonly", "b/gen", "--rebuilt", "b/gen"]);
    assert_eq!(out.status.code(), Some(3), "{out:?}");
    assert!(
        out.stderr.contains(&format!("{genp}: given as --readonly and as --rebuilt")),
        "{out:?}"
    );
    let out = map(&["--readonly", "b/gen", "--rebuilt", "b/link"]);
    assert_eq!(out.status.code(), Some(3), "{out:?}");
    assert!(
        out.stderr.contains(&format!("{genp}: given as --readonly and as --rebuilt")),
        "{out:?}"
    );

    let out = map(&["--rebuilt", "/"]);
    assert_eq!(out.status.code(), Some(3), "{out:?}");
    assert!(out.stderr.contains("a rebuilt entry is /"), "{out:?}");

    let out = map(&["--rebuilt", "b/none"]);
    assert_eq!(out.status.code(), Some(3), "{out:?}");

    let out = t.slip(&[
        "map",
        "--plan",
        &plan,
        "--from",
        "f",
        "--to",
        "renamed",
        "--rebuilt",
        "b/gen",
        "--store",
        "store",
    ]);
    assert_eq!(out.status.code(), Some(2), "{out:?}");

    // `declare`: a `--rebuilt` entry that covers no file a selected
    // pre-image read.
    t.write("decl.txt", "");
    let out = t.slip(&[
        "declare",
        "--plan",
        &plan,
        "--from",
        "decl.txt",
        "--out",
        "decl.bin",
        "--owned",
        "src",
        "--rebuilt",
        "b/gen",
        "--store",
        "store",
    ]);
    assert!(
        out.stderr.contains(&format!(
            "slip: warning: declare: --rebuilt {genp} covers no file of the selected observers"
        )),
        "{out:?}"
    );
}
