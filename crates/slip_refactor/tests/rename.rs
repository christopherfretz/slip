//! The native rename (`spec/edits.md` §9, `docs/wp/WP-071.md` "Tests").
//!
//! Every directory of `tests/rename/` is one case: `test.txt` (the
//! request: `spec:` lines naming what it covers, `from: a::b::f`, `to:
//! g`, `kind: function|variable|any`, an optional `args:` line of extra
//! front-end words), the files of a small tree (`slip_kiln::tree`; the
//! main file is `main.cpp`; everything under `dep/` is read-only), and
//! `expect.txt`, a text rendering of the `Edits`:
//!
//! ```text
//! subjects:
//!   <key, 8 digits> <role> <flags> rule=<yes|no> <display>      (follower: of=<keys>)
//! links:
//!   <a> <b>
//! claims:
//!   <file>:<line>:<column>+<length> <stance> <pieces> <tiers> votes=<n> <reason> [<subjects>] (macro=<name>@<offset>)
//! changed:
//!   <key> <file>:<line>:<column>+<length> <reason>
//! ```
//!
//! a piece being `"text"` or `spell(<key, 8 digits>, "text")`. A unit with
//! errors renders as `diagnosed` (the worker does not run a transform on
//! it; `run_on_a_diagnosed_unit_does_not_panic` checks the rule).
//! `SLIP_BLESS=1` rewrites `expect.txt`; every one is reviewed by hand.
//!
//! Keys are computed with subject paths relative to the tree's root, so
//! that they do not depend on where the tree is written.

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use slip_digest::query::{Keys, NoKey};
use slip_kiln::kiln::ast::DeclId;
use slip_kiln::tree::Tree;
use slip_kiln::written;
use slip_kiln::{ParseOptions, Unit};
use slip_model::edits::{Edits, Piece, Reason, Role, Stance, subject_flags, tiers};
use slip_model::keys::{Name, RuleTable};
use slip_model::transform::{Ownership, Rename, TargetKind, TransformKind};
use slip_model::{Digest, Status, Transform};

// ---------------------------------------------------------------------
// A case.

/// One directory.
#[derive(Debug)]
struct Case {
    name: String,
    dir: PathBuf,
    target: Vec<Vec<u8>>,
    kind: TargetKind,
    new_name: Vec<u8>,
    args: Vec<String>,
    /// Relative path and bytes; `test.txt` and `expect.txt` left out.
    files: Vec<(String, Vec<u8>)>,
}

fn cases_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("rename")
}

/// Every directory, by name.
fn cases() -> Vec<Case> {
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(cases_dir())
        .expect("tests/rename")
        .map(|e| e.expect("entry").path())
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();
    // `SLIP_CASE=<name>` runs one directory.
    if let Some(only) = std::env::var_os("SLIP_CASE") {
        dirs.retain(|d| d.file_name() == Some(only.as_os_str()));
    }
    dirs.into_iter().map(|d| read_case(&d)).collect()
}

fn read_case(dir: &Path) -> Case {
    let name = dir.file_name().unwrap().to_string_lossy().into_owned();
    let text = std::fs::read_to_string(dir.join("test.txt"))
        .unwrap_or_else(|e| panic!("{name}: test.txt: {e}"));
    let (mut target, mut kind, mut new_name, mut args, mut spec) =
        (None, None, None, Vec::new(), 0);
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let (k, v) = line.split_once(':').unwrap_or_else(|| panic!("{name}: bad line {line:?}"));
        let v = v.trim();
        match k.trim() {
            "spec" => spec += 1,
            "from" => target = Some(v.split("::").map(|c| c.as_bytes().to_vec()).collect()),
            "to" => new_name = Some(v.as_bytes().to_vec()),
            "kind" => {
                kind = Some(match v {
                    "function" => TargetKind::Function,
                    "variable" => TargetKind::Variable,
                    "any" => TargetKind::Any,
                    _ => panic!("{name}: kind {v:?}"),
                });
            }
            "args" => args = v.split_whitespace().map(str::to_owned).collect(),
            _ => panic!("{name}: unknown key {k:?}"),
        }
    }
    assert!(spec > 0, "{name}: test.txt names no section (`spec:`)");
    let mut files = Vec::new();
    collect_files(dir, dir, &mut files);
    files.retain(|(p, _)| p != "test.txt" && p != "expect.txt");
    files.sort();
    assert!(files.iter().any(|(p, _)| p == "main.cpp"), "{name}: no main.cpp");
    Case {
        name: name.clone(),
        dir: dir.to_owned(),
        target: target.unwrap_or_else(|| panic!("{name}: no from:")),
        kind: kind.unwrap_or_else(|| panic!("{name}: no kind:")),
        new_name: new_name.unwrap_or_else(|| panic!("{name}: no to:")),
        args,
        files,
    }
}

fn collect_files(root: &Path, dir: &Path, out: &mut Vec<(String, Vec<u8>)>) {
    let mut entries: Vec<PathBuf> =
        std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().path()).collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            collect_files(root, &p, out);
        } else {
            let rel = p.strip_prefix(root).unwrap().to_string_lossy().into_owned();
            out.push((rel, std::fs::read(&p).unwrap()));
        }
    }
}

fn written_options() -> ParseOptions {
    let mut o = ParseOptions::default();
    o.written = true;
    o.pp_record = true;
    o
}

fn transform(case: &Case, root: &str) -> Transform {
    Transform {
        kind: TransformKind::Rename,
        ownership: Ownership {
            owned: vec![root.as_bytes().to_vec()],
            readonly: vec![format!("{root}/dep").into_bytes()],
        },
        indivisible: true,
        rename: Some(Rename {
            target: case.target.clone(),
            kind: case.kind,
            new_name: case.new_name.clone(),
        }),
        script: None,
    }
}

/// What a run of a case gave.
enum Ran {
    Diagnosed,
    Edits(Box<Edits>),
}

/// Writes `files` as a tree, parses `main.cpp` with the written tables on
/// and, if the unit has no error, calls `f` with the unit, its keys (no
/// rules; subject paths relative to the root) and the root.
fn with_unit<R: Send>(
    name: &str,
    files: &[(String, Vec<u8>)],
    args: &[String],
    f: impl FnOnce(&Unit<'_>, &mut Keys<'_>, &str) -> R + Send,
) -> Option<R> {
    let refs: Vec<(&str, &[u8])> = files.iter().map(|(p, b)| (p.as_str(), b.as_slice())).collect();
    let tree = Tree::new(Path::new(env!("CARGO_TARGET_TMPDIR")), name, &refs).unwrap();
    let root = tree.root().to_str().unwrap().to_owned();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let obs = tree.observer("main.cpp", "c++20", &args);
    let (shard, r) = tree
        .parse(&obs, &written_options(), |u| {
            if u.errors() > 0 {
                return None;
            }
            let prefix = format!("{root}/");
            let subject = |p: &[u8]| p.strip_prefix(prefix.as_bytes()).unwrap_or(p).to_vec();
            let r = slip_digest::map::with_input(u, &subject, |input| {
                let mut keys = Keys::new(input);
                f(u, &mut keys, &root)
            });
            Some(r.unwrap_or_else(|e| panic!("{name}: {e}")))
        })
        .unwrap_or_else(|e| panic!("{name}: {e}"));
    assert!(
        matches!(shard.outcome.status, Status::Ok | Status::Diagnosed),
        "{name}: {:?}",
        shard.outcome
    );
    r.expect("a parse happened")
}

fn run_case(case: &Case) -> Ran {
    let out = with_unit(&case.name, &case.files, &case.args, |u, keys, root| {
        let t = transform(case, root);
        let ownership = t.ownership.clone();
        let sites = slip_kiln::sites::Sites::new(u, &ownership);
        let (edits, counters) =
            slip_refactor::run_counted(&t, u, keys, &|f| sites.file_ref(f)).expect("run");
        check_counters(&case.name, &edits, &counters);
        edits.validate_against(&case.name);
        // Paths relative to the root, for the rendering.
        let mut edits = edits;
        for f in &mut edits.files {
            f.path = f.path.strip_prefix(format!("{root}/").as_bytes()).unwrap_or(&f.path).to_vec();
        }
        edits
    });
    out.map_or(Ran::Diagnosed, |e| Ran::Edits(Box::new(e)))
}

/// §4.7 through the shard's own check, and the rename's own promises
/// (§9.6).
trait Validate {
    fn validate_against(&self, name: &str);
}

impl Validate for Edits {
    fn validate_against(&self, name: &str) {
        let mut e = self.clone();
        e.normalise();
        assert_eq!(&e, self, "{name}: the section is not normalised");
        assert!(self.failure.is_none() && self.groups.is_empty(), "{name}");
        assert!(self.reports.is_empty() && self.renamed.is_empty(), "{name}");
        let ids: BTreeSet<Digest> = self.subjects.iter().map(|s| s.id).collect();
        for c in &self.claims {
            for s in &c.subjects {
                assert!(ids.contains(s), "{name}: a claim names a non-subject");
            }
            for p in &c.pieces {
                if let Piece::Spelled { key, .. } = p {
                    assert!(ids.contains(key), "{name}: a spelled key is no subject");
                }
            }
            match c.stance {
                Stance::Keep => assert!(c.subjects.is_empty(), "{name}: keep with subjects"),
                Stance::Refuse => assert!(!c.subjects.is_empty(), "{name}: refuse without"),
                _ => {}
            }
        }
    }
}

fn check_counters(name: &str, edits: &Edits, counters: &[slip_model::Counter]) {
    let get = |n: &str| counters.iter().find(|c| c.name == n).map(|c| c.value);
    assert_eq!(get("edits.claims"), Some(edits.claims.len() as u64), "{name}");
    assert_eq!(get("edits.subjects"), Some(edits.subjects.len() as u64), "{name}");
    let names: Vec<&str> = counters.iter().map(|c| c.name.as_str()).collect();
    let mut sorted = names.clone();
    sorted.sort_unstable();
    assert_eq!(names, sorted, "{name}: counters not sorted");
}

// ---------------------------------------------------------------------
// The rendering.

fn short(d: &Digest) -> String {
    d.to_string()[..8].to_owned()
}

fn quoted(b: &[u8]) -> String {
    format!("{:?}", String::from_utf8_lossy(b))
}

fn role(r: Role) -> &'static str {
    match r {
        Role::Seed => "seed",
        Role::Candidate => "candidate",
        Role::Follower => "follower",
    }
}

fn flags(f: u64) -> String {
    let names = [
        (subject_flags::READ_ONLY, "read_only"),
        (subject_flags::UNSUPPORTED, "unsupported"),
        (subject_flags::NAME_TAKEN, "name_taken"),
        (subject_flags::UNREACHABLE, "unreachable"),
    ];
    let set: Vec<&str> = names.iter().filter(|(b, _)| f & b != 0).map(|(_, n)| *n).collect();
    if set.is_empty() { "-".to_owned() } else { set.join("|") }
}

fn tier_names(t: u64) -> String {
    let names = [
        (tiers::T0, "t0"),
        (tiers::T1, "t1"),
        (tiers::T2, "t2"),
        (tiers::T3, "t3"),
        (tiers::X, "x"),
    ];
    names.iter().filter(|(b, _)| t & b != 0).map(|(_, n)| *n).collect::<Vec<_>>().join("|")
}

/// `file:line:column+length`, from the case's own bytes of the file.
fn place(case: &Case, path: &[u8], lo: u64, hi: u64) -> String {
    let path = String::from_utf8_lossy(path).into_owned();
    let bytes = case.files.iter().find(|(p, _)| *p == path).map(|(_, b)| b.as_slice());
    let (line, column) = bytes.map_or((0, 0), |b| {
        let before = &b[..(lo as usize).min(b.len())];
        let line = before.split(|&c| c == b'\n').count();
        let start = before.iter().rposition(|&c| c == b'\n').map_or(0, |i| i + 1);
        (line, before.len() - start + 1)
    });
    format!("{path}:{line}:{column}+{}", hi - lo)
}

fn render(case: &Case, ran: &Ran) -> String {
    let Ran::Edits(e) = ran else { return "diagnosed\n".to_owned() };
    let mut s = String::from("subjects:\n");
    for x in &e.subjects {
        let _ = write!(
            s,
            "  {} {} {} rule={} {}",
            short(&x.id),
            role(x.role),
            flags(x.flags),
            if x.rule.is_some() { "yes" } else { "no" },
            x.display
        );
        if !x.of.is_empty() {
            let of: Vec<String> = x.of.iter().map(short).collect();
            let _ = write!(s, " of={}", of.join(","));
        }
        s.push('\n');
    }
    s.push_str("links:\n");
    for l in &e.links {
        let _ = writeln!(s, "  {} {}", short(&l.a), short(&l.b));
    }
    s.push_str("claims:\n");
    for c in &e.claims {
        let path = &e.files[c.file as usize].path;
        let stance = match c.stance {
            Stance::Replace => "replace",
            Stance::Keep => "keep",
            Stance::Refuse => "refuse",
            Stance::Unresolved => "unresolved",
        };
        let pieces: Vec<String> = c
            .pieces
            .iter()
            .map(|p| match p {
                Piece::Literal(b) => quoted(b),
                Piece::Spelled { key, text } => format!("spell({}, {})", short(key), quoted(text)),
                Piece::Copy(p) => format!("copy({}, {}, {})", p.file, p.lo, p.hi),
            })
            .collect();
        let _ = write!(
            s,
            "  {} {stance}{}{} {} votes={} {}",
            place(case, path, c.lo, c.hi),
            if pieces.is_empty() { "" } else { " " },
            pieces.join(" "),
            tier_names(c.tiers),
            c.votes,
            reason_name(c.reason),
        );
        if !c.subjects.is_empty() {
            let subs: Vec<String> = c.subjects.iter().map(short).collect();
            let _ = write!(s, " [{}]", subs.join(","));
        }
        if let Some(m) = &c.macro_site {
            let _ = write!(s, " macro={}@{}", String::from_utf8_lossy(&m.name), m.name_start);
        }
        s.push('\n');
    }
    s.push_str("changed:\n");
    for c in &e.changed {
        let path = &e.files[c.place.file as usize].path;
        let _ = writeln!(
            s,
            "  {} {} {}",
            short(&c.key),
            place(case, path, c.place.lo, c.place.hi),
            String::from_utf8_lossy(&c.reason)
        );
    }
    s
}

// ---------------------------------------------------------------------
// The tests.

/// Every directory against its `expect.txt`.
#[test]
fn directories() {
    let bless = std::env::var_os("SLIP_BLESS").is_some();
    let mut failed = Vec::new();
    let all = cases();
    assert!(!all.is_empty());
    let mut pending = Vec::new();
    for case in &all {
        let got = render(case, &run_case(case));
        let path = case.dir.join("expect.txt");
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        // `# pending` (as `crates/slip_digest/tests` mark them): the
        // lines that are not comments are what the rename should give once
        // the hole named in the comments is closed. It is run, not
        // compared, and never blessed; giving it already is a failure, so
        // that the mark is taken off when the hole closes.
        if is_pending(case) {
            let mut want = String::new();
            for l in text.lines().filter(|l| !l.starts_with('#')) {
                want.push_str(l);
                want.push('\n');
            }
            if got == want {
                failed.push(format!(
                    "--- {}: marked # pending, and gives what it expects",
                    case.name
                ));
            }
            pending.push(case.name.clone());
            continue;
        }
        let want = text;
        if got != want {
            if bless {
                std::fs::write(&path, &got).unwrap();
            } else {
                failed.push(format!("--- {}\n{}\n+++ got\n{}", case.name, want, got));
            }
        }
    }
    if !pending.is_empty() {
        eprintln!("pending ({}): {}", pending.len(), pending.join(", "));
    }
    assert!(
        failed.is_empty(),
        "{} of {} directories differ:\n{}",
        failed.len(),
        all.len(),
        failed.join("\n")
    );
}

/// Two parses of one tree give equal `Edits` (§9.7: `run` is a function
/// of the unit). Each parse writes the tree to a directory of its own, so
/// the request (its ownership names the root) and its id differ; nothing
/// else may.
#[test]
fn run_is_a_function_of_the_unit() {
    for case in cases() {
        let (Ran::Edits(a), Ran::Edits(mut b)) = (run_case(&case), run_case(&case)) else {
            continue;
        };
        b.transform = a.transform;
        assert_eq!(a, b, "{}", case.name);
    }
}

/// `docs/wp/WP-071.md` "Tests", the last paragraph: for every directory,
/// each subject's rule, applied by a walk of the post-image (the tree
/// with every `replace` claim spliced in), gives the subject's id. Asked
/// of each subject that has a rule, in a directory with no flagged
/// subject, no `# pending` mark, and claims that are all `replace` or
/// `keep` and agree at every range: anything else is not a run that
/// would be applied (a flagged subject refuses the run, §5.2).
#[test]
fn rules_give_the_ids_in_the_post_image() {
    let mut checked = 0;
    for case in cases() {
        if is_pending(&case) {
            continue;
        }
        let Ran::Edits(e) = run_case(&case) else { continue };
        // A run with a flagged subject is refused (§5.2): nothing of it is
        // applied, so there is no post-image to ask.
        let applicable = e.subjects.iter().all(|s| s.flags == 0)
            && e.claims.iter().all(|c| matches!(c.stance, Stance::Replace | Stance::Keep))
            && e.claims
                .windows(2)
                .all(|w| (w[0].file, w[0].lo, w[0].hi) != (w[1].file, w[1].lo, w[1].hi));
        let want: Vec<Digest> =
            e.subjects.iter().filter(|s| s.rule.is_some() && s.flags == 0).map(|s| s.id).collect();
        if !applicable || want.is_empty() {
            continue;
        }
        let rules: Vec<_> = e.subjects.iter().filter_map(|s| s.rule.clone()).collect();
        let table = RuleTable::new(rules).expect("a rule table");
        let files = post_image(&case, &e);
        let name = format!("{}-post", case.name);
        let new_name = case.new_name.clone();
        let got = with_unit(&name, &files, &case.args, |u, _, root| {
            let prefix = format!("{root}/");
            let subject = |p: &[u8]| p.strip_prefix(prefix.as_bytes()).unwrap_or(p).to_vec();
            slip_digest::map::with_input(u, &subject, |input| {
                // Every file owned: a run with a subject declared in a
                // read-only file is flagged `read_only` and skipped above.
                let mut keys = Keys::with_rules(input, &table, slip_digest::ReadOnly::NO_FILE);
                let mut ids = BTreeSet::new();
                for d in written::decl_names(u) {
                    let e = written::origin(u, d.decl);
                    if named(u, e, &new_name) {
                        match keys.key_id(e) {
                            Ok(id) => {
                                ids.insert(id);
                            }
                            Err(NoKey::Indeterminate(c)) => {
                                panic!("{name}: a declaration's key is indeterminate: {c:?}")
                            }
                            Err(_) => {}
                        }
                    }
                }
                ids
            })
            .expect("with_input")
        })
        .unwrap_or_else(|| panic!("{}: the post-image has errors", case.name));
        for id in &want {
            assert!(
                got.contains(id),
                "{}: no declaration in the post-image has {}",
                case.name,
                short(id)
            );
        }
        checked += 1;
    }
    assert!(checked > 0, "no directory was checked");
}

/// The wire name of a reason (`spec/edits.md` §4.2).
fn reason_name(r: Reason) -> &'static str {
    match r {
        Reason::None => "none",
        Reason::MacroBody => "macro_body",
        Reason::Pasted => "pasted",
        Reason::Crossing => "crossing",
        Reason::NotAFile => "not_a_file",
        Reason::NotOwned => "not_owned",
        Reason::FileRecoded => "file_recoded",
        Reason::NotPlain => "not_plain",
        Reason::MixedSet => "mixed_set",
        Reason::MixedInstances => "mixed_instances",
        Reason::Directive => "directive",
    }
}

/// Whether the directory's `expect.txt` is marked `# pending`.
fn is_pending(case: &Case) -> bool {
    std::fs::read_to_string(case.dir.join("expect.txt"))
        .is_ok_and(|t| t.lines().any(|l| l.trim() == "# pending"))
}

/// Whether the AST names `d` (an identifier) `name`.
fn named(u: &Unit<'_>, d: DeclId, name: &[u8]) -> bool {
    let ast = u.ast();
    ast.decls.get(d).name().as_ident().is_some_and(|s| ast.symbols().as_str(s).as_bytes() == name)
}

/// The case's files with every `replace` claim of `e` spliced in.
fn post_image(case: &Case, e: &Edits) -> Vec<(String, Vec<u8>)> {
    let mut files = case.files.clone();
    for (p, bytes) in &mut files {
        let mut edits: Vec<(usize, usize, Vec<u8>)> = e
            .claims
            .iter()
            .filter(|c| {
                c.stance == Stance::Replace && e.files[c.file as usize].path == p.as_bytes()
            })
            .map(|c| {
                let text: Vec<u8> = c
                    .pieces
                    .iter()
                    .flat_map(|p| match p {
                        Piece::Literal(b) | Piece::Spelled { text: b, .. } => b.clone(),
                        Piece::Copy(_) => panic!("{}: a copy piece", case.name),
                    })
                    .collect();
                (c.lo as usize, c.hi as usize, text)
            })
            .collect();
        edits.sort_by_key(|e| std::cmp::Reverse(e.0));
        for (lo, hi, text) in edits {
            bytes.splice(lo..hi, text);
        }
    }
    files
}

/// The worker's rule (`docs/wp/WP-071.md` "Tests", the last bullet): a
/// diagnosed unit is not given to the rename; called on one all the same,
/// `run` returns without panicking.
#[test]
fn run_on_a_diagnosed_unit_does_not_panic() {
    let files =
        [("main.cpp", &b"namespace n { void f(); }\nvoid g() { n::f(); undeclared(); }\n"[..])];
    let tree = Tree::new(Path::new(env!("CARGO_TARGET_TMPDIR")), "diagnosed-run", &files).unwrap();
    let root = tree.root().to_str().unwrap().to_owned();
    let obs = tree.observer("main.cpp", "c++20", &[]);
    let case = Case {
        name: "diagnosed-run".into(),
        dir: PathBuf::new(),
        target: vec![b"n".to_vec(), b"f".to_vec()],
        kind: TargetKind::Function,
        new_name: b"g2".to_vec(),
        args: Vec::new(),
        files: Vec::new(),
    };
    let t = transform(&case, &root);
    let (_, r) = tree
        .parse(&obs, &written_options(), |u| {
            assert!(u.errors() > 0);
            let subject = |p: &[u8]| p.to_vec();
            slip_digest::map::with_input(u, &subject, |input| {
                let mut keys = Keys::new(input);
                let sites = slip_kiln::sites::Sites::new(u, &t.ownership);
                slip_refactor::run(&t, u, &mut keys, &|f| sites.file_ref(f))
                    .map(|e| e.subjects.len())
            })
        })
        .unwrap();
    let r = r.expect("parsed").expect("with_input");
    assert_eq!(r, Ok(1), "the seed is found in the diagnosed unit");
}

/// `Err` is a failure of Slip, never a fact about the subject: a request
/// that is not a rename, one that breaks §3 (a keyword new name, a new
/// name with a universal-character-name), a unit parsed without
/// `written` or without the preprocessing record.
#[test]
fn requests_the_rename_does_not_run() {
    let files = [("main.cpp", &b"namespace n { void f(); }\n"[..])];
    let tree = Tree::new(Path::new(env!("CARGO_TARGET_TMPDIR")), "requests", &files).unwrap();
    let root = tree.root().to_str().unwrap().to_owned();
    let obs = tree.observer("main.cpp", "c++20", &[]);
    let case = |new: &[u8]| Case {
        name: "requests".into(),
        dir: PathBuf::new(),
        target: vec![b"n".to_vec(), b"f".to_vec()],
        kind: TargetKind::Function,
        new_name: new.to_vec(),
        args: Vec::new(),
        files: Vec::new(),
    };
    let good = transform(&case(b"g"), &root);
    let keyword = transform(&case(b"delete"), &root);
    let ucn = transform(&case(b"caf\\u00e9"), &root);
    let mut script = good.clone();
    script.kind = TransformKind::Script;
    let results = |options: ParseOptions| {
        let (_, r) = tree
            .parse(&obs, &options, |u| {
                let subject = |p: &[u8]| p.to_vec();
                slip_digest::map::with_input(u, &subject, |input| {
                    let sites = slip_kiln::sites::Sites::new(u, &good.ownership);
                    let files = |f| sites.file_ref(f);
                    [&good, &keyword, &ucn, &script]
                        .map(|t| slip_refactor::run(t, u, &mut Keys::new(input), &files).err())
                })
            })
            .unwrap();
        r.unwrap().unwrap()
    };
    let [good_r, keyword_r, ucn_r, script_r] = results(written_options());
    assert_eq!(good_r, None);
    assert!(keyword_r.unwrap().contains("a keyword of C++"));
    assert!(ucn_r.unwrap().contains("not an identifier"));
    assert!(script_r.unwrap().contains("runs a rename"));
    let [plain, ..] = results(ParseOptions::default());
    assert!(plain.unwrap().contains("without ParseOptions::written"));
    // `name_taken` needs the preprocessing record (§9.5): a unit parsed
    // without it is refused, not run with the macro check skipped.
    let mut no_record = written_options();
    no_record.pp_record = false;
    let [no_record_r, ..] = results(no_record);
    assert!(no_record_r.unwrap().contains("without the preprocessing record"));
}

/// The key a rename's rule maps to has the old name (a sanity check of
/// the rule's direction, `spec/verify.md` §2.1).
#[test]
fn a_rule_maps_the_new_name_to_the_old() {
    let case = read_case(&cases_dir().join("s9_3-declaration"));
    let Ran::Edits(e) = run_case(&case) else { panic!("diagnosed") };
    let rule = e.subjects[0].rule.as_ref().expect("a rule");
    assert_eq!(rule.name, Name::Identifier(case.new_name.clone()));
    assert_eq!(rule.to_name, Name::Identifier(case.target.last().unwrap().clone()));
}

fn edits_of(dir: &str) -> Edits {
    let case = read_case(&cases_dir().join(dir));
    let Ran::Edits(e) = run_case(&case) else { panic!("{dir}: diagnosed") };
    *e
}

/// Review of WP-071, A4 (`spec/digest.md` §3.4): a friend declaration that
/// does not define the function is not followed by `origin`, so a header's
/// use spells the same key in a unit that instantiates the template before
/// the header and in one that instantiates it after. Two observers giving
/// two pieces at one range would make the reduce divergent.
#[test]
fn a_friend_that_does_not_define_gives_one_piece() {
    let at = |e: &Edits| {
        let c = e
            .claims
            .iter()
            .find(|c| e.files[c.file as usize].path == b"use.h")
            .expect("a claim in use.h");
        (c.lo, c.hi, c.stance, c.pieces.clone(), c.subjects.clone())
    };
    let before = edits_of("wp103-friend-not-defining-instantiated-before-use");
    let after = edits_of("wp103-friend-not-defining-instantiated-after-use");
    assert_eq!(at(&before), at(&after));
    let seed = before.subjects.iter().find(|s| s.role == Role::Seed).unwrap().id;
    assert!(matches!(&at(&before).3[..], [Piece::Spelled { key, .. }] if *key == seed));
}

/// Review of WP-071, A5 (`spec/edits.md` §9.2): whether a subject carries
/// a rule is a function of its key and the request, never of what else
/// the observer saw. The seed has its rule (an equal one) in the unit that
/// instantiates the class template defining it as a friend and in the one
/// that does not; where its key follows the pattern member, it is
/// `unsupported` instead.
#[test]
fn a_seed_rule_does_not_depend_on_instantiation() {
    let seed = |e: &Edits| e.subjects.iter().find(|s| s.role == Role::Seed).unwrap().clone();
    let inst = seed(&edits_of("wp103-friend-defining-instantiated"));
    let not = seed(&edits_of("wp103-friend-defining-not-instantiated"));
    assert_eq!(inst.id, not.id);
    assert!(inst.rule.is_some());
    assert_eq!(inst.rule, not.rule);
    assert_ne!(inst.flags & subject_flags::UNSUPPORTED, 0);
}
