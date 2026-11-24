//! `spec/verify.md` §6.1 "Tests": the place of a finding, through the
//! verify task (§5.2 step 6) on trees the test writes (work package
//! WP-054). Each case plants one difference between the subject `s/a.cpp`
//! and the post-image `post-tree/a.cpp` and finds the finding's `location`
//! at its line on each side: a changed callee, a changed literal, a
//! changed type of a declaration, a changed default argument, a removed
//! function, an added one, an indeterminate unit.
//!
//! A parse changes the process's current directory: every test that
//! parses holds [`LOCK`], and every path is absolute (as in
//! `tests/verify_job.rs`, whose harness this is, cut to what it needs).

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
use slip_model::verdict::{Finding, FindingKind, Site};
use slip_model::{Command, Digest, FileRef, Language, Observer, Origin, Phase, Shard};

static LOCK: Mutex<()> = Mutex::new(());

/// A subject tree `s/` and a post tree `post-tree/` holding `a.cpp`, and
/// a store, in a directory of their own removed on drop.
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

impl Tree {
    fn new(name: &str, pre: &str, post: &str) -> Tree {
        static N: AtomicUsize = AtomicUsize::new(0);
        let lock = LOCK.lock().unwrap_or_else(PoisonError::into_inner);
        let n = N.fetch_add(1, Ordering::SeqCst);
        let base = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
            .join(format!("wp054-{name}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        let base = std::fs::canonicalize(&base).unwrap();
        let subject = base.join("s");
        let post_tree = base.join("post-tree");
        std::fs::create_dir_all(&subject).unwrap();
        std::fs::create_dir_all(&post_tree).unwrap();
        std::fs::write(subject.join("a.cpp"), pre).unwrap();
        std::fs::write(post_tree.join("a.cpp"), post).unwrap();
        let store = Store::open(&base.join("store"), Access::Shared).unwrap();
        Tree { base, subject, post: post_tree, store, _lock: lock }
    }

    fn observer(&self) -> Observer {
        let main = self.subject.join("a.cpp");
        let argv = vec![b"c++".to_vec(), b"-std=c++20".to_vec(), bytes(&main)];
        let command = Command { directory: bytes(&self.subject), argv, env: Vec::new() };
        Observer {
            id: command.observer_id(),
            command,
            language: Language::Cxx,
            main: FileRef { path: bytes(&main), size: 0, content: Digest([0; 32]) },
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

    /// The verify task's shard, after the map shard was made and published.
    fn verify(&self) -> Shard {
        let o = self.observer();
        let job = self.job(&o, Phase::Map, 0);
        let shard = MapTask::new().run(&job).unwrap();
        self.store.publish_shard(&job.plan, Phase::Map, 0, &o.id, &shard.to_bytes()).unwrap();
        let image =
            Image::new(vec![RootMap { subject: bytes(&self.subject), post: bytes(&self.post) }])
                .unwrap();
        let declared = Declared::none();
        let declared_id = content_id(&declared.to_bytes()).unwrap();
        let params = Params { pre: 0, declared, declared_id, image, limit: 64, locate: 16 };
        let v = verify(&self.store, &self.job(&o, Phase::Verify, 1), &params).unwrap();
        v.validate().unwrap();
        // §7.2: every unit wanted had a trace.
        assert!(!v.counters.iter().any(|c| c.name == "verify.traces.unstable"));
        v
    }

    /// The subject path of `a.cpp`: both sides' sites name it.
    fn file(&self) -> Vec<u8> {
        bytes(&self.subject.join("a.cpp"))
    }
}

/// The finding of `kind` named `name`.
fn finding<'v>(v: &'v Shard, kind: FindingKind, name: &str) -> &'v Finding {
    let findings = &v.verdict.as_ref().expect("a verdict").findings;
    findings
        .iter()
        .find(|f| f.kind == kind && f.name == name)
        .unwrap_or_else(|| panic!("no {kind:?} {name}: {findings:#?}"))
}

/// Asserts a site's file, line and node.
fn at(site: &Site, file: &[u8], line: u64, node: &str) {
    assert_eq!(
        (site.file.as_slice(), site.line, site.node.as_str()),
        (file, line, node),
        "{site:#?}"
    );
    assert!(site.column > 0, "{site:#?}");
}

fn empty(site: &Site) {
    assert_eq!(*site, Site::default());
}

// A call that now reaches another function: both sites at the callee's
// reference on the line of the call, each naming the function it refers
// to in its image.
#[test]
fn s6_1_changed_callee() {
    let src = |callee: &str| {
        format!("int g(int);\nint h(int);\nint f(int x) {{\n  x += 1;\n  return {callee}(x);\n}}\n")
    };
    let tree = Tree::new("callee", &src("g"), &src("h"));
    let v = tree.verify();
    let w = finding(&v, FindingKind::Changed, "f(int)").location.clone().expect("a location");
    at(&w.pre, &tree.file(), 5, "DeclRefExpr");
    at(&w.post, &tree.file(), 5, "DeclRefExpr");
    assert_eq!(
        (w.pre.what.as_str(), w.post.what.as_str()),
        ("refers to g(int)", "refers to h(int)")
    );
}

// A changed literal: the literal, on its line; no reference.
#[test]
fn s6_1_changed_literal() {
    let src = |n: u32| format!("int f(int x) {{\n  x += 1;\n  return x\n    * {n};\n}}\n");
    let tree = Tree::new("literal", &src(2), &src(3));
    let v = tree.verify();
    let w = finding(&v, FindingKind::Changed, "f(int)").location.clone().expect("a location");
    at(&w.pre, &tree.file(), 4, "IntegerLiteral");
    at(&w.post, &tree.file(), 4, "IntegerLiteral");
    assert_eq!((w.pre.what.as_str(), w.post.what.as_str()), ("", ""));
}

// A changed type of a declaration: the declaration statement, on its
// line.
#[test]
fn s6_1_changed_declaration_type() {
    let src = |t: &str| format!("int f(int x) {{\n  x += 1;\n  {t} y = x;\n  return y;\n}}\n");
    let tree = Tree::new("decl-type", &src("long"), &src("short"));
    let v = tree.verify();
    let w = finding(&v, FindingKind::Changed, "f(int)").location.clone().expect("a location");
    at(&w.pre, &tree.file(), 3, "DeclStmt");
    at(&w.post, &tree.file(), 3, "DeclStmt");
}

// A changed default argument: the default argument's unit, at the
// expression's line.
#[test]
fn s6_1_changed_default_argument() {
    let src = |n: u32| format!("int g(int a,\n      int b =\n        {n} + 1);\n");
    let tree = Tree::new("default-arg", &src(1), &src(2));
    let v = tree.verify();
    let findings = &v.verdict.as_ref().unwrap().findings;
    let w = findings
        .iter()
        .filter(|f| f.kind == FindingKind::Changed)
        .find_map(|f| f.location.clone().filter(|w| w.pre.line == 3))
        .unwrap_or_else(|| panic!("no changed finding placed at line 3: {findings:#?}"));
    at(&w.pre, &tree.file(), 3, "IntegerLiteral");
    at(&w.post, &tree.file(), 3, "IntegerLiteral");
}

// A removed function: its first mark (its declaration's line) in the
// pre-image, the empty site in the post-image.
#[test]
fn s6_1_removed_function() {
    let tree = Tree::new("removed", "int kept(int);\nint gone(int);\n", "int kept(int);\n");
    let v = tree.verify();
    let w = finding(&v, FindingKind::Removed, "gone(int)").location.clone().expect("a location");
    at(&w.pre, &tree.file(), 2, "function");
    empty(&w.post);
}

// An added function: the empty site in the pre-image, its first mark in
// the post-image.
#[test]
fn s6_1_added_function() {
    let tree = Tree::new("added", "int kept(int);\n", "int kept(int);\n\nint added(int);\n");
    let v = tree.verify();
    let w = finding(&v, FindingKind::Added, "added(int)").location.clone().expect("a location");
    empty(&w.pre);
    at(&w.post, &tree.file(), 3, "function");
}

// An indeterminate unit (a file-scope compound literal, whose implicit
// variable is unsupported): in each image its first mark with the cause
// as `what`.
#[test]
fn s6_1_indeterminate_unit() {
    let pre = "int a = 1;\n\nint *p = (int[]){1, 2};\n";
    let post = "int a = 2;\n\nint *p = (int[]){1, 2};\n";
    let tree = Tree::new("indeterminate", pre, post);
    let v = tree.verify();
    let f = finding(&v, FindingKind::Indeterminate, "p");
    let w = f.location.clone().expect("a location");
    for s in [&w.pre, &w.post] {
        at(s, &tree.file(), 3, "variable");
        assert!(!s.what.is_empty(), "no cause: {s:#?}");
    }
}

// An explicit specialisation removed (E11 `bad-3`, WP-128): declared in
// the pre-image, derived in the post-image, a `changed` of §4 step 4 whose
// sites are the units' first marks. The pre side is the specialisation the
// source wrote (line 3), not the declaration Kiln made from the template
// before it was specialised, which is at the pattern (line 1); the post
// side, an instantiation, is at the pattern.
#[test]
fn s6_1_explicit_specialisation_removed() {
    let pre = "template <class T> int pick() { return 1; }\n\n\
               template <> int pick<int>() { return 2; }\nint user() { return pick<int>(); }\n";
    let post = "template <class T> int pick() { return 1; }\n\n\n\
                int user() { return pick<int>(); }\n";
    let tree = Tree::new("explicit-spec", pre, post);
    let v = tree.verify();
    let w = finding(&v, FindingKind::Changed, "pick<int>()").location.clone().expect("a location");
    at(&w.pre, &tree.file(), 3, "specialisation");
    at(&w.post, &tree.file(), 1, "specialisation");
    assert_eq!((w.pre.what.as_str(), w.post.what.as_str()), ("", ""));
}
