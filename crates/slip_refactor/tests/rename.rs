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
use slip_kiln::kiln::ast::decl::FriendKind;
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

fn written_options() -> ParseOptions<'static> {
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
                Piece::Identifier { text, key: None } => format!("identifier({})", quoted(text)),
                Piece::Identifier { text, key: Some(key) } => {
                    format!("identifier({}, {})", short(key), quoted(text))
                }
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
        // Reduce's selection (§5.2) over this one observer: a run with a
        // follower partly selected (`mixed_follower`) is refused, and only
        // the selected subjects are renamed.
        let Some(selected) = selection(&e) else { continue };
        let want: Vec<Digest> = e
            .subjects
            .iter()
            .filter(|s| s.rule.is_some() && s.flags == 0 && selected.contains(&s.id))
            .map(|s| s.id)
            .collect();
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
                let mut insert = |keys: &mut Keys<'_>, e: DeclId| match keys.key_id(e) {
                    Ok(id) => {
                        ids.insert(id);
                    }
                    Err(NoKey::Indeterminate(c)) => {
                        panic!("{name}: a declaration's key is indeterminate: {c:?}")
                    }
                    Err(_) => {}
                };
                let ast = u.ast();
                for d in written::decl_names(u) {
                    let e = written::origin(u, d.decl);
                    if named(u, e, &new_name) {
                        insert(&mut keys, e);
                        // The followers that have no name token of their
                        // own (WP-072): a using-declaration's shadows, and
                        // an unnamed class or enumeration the declaration
                        // names.
                        for s in ast.using_shadows(d.decl) {
                            insert(&mut keys, s);
                        }
                        if let Some(t) = ast.decls.get(e).ty().and_then(|q| unnamed_tag(u, q)) {
                            insert(&mut keys, t);
                        }
                    }
                }
                // The followers no written name stands for: the members
                // of instantiated specialisations (`docs/wp/WP-119.md`
                // item 4: no longer flagged, so their rules are asked
                // here, under which their keys must be determinate and
                // their ids those of the post-image).
                for d in ast.decls.ids() {
                    if named(u, d, &new_name) {
                        let e = keys.entity(d);
                        insert(&mut keys, e);
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

/// The subjects reduce selects in one observer's `e` (§5.2: the seeds,
/// closed under the links, and every follower all of whose `of` is
/// selected), or `None` when a follower is partly selected
/// (`mixed_follower`).
fn selection(e: &Edits) -> Option<BTreeSet<Digest>> {
    let mut selected: BTreeSet<Digest> =
        e.subjects.iter().filter(|s| s.role == Role::Seed).map(|s| s.id).collect();
    loop {
        let before = selected.len();
        for l in &e.links {
            if selected.contains(&l.a) || selected.contains(&l.b) {
                selected.insert(l.a);
                selected.insert(l.b);
            }
        }
        for s in e.subjects.iter().filter(|s| s.role == Role::Follower) {
            if !s.of.is_empty() && s.of.iter().all(|o| selected.contains(o)) {
                selected.insert(s.id);
            }
        }
        if selected.len() == before {
            break;
        }
    }
    let mixed = e.subjects.iter().filter(|s| s.role == Role::Follower).any(|s| {
        s.of.iter().any(|o| selected.contains(o)) && !s.of.iter().all(|o| selected.contains(o))
    });
    if mixed { None } else { Some(selected) }
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

/// The unnamed class or enumeration a declaration of type `q` declares
/// an object of, through pointers, references and arrays.
fn unnamed_tag(u: &Unit<'_>, q: slip_kiln::kiln::ast::QualType) -> Option<DeclId> {
    use slip_kiln::kiln::ast::types::TypeView;
    let ast = u.ast();
    let mut q = ast.types.canonical(q);
    for _ in 0..64 {
        match ast.types.view(q.ty()) {
            TypeView::Record(d) | TypeView::Enum(d) => {
                let d = ast.first(d);
                return ast.decls.get(d).name().is_empty().then_some(d);
            }
            TypeView::Pointer(x) | TypeView::LValueRef(x) | TypeView::RValueRef(x) => {
                q = ast.types.canonical(x);
            }
            TypeView::ConstantArray(a) => q = ast.types.canonical(a.elem),
            TypeView::IncompleteArray(x) => q = ast.types.canonical(x),
            _ => return None,
        }
    }
    None
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
                        Piece::Literal(b)
                        | Piece::Spelled { text: b, .. }
                        | Piece::Identifier { text: b, .. } => b.clone(),
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
/// that does not. Where its key follows the pattern member it is no longer
/// `unsupported` (`docs/wp/WP-119.md` item 4): the digest accepts the rule.
#[test]
fn a_seed_rule_does_not_depend_on_instantiation() {
    let seed = |e: &Edits| e.subjects.iter().find(|s| s.role == Role::Seed).unwrap().clone();
    let inst = seed(&edits_of("wp103-friend-defining-instantiated"));
    let not = seed(&edits_of("wp103-friend-defining-not-instantiated"));
    assert_eq!(inst.id, not.id);
    assert!(inst.rule.is_some());
    assert_eq!(inst.rule, not.rule);
    assert_eq!(inst.flags, 0);
    assert_eq!(not.flags, 0);
}

/// `docs/wp/WP-119.md` item 4, the WP-073 fix round's F3 (`spec/edits.md`
/// §9.5, `unreachable`): the explicit instantiation after the member's
/// explicit specialisation was recorded nowhere by Kiln, and the post-image
/// kept `template void S<int>::f();`, which named no member. Since Kiln
/// WP-2470 (in the pin since WP-066) it has its declaration row: the name
/// is claimed and renamed, and the post-image compiles. The directory
/// stays `# pending` (its follower `unreachable`, what it expects).
#[test]
fn f3_post_image_compiles() {
    let case = cases()
        .into_iter()
        .find(|c| c.name == "s9_5-unreachable-explicit-instantiation-after-specialisation")
        .expect("the F3 directory");
    let Ran::Edits(e) = run_case(&case) else { panic!("F3 diagnosed") };
    let files = post_image(&case, &e);
    let main = &files.iter().find(|(p, _)| p == "main.cpp").unwrap().1;
    assert!(main.ends_with(b"template void S<int>::g(); }\n"), "{}", String::from_utf8_lossy(main));
    assert!(
        with_unit("f3-post", &files, &case.args, |_, _, _| ()).is_some(),
        "the F3 post-image does not compile"
    );
}

/// `docs/wp/WP-073.md` part C and `docs/wp/WP-119.md` item 4
/// (`spec/edits.md` §9.2, `spec/digest.md` §3.4): the followers of a member
/// of a class template carry no flag. In the post-image of every directory
/// of WP-073 (its `test.txt` names a part of the package or its fix round)
/// with followers and no flag, the seeds' rules alone give each subject's
/// id to a declaration named like the new name (the instantiated member
/// takes the pattern member's name as the rules map it); and with the
/// followers' own rules added, which the digest now accepts, no key is
/// indeterminate and every subject's id is still given (until WP-119 those
/// rules made it `key/rule-on-instantiated-member`, and the followers were
/// `unsupported` for it).
#[test]
fn followers_of_class_template_members_map_with_their_seed() {
    let mut checked = 0;
    for case in cases() {
        let request = std::fs::read_to_string(case.dir.join("test.txt")).unwrap();
        if is_pending(&case) || !(request.contains("WP-073 part") || request.contains("WP-073 fix"))
        {
            continue;
        }
        let Ran::Edits(e) = run_case(&case) else { continue };
        let followers: Vec<_> = e.subjects.iter().filter(|s| s.role == Role::Follower).collect();
        let only_this_flag = !followers.is_empty()
            && e.subjects.iter().all(|s| s.flags == 0)
            && e.claims.iter().all(|c| matches!(c.stance, Stance::Replace | Stance::Keep));
        if !only_this_flag {
            continue;
        }
        let rules = |with_followers: bool| {
            let r: Vec<_> = e
                .subjects
                .iter()
                .filter(|s| with_followers || s.role != Role::Follower)
                .filter_map(|s| s.rule.clone())
                .collect();
            RuleTable::new(r).expect("a rule table")
        };
        let files = post_image(&case, &e);
        let new_name = case.new_name.clone();
        let walk = |table: RuleTable, tag: &str| {
            let name = format!("{}-{tag}", case.name);
            with_unit(&name, &files, &case.args, |u, _, root| {
                let prefix = format!("{root}/");
                let subject = |p: &[u8]| p.strip_prefix(prefix.as_bytes()).unwrap_or(p).to_vec();
                slip_digest::map::with_input(u, &subject, |input| {
                    let mut keys = Keys::with_rules(input, &table, slip_digest::ReadOnly::NO_FILE);
                    let mut ids = BTreeSet::new();
                    let mut causes = Vec::new();
                    for d in u.ast().decls.ids() {
                        if !named(u, d, &new_name) {
                            continue;
                        }
                        match keys.key_id(keys.entity(d)) {
                            Ok(id) => {
                                ids.insert(id);
                            }
                            Err(NoKey::Indeterminate(c)) => {
                                // Whether the entity is a friend a class
                                // template defines, instantiated here.
                                let ast = u.ast();
                                let friend = ast.redecls_of(keys.entity(d)).any(|x| {
                                    ast.instantiated_from(x).is_some()
                                        && ast.decls.get(x).misc().friend() != FriendKind::None
                                });
                                let tag = if friend { " (instantiated friend)" } else { "" };
                                causes.push(format!("{c:?}{tag}"));
                            }
                            Err(_) => {}
                        }
                    }
                    (ids, causes)
                })
                .expect("with_input")
            })
            .unwrap_or_else(|| panic!("{}: the post-image has errors", case.name))
        };
        // Where the seed is itself an instantiated member (a friend the
        // class template defines, redeclared at namespace scope: F4), the
        // seeds' rules alone put a rule on an instantiated member with none
        // on its pattern: indeterminate, as it should be, only for that
        // cause and only on such a friend; the pattern's rule (a
        // follower's) beside it, below, makes it the key following gives.
        let (ids, causes) = walk(rules(false), "seeds");
        if causes.is_empty() {
            for s in &e.subjects {
                assert!(ids.contains(&s.id), "{}: no declaration has {}", case.name, short(&s.id));
            }
        } else {
            assert!(
                causes.iter().all(|c| {
                    c.contains("rule-on-instantiated-member")
                        && c.ends_with("(instantiated friend)")
                }),
                "{}: indeterminate under the seeds' rules: {causes:?}",
                case.name
            );
        }
        let (ids, causes) = walk(rules(true), "all");
        assert!(
            causes.is_empty(),
            "{}: indeterminate under every subject's rule: {causes:?}",
            case.name
        );
        for s in &e.subjects {
            assert!(
                ids.contains(&s.id),
                "{}: with every rule, no declaration has {}",
                case.name,
                short(&s.id)
            );
        }
        checked += 1;
    }
    assert!(checked >= 6, "only {checked} directories with followers were checked");
}

/// `docs/wp/WP-073.md` part C, with §4.1: a seed that is a member of a
/// class template has one rule and no flag whether or not the observer
/// instantiates the class; the followers come only where it does.
#[test]
fn a_class_template_member_seed_does_not_depend_on_instantiation() {
    let inst = edits_of("s9_2-class-template-member-function");
    let not = edits_of("s9_2-class-template-member-not-instantiated");
    let seed = |e: &Edits| e.subjects.iter().find(|s| s.role == Role::Seed).unwrap().clone();
    let (a, b) = (seed(&inst), seed(&not));
    assert_eq!((a.id, &a.rule, a.flags), (b.id, &b.rule, b.flags));
    assert!(a.rule.is_some() && a.flags == 0);
    let followers = |e: &Edits| e.subjects.iter().filter(|s| s.role == Role::Follower).count();
    assert_eq!((followers(&inst), followers(&not)), (1, 0));
    assert!(inst.subjects.iter().all(|s| s.role != Role::Follower || s.of == [a.id]));
}

// ---------------------------------------------------------------------
// Families across observers (`docs/wp/WP-072.md` "Tests", the last
// paragraph): two observers' sections reduced (`slip_reduce`, §5.2).

/// The shards of `mains`, each parsed as an observer of one tree of
/// `files`, with the run of `t` over it as its `edits` section.
fn map_shards(
    name: &str,
    files: &[(&str, &[u8])],
    mains: &[&str],
    t: &dyn Fn(&str) -> Transform,
) -> (Transform, Vec<slip_model::Shard>) {
    let tree = Tree::new(Path::new(env!("CARGO_TARGET_TMPDIR")), name, files).unwrap();
    let root = tree.root().to_str().unwrap().to_owned();
    let t = t(&root);
    let mut shards = Vec::new();
    for main in mains {
        let obs = tree.observer(main, "c++20", &[]);
        let (mut shard, r) = tree
            .parse(&obs, &written_options(), |u| {
                assert_eq!(u.errors(), 0, "{name}: {main} has errors");
                let prefix = format!("{root}/");
                let subject = |p: &[u8]| p.strip_prefix(prefix.as_bytes()).unwrap_or(p).to_vec();
                slip_digest::map::with_input(u, &subject, |input| {
                    let mut keys = Keys::new(input);
                    let sites = slip_kiln::sites::Sites::new(u, &t.ownership);
                    slip_refactor::run(&t, u, &mut keys, &|f| sites.file_ref(f)).expect("run")
                })
                .expect("with_input")
            })
            .unwrap();
        shard.edits = Some(r.expect("parsed"));
        shards.push(shard);
    }
    (t, shards)
}

/// The shards in plan order (`slip_reduce::Shards`).
struct Plan(Vec<slip_model::Shard>);

impl slip_reduce::Shards for Plan {
    fn open(&self, index: usize) -> Result<Option<Vec<u8>>, String> {
        Ok(self.0.get(index).map(slip_model::Shard::to_bytes))
    }
    fn len(&self) -> usize {
        self.0.len()
    }
    fn observer(&self, index: usize) -> Digest {
        self.0[index].observer
    }
}

/// Reduces the shards of `t`.
fn reduced(name: &str, t: &Transform, shards: Vec<slip_model::Shard>) -> slip_reduce::Reduced {
    let id = slip_model::formats::content_id(&t.to_bytes()).unwrap();
    let scratch = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("{name}-scratch"));
    std::fs::create_dir_all(&scratch).unwrap();
    let options = slip_reduce::Options::new(scratch.clone());
    let r = slip_reduce::reduce(t, id, Digest([0x50; 32]), 1, &Plan(shards), &options)
        .unwrap_or_else(|e| panic!("{name}: {e}"));
    let _ = std::fs::remove_dir(&scratch);
    r
}

/// Two observers that share only the base class: one sees the seed
/// `D1::f` overriding `B::f`, the other `D2::f` overriding `B::f` and an
/// unrelated `X::f`. The request names `n::D1::f`.
fn family_over_two_observers(name: &str) -> (Vec<slip_model::Shard>, slip_reduce::Reduced) {
    let files: [(&str, &[u8]); 3] = [
        ("base.h", b"namespace n { struct B { virtual void f(); }; }\n"),
        ("a.cpp", b"#include \"base.h\"\nnamespace n { struct D1 : B { void f() override; }; }\nvoid ha(n::D1& d) { d.f(); }\n"),
        ("b.cpp", b"#include \"base.h\"\nnamespace n { struct D2 : B { void f() override; }; }\nnamespace n { struct X { virtual void f(); }; }\nvoid hb(n::B& b, n::X& x) { b.f(); x.f(); }\n"),
    ];
    let case = |root: &str| {
        let c = Case {
            name: name.into(),
            dir: PathBuf::new(),
            target: vec![b"n".to_vec(), b"D1".to_vec(), b"f".to_vec()],
            kind: TargetKind::Function,
            new_name: b"g".to_vec(),
            args: Vec::new(),
            files: Vec::new(),
        };
        transform(&c, root)
    };
    let (t, shards) = map_shards(name, &files, &["a.cpp", "b.cpp"], &case);
    let r = reduced(name, &t, shards.clone());
    (shards, r)
}

/// The subject of `shard` displayed as `display`.
fn subject_named(shard: &slip_model::Shard, display: &str) -> slip_model::edits::Subject {
    let e = shard.edits.as_ref().unwrap();
    e.subjects.iter().find(|s| s.display == display).unwrap_or_else(|| panic!("{display}")).clone()
}

/// The family is selected whole: the second observer has no seed, and its
/// `D2::f` is reached from the seed through `B::f`, which both observers
/// link. Every declaration of the three is edited.
#[test]
fn a_family_split_over_two_observers_is_selected_whole() {
    let (shards, r) = family_over_two_observers("family-two-observers-whole");
    let (a, b) = (&shards[0], &shards[1]);
    let d1 = subject_named(a, "n::D1::f()");
    let base = subject_named(a, "n::B::f()");
    let d2 = subject_named(b, "n::D2::f()");
    assert_eq!(d1.role, Role::Seed);
    assert_eq!(subject_named(b, "n::B::f()").role, Role::Candidate);
    assert!(!b.edits.as_ref().unwrap().subjects.iter().any(|s| s.role == Role::Seed));
    let set = &r.edits;
    assert_eq!(set.state, slip_model::editset::SetState::Accepted, "{:?}", set.refusals);
    let selected: BTreeSet<Digest> = set.subjects.iter().map(|s| s.id).collect();
    for id in [d1.id, base.id, d2.id] {
        assert!(selected.contains(&id), "{} not selected", short(&id));
    }
    // Each of the three is renamed where it is declared.
    for (path, id) in [("base.h", base.id), ("a.cpp", d1.id), ("b.cpp", d2.id)] {
        let edited = set.edits.iter().any(|e| {
            set.files[e.file as usize].path.ends_with(format!("/{path}").as_bytes())
                && e.subjects.contains(&id)
        });
        assert!(edited, "{path}: no edit of {}", short(&id));
    }
    assert!(r.declared.is_some());
}

/// An unrelated family is not selected: `X::f` is a candidate of the
/// second observer with no link to the seed's family, so its claims are
/// `keep`s of the edit set and nothing of it is edited or ruled.
#[test]
fn an_unrelated_family_is_not_selected_and_kept() {
    let (shards, r) = family_over_two_observers("family-two-observers-unrelated");
    let b = &shards[1];
    let x = subject_named(b, "n::X::f()");
    assert_eq!(x.role, Role::Candidate);
    let eb = b.edits.as_ref().unwrap();
    assert!(!eb.links.iter().any(|l| l.a == x.id || l.b == x.id), "X::f is linked");
    let set = &r.edits;
    assert_eq!(set.state, slip_model::editset::SetState::Accepted, "{:?}", set.refusals);
    assert!(!set.subjects.iter().any(|s| s.id == x.id), "X::f selected");
    assert!(!set.edits.iter().any(|e| e.subjects.contains(&x.id)), "X::f edited");
    // Every claim the second observer made for X::f is kept.
    let file = |i: u64| eb.files[i as usize].path.clone();
    let mut claimed = 0;
    for c in eb.claims.iter().filter(|c| c.subjects.contains(&x.id)) {
        claimed += 1;
        let kept = set.kept.iter().any(|p| {
            set.files[p.file as usize].path == file(c.file) && (p.lo, p.hi) == (c.lo, c.hi)
        });
        assert!(kept, "the claim at {}..{} is not kept", c.lo, c.hi);
    }
    assert_eq!(claimed, 2, "X::f's declaration and its call");
    let rules = r.declared.as_ref().map(|d| d.rules().rules().len());
    assert_eq!(rules, Some(3), "the three functions of the family, and no other");
}

// ---------------------------------------------------------------------
// End to end (`docs/wp/WP-120.md` "Tests"): the shards of every observer
// reduced, the accepted edits applied, and each observer's pre-image
// (digested with no rule) compared with its post-image (digested under
// the run's rules), unit by unit, as verification compares them
// (`spec/verify.md` §1, `spec/digest.md` §3.4).

/// What one end-to-end run gave.
#[derive(Debug)]
struct EndToEnd {
    /// Reduce's refusals of subjects (`cause display`) and of sites
    /// (`cause`), sorted.
    refusals: Vec<String>,
    /// Per observer (`main: kind name`): the units whose digest differs,
    /// is only on one side, or is indeterminate on either. Empty: the
    /// rename verifies.
    differences: Vec<String>,
}

/// A tree's files: path and bytes.
type Image = Vec<(String, Vec<u8>)>;

/// The shards of `mains` over `files`, reduced, applied, and verified;
/// `wrong` changes the post-image before it is digested (the wrong edit
/// beside a right one). Every rule of the run is kept: a rule on a member
/// of an instantiated specialisation is the digest's to accept
/// (`docs/wp/WP-119.md` item 1; until then the tests ran with those rules
/// deleted, `Wp119::Simulated`). Besides the units, each observer's rule
/// use is checked: a rule whose `direct` is not 0 there makes the
/// observer stale (`spec/verify.md` §9.2), a difference `stale <name>`.
fn end_to_end(
    name: &str,
    files: &[(&str, &[u8])],
    mains: &[&str],
    target: &[&str],
    new: &str,
    wrong: &dyn Fn(&mut Image),
) -> EndToEnd {
    let request = |root: &str| {
        let c = Case {
            name: name.into(),
            dir: PathBuf::new(),
            target: target.iter().map(|c| c.as_bytes().to_vec()).collect(),
            kind: TargetKind::Function,
            new_name: new.as_bytes().to_vec(),
            args: Vec::new(),
            files: Vec::new(),
        };
        transform(&c, root)
    };
    let (t, shards) = map_shards(name, files, mains, &request);
    let r = reduced(name, &t, shards);
    let set = &r.edits;
    let display = |id: &Digest| {
        set.subjects.iter().find(|s| s.id == *id).map_or_else(|| short(id), |s| s.display.clone())
    };
    let mut refusals: Vec<String> = set
        .refusals
        .iter()
        .map(|x| match &x.subject {
            Some(id) => format!("{:?} {}", x.cause, display(id)),
            None => format!("{:?}", x.cause),
        })
        .collect();
    refusals.sort();
    refusals.dedup();
    if set.state != slip_model::editset::SetState::Accepted {
        return EndToEnd { refusals, differences: Vec::new() };
    }
    let mut post = applied(name, set, files);
    wrong(&mut post);
    let rules: Vec<_> = set.subjects.iter().filter_map(|s| s.rule.clone()).collect();
    let table = RuleTable::new(rules).expect("a rule table");
    let none = RuleTable::new(Vec::new()).unwrap();
    let pre: Vec<(String, Vec<u8>)> =
        files.iter().map(|(p, b)| ((*p).to_owned(), b.to_vec())).collect();
    let mut differences = Vec::new();
    for main in mains {
        let (a, _) = digest_units(&format!("{name}-pre"), &pre, main, &none);
        let (b, uses) = digest_units(&format!("{name}-post"), &post, main, &table);
        for u in uses.iter().filter(|u| u.direct != 0) {
            let rule = &table.rules()[usize::try_from(u.rule).unwrap()];
            if rule.kind != slip_model::keys::KeyKind::Namespace {
                differences.push(format!("{main}: stale {:?}", rule.to_name));
            }
        }
        for (key, (digest, indeterminate, n)) in &a {
            match b.get(key) {
                None => differences.push(format!("{main}: removed {n}")),
                Some((d2, i2, _)) if d2 != digest || *indeterminate || *i2 => {
                    differences.push(format!("{main}: changed {n}"));
                }
                Some(_) => {}
            }
        }
        for (key, (_, _, n)) in &b {
            if !a.contains_key(key) {
                differences.push(format!("{main}: added {n}"));
            }
        }
    }
    EndToEnd { refusals, differences }
}

/// The units of an observer: key to digest, whether it is indeterminate,
/// and its name.
type Units = std::collections::BTreeMap<Digest, (Digest, bool, String)>;

/// Every unit of observer `main` of the tree `files`, digested under
/// `rules` (every file owned), and how each rule was used
/// (`spec/verify.md` §9.1).
fn digest_units(
    name: &str,
    files: &[(String, Vec<u8>)],
    main: &str,
    rules: &RuleTable,
) -> (Units, Vec<slip_model::agreement::Use>) {
    let refs: Vec<(&str, &[u8])> = files.iter().map(|(p, b)| (p.as_str(), b.as_slice())).collect();
    let tree = Tree::new(Path::new(env!("CARGO_TARGET_TMPDIR")), name, &refs).unwrap();
    let root = tree.root().to_str().unwrap().to_owned();
    let obs = tree.observer(main, "c++20", &[]);
    let (_, r) = tree
        .parse(&obs, &written_options(), |u| {
            assert_eq!(u.errors(), 0, "{name}: {main} has errors");
            let prefix = format!("{root}/");
            let subject = |p: &[u8]| p.strip_prefix(prefix.as_bytes()).unwrap_or(p).to_vec();
            slip_digest::map::with_input(u, &subject, |input| {
                let out = slip_digest::digest(
                    input,
                    &slip_digest::Options {
                        rules,
                        read_only: slip_digest::ReadOnly::NO_FILE,
                        detail: slip_digest::Detail::All,
                        names: true,
                        rule_use: !rules.is_empty(),
                    },
                );
                let names: std::collections::BTreeMap<Digest, String> =
                    out.names.into_iter().collect();
                let mut units = std::collections::BTreeMap::new();
                for g in &out.digests.groups {
                    for unit in g.units.as_deref().unwrap_or(&[]) {
                        let n = names.get(&unit.key).cloned().unwrap_or_else(|| short(&unit.key));
                        units.insert(unit.key, (unit.digest, unit.is_indeterminate(), n));
                    }
                }
                (units, out.rule_use)
            })
            .expect("with_input")
        })
        .unwrap();
    r.expect("parsed")
}

/// The post-image: every accepted edit of `set` spliced into its file.
fn applied(name: &str, set: &slip_model::editset::EditSet, files: &[(&str, &[u8])]) -> Image {
    let mut post: Image = files.iter().map(|(p, b)| ((*p).to_owned(), b.to_vec())).collect();
    for (path, bytes) in &mut post {
        let suffix = format!("/{path}");
        let mut edits: Vec<(usize, usize, Vec<u8>)> = set
            .edits
            .iter()
            .filter(|e| set.files[e.file as usize].path.ends_with(suffix.as_bytes()))
            .map(|e| {
                let text: Vec<u8> = e
                    .pieces
                    .iter()
                    .flat_map(|p| match p {
                        Piece::Literal(b)
                        | Piece::Spelled { text: b, .. }
                        | Piece::Identifier { text: b, .. } => b.clone(),
                        Piece::Copy(_) => panic!("{name}: a copy piece"),
                    })
                    .collect();
                (e.lo as usize, e.hi as usize, text)
            })
            .collect();
        edits.sort_by_key(|e| std::cmp::Reverse(e.0));
        for (lo, hi, text) in edits {
            bytes.splice(lo..hi, text);
        }
    }
    post
}

/// No change to the post-image.
fn right(_: &mut Image) {}

/// In `path` of the post-image, `from` (once) replaced by `to`.
fn edit_back(post: &mut [(String, Vec<u8>)], path: &str, from: &str, to: &str) {
    let (_, bytes) = post.iter_mut().find(|(p, _)| p == path).expect(path);
    let text = String::from_utf8(bytes.clone()).unwrap();
    assert!(text.contains(from), "{path}: no {from:?} in {text}");
    *bytes = text.replacen(from, to, 1).into_bytes();
}

/// `docs/wp/WP-120.md` item 1 (a), end to end: `struct D : B<int>` and an
/// observer that sees the base only. The rename verifies in both observers
/// with every rule kept, those of the members of `B<int>` and `B<long>`
/// included (`docs/wp/WP-119.md` item 1: until then WP-073's flag refused
/// the run, `SubjectUnsupported n::B<int>::f()` and `n::B<long>::f()`); and
/// the overrider left behind (the wrong edit) is a difference: the digest
/// writes what a function overrides (`spec/digest-kinds.md` `overrides`).
#[test]
fn wp120_override_of_an_instantiated_member_end_to_end() {
    let files: [(&str, &[u8]); 3] = [
        ("b.h", b"namespace n { template <class T> struct B { virtual void f(); }; }\n"),
        ("d.cpp", b"#include \"b.h\"\nnamespace n { struct D : B<int> { void f() override; }; }\nvoid h(n::D& d, n::B<int>& b) { d.f(); b.f(); }\n"),
        ("base.cpp", b"#include \"b.h\"\nvoid k(n::B<long>* b) { b->f(); }\n"),
    ];
    let run = |name: &str, wrong: &dyn Fn(&mut Image)| {
        end_to_end(name, &files, &["d.cpp", "base.cpp"], &["n", "D", "f"], "g", wrong)
    };
    let r = run("wp120-a-right", &right);
    assert!(r.refusals.is_empty() && r.differences.is_empty(), "{r:?}");
    let w = run("wp120-a-wrong", &|post| {
        edit_back(post, "d.cpp", "void g() override;", "void f();");
    });
    assert!(w.differences.iter().any(|d| d.contains("n::D")), "{w:?}");
}

/// Item 1 (b), end to end: an overrider in a class template with a base
/// that is not dependent; one observer instantiates it, one sees the base
/// only. Not instantiated, it verifies; instantiated, it verifies too, with
/// the rule of `E<int>::f` kept (`docs/wp/WP-119.md` item 1: until then
/// refused, `SubjectUnsupported n::E<int>::f()`).
#[test]
fn wp120_override_in_a_class_template_end_to_end() {
    let base: (&str, &[u8]) = ("base.h", b"namespace n { struct Base { virtual void f(); }; }\n");
    let e: (&str, &[u8]) = ("e.h", b"#include \"base.h\"\nnamespace n { template <class T> struct E : Base { void f() override; }; }\n");
    let files = [
        base,
        e,
        ("use.cpp", b"#include \"e.h\"\nvoid h(n::E<int>& x, n::Base& b) { x.f(); b.f(); }\n"),
        ("only.cpp", b"#include \"base.h\"\nvoid k(n::Base& b) { b.f(); }\n"),
        ("pattern.cpp", b"#include \"e.h\"\n"),
    ];
    let target = ["n", "Base", "f"];
    let not = end_to_end("wp120-b-not", &files, &["pattern.cpp", "only.cpp"], &target, "g", &right);
    assert!(not.refusals.is_empty() && not.differences.is_empty(), "{not:?}");
    let r = end_to_end("wp120-b-right", &files, &["use.cpp", "only.cpp"], &target, "g", &right);
    assert!(r.refusals.is_empty() && r.differences.is_empty(), "{r:?}");
    let w = end_to_end("wp120-b-wrong", &files, &["pattern.cpp"], &target, "g", &|post| {
        edit_back(post, "e.h", "void g() override;", "void f();");
    });
    assert!(!w.differences.is_empty(), "{w:?}");
}

/// Item 1 (c), end to end: a using-declaration in a class template's
/// pattern. Instantiated, the using-declaration of `F<int>` follows the
/// pattern's, with a rule the digest accepts (`docs/wp/WP-119.md` item 1:
/// until then refused, `SubjectUnsupported n::F<int>::f`), and its shadow,
/// not linked by Kiln, has a rule of its own. Both observers verify.
#[test]
fn wp120_using_declaration_in_a_class_template_end_to_end() {
    let files: [(&str, &[u8]); 3] = [
        ("f.h", b"namespace n { struct Base { void f(); }; template <class T> struct F : Base { using Base::f; }; }\n"),
        ("use.cpp", b"#include \"f.h\"\nvoid h(n::F<int>& x) { x.f(); }\n"),
        ("pattern.cpp", b"#include \"f.h\"\n"),
    ];
    let target = ["n", "Base", "f"];
    let not = end_to_end("wp120-c-not", &files, &["pattern.cpp"], &target, "g", &right);
    assert!(not.refusals.is_empty() && not.differences.is_empty(), "{not:?}");
    let r = end_to_end("wp120-c-right", &files, &["use.cpp", "pattern.cpp"], &target, "g", &right);
    assert!(r.refusals.is_empty() && r.differences.is_empty(), "{r:?}");
}

/// Item 1, the dependent base: one observer instantiates `G<B>` (the
/// pattern's name replaced), one sees the pattern only (kept): reduce sees
/// the disagreement (`contested`), and nothing else since
/// `docs/wp/WP-119.md` item 1 (before it the member of `G<n::B>` added
/// WP-073's flag, `SubjectUnsupported n::G<n::B>::f()`).
#[test]
fn wp120_dependent_base_over_two_observers_is_contested() {
    let files: [(&str, &[u8]); 3] = [
        ("g.h", b"namespace n { struct B { virtual void f(); }; template <class T> struct G : T { void f() override; }; }\n"),
        ("use.cpp", b"#include \"g.h\"\nvoid h(n::G<n::B>& x) { x.f(); }\n"),
        ("pattern.cpp", b"#include \"g.h\"\n"),
    ];
    let r =
        end_to_end("wp120-g", &files, &["use.cpp", "pattern.cpp"], &["n", "B", "f"], "g", &right);
    assert_eq!(r.refusals, ["Contested n::G::f()"], "{r:?}");
}

/// The refusals of a run as their causes, sorted, without subjects.
fn causes(r: &EndToEnd) -> Vec<&str> {
    let mut c: Vec<&str> =
        r.refusals.iter().map(|x| x.split(' ').next().unwrap_or_default()).collect();
    c.dedup();
    c
}

/// WP-120 fix round, F1 and F2, under `docs/wp/WP-119.md` item 6: a hidden
/// shadow follows its target and the member that hides it, renamed together
/// or neither. F1 (the using-declaration renamed, `using B::g;`, target and
/// hider not) is accepted and verifies (it was `mixed_follower` while the
/// digest wrote hidden shadows); F2 (the hider renamed and the target not,
/// or the reverse) stays refused (`mixed_follower`): the hidden declaration
/// would become visible.
#[test]
fn wp120_hidden_shadow_target_and_hider_end_to_end() {
    let f1: [(&str, &[u8]); 1] = [(
        "main.cpp",
        b"namespace n {\nstruct A { virtual void f(); };\nstruct B : A { void f() override; void f(int); };\nstruct D : B { using B::f; void f(int); };\n}\nvoid h(n::D& d) { d.f(); d.f(1); }\n",
    )];
    let r = end_to_end("wp120-f1", &f1, &["main.cpp"], &["n", "A", "f"], "g", &right);
    assert!(r.refusals.is_empty() && r.differences.is_empty(), "{r:?}");
    let f2: [(&str, &[u8]); 1] = [(
        "main.cpp",
        b"namespace n {\nstruct B { void f(); void f(int); };\nstruct D : B { using B::f; void f(); };\n}\nvoid h(n::D& d) { d.f(); d.f(1); }\n",
    )];
    for (name, target) in
        [("wp120-f2-derived", ["n", "D", "f"]), ("wp120-f2-base", ["n", "B", "f"])]
    {
        let r = end_to_end(name, &f2, &["main.cpp"], &target, "g", &right);
        assert_eq!(causes(&r), ["MixedFollower"], "{name}: {r:?}");
    }
}

/// WP-120 fix round, question 3: a dependent using-declaration
/// instantiated with a base whose `f()` hides `X::f(int)`. Renamed alone,
/// the kept `using Base<T>::f;` (its name `unresolved`) would name
/// `X::f(int)`. Refused as `mixed_follower` on the instantiated
/// using-declaration (it follows the pattern's, which is not renamed),
/// with no WP-073 flag since `docs/wp/WP-119.md` item 4 (the review saw it
/// accepted with that flag lifted, and the digest show the shadow changed).
#[test]
fn wp120_dependent_using_partly_selected_end_to_end() {
    let files: [(&str, &[u8]); 1] = [(
        "main.cpp",
        b"namespace n {\nstruct X { void f(int); };\ntemplate <class U> struct Base : X { void f(); };\ntemplate <class T> struct F : Base<T> { using Base<T>::f; };\n}\nvoid h(n::F<int>& x) { x.f(); }\n",
    )];
    let r = end_to_end("wp120-q3", &files, &["main.cpp"], &["n", "Base", "f"], "g", &right);
    assert!(causes(&r).contains(&"MixedFollower"), "{r:?}");
}

/// WP-120 fix round, F4: a local class deriving from `T`, in a function
/// template and in a member function of an instantiated class template,
/// instantiated with a base in the family in one observer and not in the
/// other: kept in one, replaced in the other (`contested`).
#[test]
fn wp120_local_class_dependent_base_over_two_observers_is_contested() {
    let ft: [(&str, &[u8]); 3] = [
        ("t.h", b"namespace n { struct B { virtual void f(); }; }\ntemplate <class T> int t() { struct L : T { void f() override {} }; L l; l.f(); return 0; }\n"),
        ("use.cpp", b"#include \"t.h\"\nint u = t<n::B>();\n"),
        ("pattern.cpp", b"#include \"t.h\"\n"),
    ];
    let mt: [(&str, &[u8]); 3] = [
        ("t.h", b"namespace n { struct B { virtual void f(); }; }\ntemplate <class T> struct S { int t() { struct L : T { void f() override {} }; L l; l.f(); return 0; } };\n"),
        ("use.cpp", b"#include \"t.h\"\nint u = S<n::B>().t();\n"),
        ("pattern.cpp", b"#include \"t.h\"\nS<n::B> s;\n"),
    ];
    for (name, files) in [("wp120-f4-ft", &ft), ("wp120-f4-mt", &mt)] {
        let r = end_to_end(name, files, &["use.cpp", "pattern.cpp"], &["n", "B", "f"], "g", &right);
        assert_eq!(causes(&r), ["Contested"], "{name}: {r:?}");
        let one = end_to_end(
            &format!("{name}-one"),
            files,
            &["pattern.cpp"],
            &["n", "B", "f"],
            "g",
            &right,
        );
        assert!(one.refusals.is_empty() && one.differences.is_empty(), "{name}: {one:?}");
    }
}

/// Item 3, end to end: the unhide idiom renamed from the base verifies,
/// and the overrider left behind is a difference; renamed from the derived
/// class it verifies too since `docs/wp/WP-119.md` item 6 (the hidden
/// shadow follows `B::f()` and its hider, both renamed; before, the kept
/// using-declaration made it `mixed_follower`), and, with the override
/// declared before the using-declaration, it verifies.
#[test]
fn wp120_unhide_idiom_end_to_end() {
    let idiom: [(&str, &[u8]); 1] = [(
        "main.cpp",
        b"namespace n {\nstruct B { virtual void f(); void f(int); };\nstruct D : B { using B::f; void f() override; };\n}\nvoid h(n::D& d) { d.f(); d.f(1); }\n",
    )];
    let base = end_to_end("wp120-u-base", &idiom, &["main.cpp"], &["n", "B", "f"], "g", &right);
    assert!(base.refusals.is_empty() && base.differences.is_empty(), "{base:?}");
    let wrong =
        end_to_end("wp120-u-wrong", &idiom, &["main.cpp"], &["n", "B", "f"], "g", &|post| {
            edit_back(post, "main.cpp", "void g() override;", "void f();");
        });
    assert!(!wrong.differences.is_empty(), "{wrong:?}");
    let derived =
        end_to_end("wp120-u-derived", &idiom, &["main.cpp"], &["n", "D", "f"], "g", &right);
    // The hidden shadow follows `B::f()` and its hider `D::f()`, both
    // renamed (`docs/wp/WP-119.md` item 6; it was `mixed_follower` while
    // the kept using-declaration was a party).
    assert!(derived.refusals.is_empty() && derived.differences.is_empty(), "{derived:?}");
    let before: [(&str, &[u8]); 1] = [(
        "main.cpp",
        b"namespace n {\nstruct B { virtual void f(); void f(int); };\nstruct D : B { void f() override; using B::f; };\n}\nvoid h(n::D& d) { d.f(); d.f(1); }\n",
    )];
    let b = end_to_end("wp120-u-before", &before, &["main.cpp"], &["n", "D", "f"], "g", &right);
    assert!(b.refusals.is_empty() && b.differences.is_empty(), "{b:?}");
    // A second derived class in which B::f() is not hidden: refused.
    let second: [(&str, &[u8]); 1] = [(
        "main.cpp",
        b"namespace n {\nstruct B { virtual void f(); void f(int); };\nstruct D : B { void f() override; using B::f; };\nstruct E : B { using B::f; };\n}\nvoid h(n::E& e) { e.f(); e.f(1); }\n",
    )];
    let s = end_to_end("wp120-u-second", &second, &["main.cpp"], &["n", "D", "f"], "g", &right);
    assert!(s.refusals.len() == 1 && s.refusals[0].starts_with("MixedFollower "), "{s:?}");
}

/// Item 4, the pair under the digest: a local class of a variable
/// template's initialiser, one of a function template and one of a
/// member function of a class template, each instantiated twice, are renamed with the pattern's candidate alone and
/// verify (the instantiated local classes' members follow the pattern
/// member, `spec/digest.md` §3.4); leaving the local class's name behind is
/// a difference.
#[test]
fn wp120_local_classes_of_templates_end_to_end() {
    let vt: [(&str, &[u8]); 1] = [(
        "main.cpp",
        b"namespace n { struct B { virtual void f(); }; }\ntemplate <class T> int v = [] { struct L : n::B { void f() override {} }; L l; l.f(); return 0; }();\nint x = v<int> + v<long>;\n",
    )];
    let ft: [(&str, &[u8]); 1] = [(
        "main.cpp",
        b"namespace n { struct B { virtual void f(); }; }\ntemplate <class T> int t() { struct L : n::B { void f() override {} }; L l; l.f(); return 0; }\nint u = t<int>() + t<long>();\n",
    )];
    let mt: [(&str, &[u8]); 1] = [(
        "main.cpp",
        b"namespace n { struct B { virtual void f(); }; }\ntemplate <class T> struct S { int t() { struct L : n::B { void f() override {} }; L l; l.f(); return 0; } };\nint u = S<int>().t() + S<long>().t();\n",
    )];
    for (name, files) in [("wp120-vt", &vt), ("wp120-ft", &ft), ("wp120-mt", &mt)] {
        let r = end_to_end(name, files, &["main.cpp"], &["n", "B", "f"], "g", &right);
        assert!(r.refusals.is_empty() && r.differences.is_empty(), "{name}: {r:?}");
        let w = end_to_end(
            &format!("{name}-wrong"),
            files,
            &["main.cpp"],
            &["n", "B", "f"],
            "g",
            &|post| {
                edit_back(post, "main.cpp", "void g() override {}", "void f() {}");
                edit_back(post, "main.cpp", "l.g()", "l.f()");
            },
        );
        assert!(!w.differences.is_empty(), "{name}: {w:?}");
    }
}

/// `docs/wp/WP-119.md` item 4 (b): a namespace-scope redeclaration of a
/// friend the class template defines, before or after the instantiation
/// (WP-073 fix round F4). Both orders rename and verify with every rule
/// kept; the redeclaration left behind is a difference in both.
#[test]
fn wp119_friend_redeclared_both_orders_end_to_end() {
    let after: [(&str, &[u8]); 1] = [(
        "main.cpp",
        b"namespace n { template <class T> struct S { friend void f(S) {} };\nS<int> s; void f(S<int>); void u() { f(s); } }\n",
    )];
    let before: [(&str, &[u8]); 1] = [(
        "main.cpp",
        b"namespace n { template <class T> struct S { friend void f(S) {} };\nvoid f(S<int>); S<int> s; void u() { f(s); } }\n",
    )];
    for (name, files) in [("wp119-f4-after", &after), ("wp119-f4-before", &before)] {
        let r = end_to_end(name, files, &["main.cpp"], &["n", "f"], "g", &right);
        assert!(r.refusals.is_empty() && r.differences.is_empty(), "{name}: {r:?}");
        let w =
            end_to_end(&format!("{name}-wrong"), files, &["main.cpp"], &["n", "f"], "g", &|post| {
                edit_back(post, "main.cpp", "void g(S<int>);", "void f(S<int>);");
            });
        assert!(!w.differences.is_empty(), "{name}: {w:?}");
    }
}

/// `docs/wp/WP-119.md` item 5, end to end: a friend the class template
/// defines, redeclared at namespace scope, seen by an observer that does
/// not instantiate the class (A) and one that does and calls it (B, where
/// a global template `f` would take the call if it were left behind).
/// Renamed, both verify; each wrong edit is a difference.
#[test]
fn wp119_friend_over_two_observers_end_to_end() {
    let files: [(&str, &[u8]); 3] = [
        (
            "s.h",
            b"namespace n { template <class T> struct S { friend int f(S) { return 0; } };\nint f(S<int>); }\n",
        ),
        ("a.cpp", b"#include \"s.h\"\n"),
        (
            "b.cpp",
            b"#include \"s.h\"\ntemplate <class T> int f(T) { return 1; }\nint u() { n::S<int> s; return f(s); }\n",
        ),
    ];
    let mains = ["a.cpp", "b.cpp"];
    let r = end_to_end("wp119-i5", &files, &mains, &["n", "f"], "g", &right);
    assert!(r.refusals.is_empty() && r.differences.is_empty(), "{r:?}");
    let wrongs: [(&str, &str, &str, &str); 3] = [
        ("wp119-i5-namespace-kept", "s.h", "int g(S<int>);", "int f(S<int>);"),
        ("wp119-i5-friend-kept", "s.h", "friend int g(S)", "friend int f(S)"),
        ("wp119-i5-call-kept", "b.cpp", "return g(s);", "return f(s);"),
    ];
    for (name, path, from, to) in wrongs {
        let w = end_to_end(name, &files, &mains, &["n", "f"], "g", &|post| {
            edit_back(post, path, from, to);
        });
        let stale_a = w.differences.iter().any(|d| d.starts_with("a.cpp: stale"));
        let b_differs =
            w.differences.iter().any(|d| d.starts_with("b.cpp: ") && !d.contains("stale"));
        match name {
            // A keeps `f(S<int>)` (or the friend pattern) under the name the
            // rule says was renamed: stale; B shows the function changed.
            "wp119-i5-namespace-kept" | "wp119-i5-friend-kept" => {
                assert!(stale_a && b_differs, "{name}: {w:?}");
            }
            // The call finds the global template: `u()` changed in B.
            _ => assert!(w.differences.iter().any(|d| d == "b.cpp: changed u()"), "{name}: {w:?}"),
        }
    }
    // The call left as `f` where no other `f` is found (ADL finds the
    // renamed `g` only): the post-image does not compile, which
    // verification refuses (the parse's errors; here the helper's assert).
    let alone: [(&str, &[u8]); 3] = [
        files[0],
        files[1],
        ("b.cpp", b"#include \"s.h\"\nint u() { n::S<int> s; return f(s); }\n"),
    ];
    let r = end_to_end("wp119-i5-alone", &alone, &mains, &["n", "f"], "g", &right);
    assert!(r.refusals.is_empty() && r.differences.is_empty(), "{r:?}");
    let failed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        end_to_end("wp119-i5-alone-call-kept", &alone, &mains, &["n", "f"], "g", &|post| {
            edit_back(post, "b.cpp", "return g(s);", "return f(s);");
        })
    }));
    assert!(failed.is_err(), "the post-image with the call left as `f` compiles");
}

/// `docs/wp/WP-119.md` item 6 under WP-120's N4: an instantiated
/// dependent using-declaration is looked at when a subject hides what it
/// names. `D<int>::f()` (a follower of the seed `n::D::f`) hides the
/// shadow of `Base<int>::f()`; renaming the hider and not the target would
/// make `Base<int>::f()` visible in `D<int>`: refused (`mixed_follower`).
#[test]
fn wp119_instantiated_using_hidden_by_a_subject_end_to_end() {
    let files: [(&str, &[u8]); 1] = [(
        "main.cpp",
        b"namespace n {\ntemplate <class T> struct Base { void f(); void f(int); };\ntemplate <class T> struct D : Base<T> { using Base<T>::f; void f(); };\n}\nvoid h(n::D<int>& d) { d.f(); d.f(1); }\n",
    )];
    let r = end_to_end("wp119-n4-inst", &files, &["main.cpp"], &["n", "D", "f"], "g", &right);
    assert!(causes(&r).contains(&"MixedFollower"), "{r:?}");
}

/// `docs/wp/WP-127.md` item 2, the refusing form (was WP-119's known
/// mismatch F1): a dependent name of a pattern that an instantiation binds
/// to a renamed entity is refused at the pattern's name (`site_refused`,
/// `mixed_instances`), in its three forms (`t.f()`, `T::f()`, `&T::f`),
/// with one instantiation and with two of which one binds elsewhere
/// (`n::R`). Before, the run was accepted and verification reported
/// `changed call<n::S<int>>(...)`.
#[test]
fn wp127_dependent_name_bound_by_an_instantiation_is_refused() {
    let forms: [(&str, &str, &str); 3] = [
        ("call", "template <class T> int call(T t) { return t.f(); }\n", "call(n::S<int>())"),
        ("static", "template <class T> int call() { return T::f(); }\n", "call<n::S<int>>()"),
        (
            "address",
            "template <class T> int call(T t) { int (T::*p)() = &T::f; return (t.*p)(); }\n",
            "call(n::S<int>())",
        ),
    ];
    for (name, pattern, use_s) in forms {
        let stat = if name == "static" { "static " } else { "" };
        let classes = format!(
            "namespace n {{\nstruct Base {{ {stat}int f() {{ return 2; }} }};\ntemplate <class T> struct S : Base {{ {stat}int f() {{ return 1; }} }};\nstruct R {{ {stat}int f() {{ return 3; }} }};\n}}\n"
        );
        let use_r = use_s.replace("n::S<int>", "n::R");
        for (tag, uses) in [
            ("one", format!("int u() {{ return {use_s}; }}\n")),
            ("two", format!("int u() {{ return {use_s} + {use_r}; }}\n")),
        ] {
            let text = format!("{classes}{pattern}{uses}");
            let files: [(&str, &[u8]); 1] = [("main.cpp", text.as_bytes())];
            let n = format!("wp127-dep-{name}-{tag}");
            let r = end_to_end(&n, &files, &["main.cpp"], &["n", "S", "f"], "g", &right);
            assert_eq!(causes(&r), ["SiteRefused"], "{n}: {r:?}");
        }
        // Instantiated with `n::R` only: nothing binds the name to a
        // subject, it stays `unresolved`, and the rename verifies.
        let text = format!("{classes}{pattern}int u() {{ return {use_r}; }}\n");
        let files: [(&str, &[u8]); 1] = [("main.cpp", text.as_bytes())];
        let n = format!("wp127-dep-{name}-elsewhere");
        let r = end_to_end(&n, &files, &["main.cpp"], &["n", "S", "f"], "g", &right);
        assert!(r.refusals.is_empty() && r.differences.is_empty(), "{n}: {r:?}");
    }
}

/// Item 2 over two observers of which only one instantiates the pattern
/// with `n::S<int>`, in both orders: refused whichever observer comes
/// first (the mutation "checked in the first observer only").
#[test]
fn wp127_dependent_name_two_observers_one_instantiates() {
    let files: [(&str, &[u8]); 3] = [
        (
            "t.h",
            b"namespace n {\nstruct Base { int f() { return 2; } };\ntemplate <class T> struct S : Base { int f() { return 1; } };\n}\ntemplate <class T> int call(T t) { return t.f(); }\n",
        ),
        ("use.cpp", b"#include \"t.h\"\nint u() { return call(n::S<int>()); }\n"),
        ("pattern.cpp", b"#include \"t.h\"\nint k() { return n::S<int>().f(); }\n"),
    ];
    for mains in [["use.cpp", "pattern.cpp"], ["pattern.cpp", "use.cpp"]] {
        let n = format!("wp127-dep-two-{}", mains[0].trim_end_matches(".cpp"));
        let r = end_to_end(&n, &files, &mains, &["n", "S", "f"], "g", &right);
        assert_eq!(causes(&r), ["SiteRefused"], "{n}: {r:?}");
    }
    // The observer that does not instantiate alone: accepted, and it
    // verifies (no observer makes the instantiation that would differ:
    // what the rename does not see, `spec/edits.md` §9.4).
    let r =
        end_to_end("wp127-dep-two-alone", &files, &["pattern.cpp"], &["n", "S", "f"], "g", &right);
    assert!(r.refusals.is_empty() && r.differences.is_empty(), "{r:?}");
}

/// `docs/wp/WP-127.md` item 1 (was WP-119's known mismatch F2): the
/// using-declaration is read by what its declarator names, not by its
/// shadows, so the order of the members does not matter. Every named
/// declaration renamed (`B::f()`, with its overrider `D::f()`): rewritten
/// to `using B::g;` and verifies, the overrider before the
/// using-declaration or after it.
#[test]
fn wp127_using_named_set_all_renamed_both_orders() {
    let member_first: [(&str, &[u8]); 1] = [(
        "main.cpp",
        b"namespace n {\nstruct A { void f(int); };\nstruct B : A { virtual void f(); };\nstruct D : B { void f() override; using B::f; };\n}\nvoid h(n::D& d) { d.f(); }\n",
    )];
    let using_first: [(&str, &[u8]); 1] = [(
        "main.cpp",
        b"namespace n {\nstruct A { void f(int); };\nstruct B : A { virtual void f(); };\nstruct D : B { using B::f; void f() override; };\n}\nvoid h(n::D& d) { d.f(); }\n",
    )];
    for seed in ["B", "D"] {
        for (order, files) in [("mf", &member_first), ("uf", &using_first)] {
            let n = format!("wp127-f2-{order}-{seed}");
            let r = end_to_end(&n, files, &["main.cpp"], &["n", seed, "f"], "g", &right);
            assert!(r.refusals.is_empty() && r.differences.is_empty(), "{n}: {r:?}");
        }
    }
}

/// Item 1, some named declarations renamed and some not: the name would
/// mean two things. Refused as `mixed_follower`, in both member orders:
/// (a) `B::f()` renamed through its family and `B::f(int)` not, both
/// introduced (the using-declaration's `of` holds both); (b) a
/// non-virtual `B::f()` hidden by `D::f()`: renaming one of them unhides
/// it (`D::f` or `B::f` the seed).
#[test]
fn wp127_using_named_set_mixed_both_orders() {
    let family = |d: &str| {
        format!(
            "namespace n {{\nstruct A {{ virtual void f(); }};\nstruct B : A {{ void f() override; void f(int); }};\nstruct D : B {{ {d} }};\n}}\nvoid h(n::D& d) {{ d.f(); d.f(1); }}\n"
        )
    };
    let hidden = |d: &str| {
        format!(
            "namespace n {{\nstruct B {{ void f(); void f(int); }};\nstruct D : B {{ {d} }};\n}}\nvoid h(n::D& d) {{ d.f(); d.f(1); }}\n"
        )
    };
    let cases = [
        ("a-mf", family("void f(long); using B::f;"), ["n", "A", "f"]),
        ("a-uf", family("using B::f; void f(long);"), ["n", "A", "f"]),
        ("b-mf-d", hidden("void f(); using B::f;"), ["n", "D", "f"]),
        ("b-uf-d", hidden("using B::f; void f();"), ["n", "D", "f"]),
        ("b-mf-b", hidden("void f(); using B::f;"), ["n", "B", "f"]),
        ("b-uf-b", hidden("using B::f; void f();"), ["n", "B", "f"]),
    ];
    for (tag, text, target) in &cases {
        let files: [(&str, &[u8]); 1] = [("main.cpp", text.as_bytes())];
        let n = format!("wp127-mixed-{tag}");
        let r = end_to_end(&n, &files, &["main.cpp"], target, "g", &right);
        assert_eq!(causes(&r), ["MixedFollower"], "{n}: {r:?}");
    }
}

/// Item 1 in a class template whose using-declaration names a dependent
/// base (`docs/wp/WP-120.md`'s rule: the pattern's name is `unresolved`,
/// the instantiated using-declaration follows the pattern's): the hider
/// `D<int>::f()` renamed, both member orders, refused (`mixed_follower`).
#[test]
fn wp127_using_named_set_dependent_base_both_orders() {
    for (tag, d) in [("mf", "void f(); using Base<T>::f;"), ("uf", "using Base<T>::f; void f();")] {
        let text = format!(
            "namespace n {{\ntemplate <class T> struct Base {{ void f(); void f(int); }};\ntemplate <class T> struct D : Base<T> {{ {d} }};\n}}\nvoid h(n::D<int>& d) {{ d.f(); d.f(1); }}\n"
        );
        let files: [(&str, &[u8]); 1] = [("main.cpp", text.as_bytes())];
        let n = format!("wp127-dep-base-{tag}");
        let r = end_to_end(&n, &files, &["main.cpp"], &["n", "D", "f"], "g", &right);
        assert!(causes(&r).contains(&"MixedFollower"), "{n}: {r:?}");
    }
}

/// The classes of the review's F1 and F3 (`spec/landed/edits/WP-127.md`):
/// `C`'s lookup of `f` is decided by dominance through the virtual base
/// `A` ([class.member.lookup]/6), which `member_lookup` does not
/// implement, so what `using C::f;` names cannot be told. `b1` is `B1`'s
/// members, `d` `D`'s.
fn dominance(b1: &str, d: &str, calls: &str) -> String {
    format!(
        "namespace n {{\nstruct A {{ void f(); }};\nstruct B1 : virtual A {{ {b1} }};\nstruct B2 : virtual A {{}};\nstruct C : B1, B2 {{}};\nstruct D : C {{ {d} }};\n}}\nvoid h(n::D& d) {{ {calls} }}\n"
    )
}

/// A chain `C0 <- C1 <- ... <- C65` deeper than `member_lookup`'s cap
/// (64), `C0` with `f()` and `f(int)`, `D : C65` with `d`.
fn deep_chain(d: &str) -> String {
    use std::fmt::Write as _;
    let mut s = String::from("namespace n {\nstruct C0 { void f(); void f(int); };\n");
    for i in 1..=65 {
        writeln!(s, "struct C{i} : C{} {{}};", i - 1).unwrap();
    }
    writeln!(s, "struct D : C65 {{ {d} }};\n}}\nvoid h(n::D& d) {{ d.f(); d.f(1); }}").unwrap();
    s
}

/// The review's F1: a using-declaration whose named set cannot be told
/// (bases give different sets; the depth cap) and a member of its class
/// named like it a subject is refused in both member orders. Member first
/// it was accepted and did not verify (`added n::D::<shadow n::B1::f()>`):
/// the follower was flagged `unsupported` but its `of` held the shadows
/// only, so it was never selected. Now the class's members named like it
/// are in `of` too.
#[test]
fn wp127_unknown_named_set_refused_both_orders() {
    let both = "void f(); void f(int);";
    let longs = "void f(long); void f(int);";
    let cases: [(&str, String, [&str; 3]); 8] = [
        (
            "dominance-mf",
            dominance(both, "void f(); using C::f;", "d.f(); d.f(1);"),
            ["n", "D", "f"],
        ),
        (
            "dominance-uf",
            dominance(both, "using C::f; void f();", "d.f(); d.f(1);"),
            ["n", "D", "f"],
        ),
        (
            "dominance-mf-b1",
            dominance(both, "void f(); using C::f;", "d.f(); d.f(1);"),
            ["n", "B1", "f"],
        ),
        (
            "dominance-uf-b1",
            dominance(both, "using C::f; void f();", "d.f(); d.f(1);"),
            ["n", "B1", "f"],
        ),
        (
            "long-mf",
            dominance(longs, "void f(long); using C::f;", "d.f(1L); d.f(1);"),
            ["n", "D", "f"],
        ),
        (
            "long-uf",
            dominance(longs, "using C::f; void f(long);", "d.f(1L); d.f(1);"),
            ["n", "D", "f"],
        ),
        ("deep-mf", deep_chain("void f(); using C65::f;"), ["n", "D", "f"]),
        ("deep-uf", deep_chain("using C65::f; void f();"), ["n", "D", "f"]),
    ];
    for (tag, text, target) in &cases {
        let files: [(&str, &[u8]); 1] = [("main.cpp", text.as_bytes())];
        let n = format!("wp127-unknown-{tag}");
        let r = end_to_end(&n, &files, &["main.cpp"], target, "g", &right);
        assert_eq!(causes(&r), ["MixedFollower"], "{n}: {r:?}");
    }
}

/// F1's shape in an instantiated class whose dependent base is the
/// dominance hierarchy (`instantiated_unfollowed_using` with a set that
/// cannot be told): refused in both member orders.
#[test]
fn wp127_unknown_named_set_instantiated_refused_both_orders() {
    for (tag, e) in [("mf", "void f(); using T::f;"), ("uf", "using T::f; void f();")] {
        let text = format!(
            "namespace n {{\nstruct A {{ void f(); }};\nstruct B1 : virtual A {{ void f(); void f(int); }};\nstruct B2 : virtual A {{}};\nstruct C : B1, B2 {{}};\ntemplate <class T> struct E : T {{ {e} }};\n}}\nvoid h(n::E<n::C>& e) {{ e.f(); e.f(1); }}\n"
        );
        let files: [(&str, &[u8]); 1] = [("main.cpp", text.as_bytes())];
        let n = format!("wp127-unknown-inst-{tag}");
        let r = end_to_end(&n, &files, &["main.cpp"], &["n", "E", "f"], "g", &right);
        assert_eq!(causes(&r), ["MixedFollower"], "{n}: {r:?}");
    }
}

/// The review's F2: a member hides a declaration the using-declarator
/// names only if it corresponds to it ([namespace.udecl]/14); where both
/// are non-template non-static member functions with non-dependent types
/// that is told exactly. `D::f(int)` (renamed with `A::f(int)`) does not
/// correspond to `B::f()` and is no hider of it, so the answer is one in
/// both orders: accepted, and it verifies (member first it was
/// `MixedFollower`).
#[test]
fn wp127_hider_corresponds_both_orders() {
    for (tag, d) in [
        ("mf", "void f(int) override; void f(); using B::f;"),
        ("uf", "using B::f; void f(int) override; void f();"),
    ] {
        let text = format!(
            "namespace n {{\nstruct A {{ virtual void f(int); }};\nstruct B {{ void f(); }};\nstruct D : A, B {{ {d} }};\n}}\nvoid h(n::D& d) {{ d.f(); d.f(1); }}\n"
        );
        let files: [(&str, &[u8]); 1] = [("main.cpp", text.as_bytes())];
        let n = format!("wp127-corresponds-{tag}");
        let r = end_to_end(&n, &files, &["main.cpp"], &["n", "A", "f"], "g", &right);
        assert!(r.refusals.is_empty() && r.differences.is_empty(), "{n}: {r:?}");
    }
}

/// The review's F3, a limit: lookup in `C` finds `B1::f()` by dominance,
/// which `member_lookup` does not implement, so renaming `n::B1::f` is
/// refused (the right edit is `using C::g;`). Recorded so that the day it
/// is built this test changes.
#[test]
fn wp127_dominance_limit_refused() {
    for (tag, d) in [("uf", "using C::f; void f(int);"), ("mf", "void f(int); using C::f;")] {
        let text = dominance("void f();", d, "d.f(); d.f(1);");
        let files: [(&str, &[u8]); 1] = [("main.cpp", text.as_bytes())];
        let n = format!("wp127-dominance-limit-{tag}");
        let r = end_to_end(&n, &files, &["main.cpp"], &["n", "B1", "f"], "g", &right);
        assert_eq!(causes(&r), ["MixedFollower"], "{n}: {r:?}");
    }
}

/// Item 1 over two observers in both orders, one of which includes the
/// using-declaration's class and uses it, the other the header only; and
/// a class template instantiated in one observer only. A class's members
/// and bases are in its definition, which is one in every observer that
/// sees it (ODR), so a non-template class's named set is one; what can
/// differ is whether an observer instantiates a template, and the
/// refusal of the one that does holds.
#[test]
fn wp127_named_set_two_observers() {
    let header = "namespace n {\nstruct A { void f(); };\nstruct B1 : virtual A { void f(); void f(int); };\nstruct B2 : virtual A {};\nstruct C : B1, B2 {};\nstruct D : C { void f(); using C::f; };\ntemplate <class T> struct E : T { void f(); using T::f; };\n}\n";
    for (tag, use_) in [
        ("class", "#include \"t.h\"\nvoid h(n::D& d) { d.f(); d.f(1); }\n"),
        ("template", "#include \"t.h\"\nvoid h(n::E<n::C>& e) { e.f(); e.f(1); }\n"),
    ] {
        let target: [&str; 3] = if tag == "class" { ["n", "D", "f"] } else { ["n", "E", "f"] };
        let files: [(&str, &[u8]); 3] = [
            ("t.h", header.as_bytes()),
            ("use.cpp", use_.as_bytes()),
            ("other.cpp", b"#include \"t.h\"\nint k() { return 0; }\n"),
        ];
        for mains in [["use.cpp", "other.cpp"], ["other.cpp", "use.cpp"]] {
            let n = format!("wp127-named-two-{tag}-{}", mains[0].trim_end_matches(".cpp"));
            let r = end_to_end(&n, &files, &mains, &target, "g", &right);
            assert_eq!(causes(&r), ["MixedFollower"], "{n}: {r:?}");
        }
    }
}

/// F1's family with `C : B2, B1` (the first base gives `A::f()`, the
/// second `B1::f()`) and `B1` declaring `f()` only, hidden by `D::f()`:
/// member first Kiln makes no shadow at all, so what `using C::f;` names
/// is empty and cannot be told. From `n::D::f` it is refused in both
/// orders (`unsupported` member first: the follower's `of` is `D::f()`
/// alone, renamed; `mixed_follower` using first, the hidden shadow of the
/// kept `B1::f()`). From `n::A::f` nothing of the rename is near it: it is
/// kept, and the rename verifies in both orders.
#[test]
fn wp127_unknown_named_set_nothing_shadowed() {
    let text = |d: &str| {
        format!(
            "namespace n {{\nstruct A {{ void f(); }};\nstruct B1 : virtual A {{ void f(); }};\nstruct B2 : virtual A {{}};\nstruct C : B2, B1 {{}};\nstruct D : C {{ {d} }};\n}}\nvoid h(n::D& d) {{ d.f(); }}\n"
        )
    };
    let mf = text("void f(); using C::f;");
    let uf = text("using C::f; void f();");
    for (tag, src, want) in [("mf", &mf, "SubjectUnsupported"), ("uf", &uf, "MixedFollower")] {
        let files: [(&str, &[u8]); 1] = [("main.cpp", src.as_bytes())];
        let n = format!("wp127-unknown-empty-{tag}");
        let r = end_to_end(&n, &files, &["main.cpp"], &["n", "D", "f"], "g", &right);
        assert_eq!(causes(&r), [want], "{n}: {r:?}");
        let n = format!("wp127-unknown-empty-{tag}-a");
        let r = end_to_end(&n, &files, &["main.cpp"], &["n", "A", "f"], "g", &right);
        assert!(r.refusals.is_empty() && r.differences.is_empty(), "{n}: {r:?}");
    }
}

/// A declaration without a shadow that no member of the class could hide
/// (`named_set`'s "introduced" arm): `using B2::f;` names `A::f()`, which
/// `using B1::f;` introduced before it, and `B2::f(int)`. It is in the
/// using-declaration's `of`, so renaming `B2::f(int)` and not `A::f()` is
/// `mixed_follower` (conservative: `A::f()` stays introduced by `using
/// B1::f;`, so `using B2::g;` would also be right).
#[test]
fn wp127_second_using_declarator_introduced_arm() {
    let text = "namespace n {\nstruct A { void f(); };\nstruct B1 : virtual A {};\nstruct B2 : virtual A { void f(int); using A::f; };\nstruct D : B1, B2 { using B1::f; using B2::f; };\n}\nvoid h(n::D& d) { d.f(); d.f(1); }\n";
    let files: [(&str, &[u8]); 1] = [("main.cpp", text.as_bytes())];
    let r = end_to_end("wp127-second-using", &files, &["main.cpp"], &["n", "B2", "f"], "g", &right);
    assert_eq!(causes(&r), ["MixedFollower"], "{r:?}");
}

// ---------------------------------------------------------------------
// WP-147 item 1 (`spec/landed/edits/WP-147.md`, rows R1 and R2): the
// package's end-to-end case through the map phase's rename and reduce,
// on this workspace's volume, which finds a name in any case and keeps
// the spelling it was asked with. Apply's half is
// `crates/slip_cli/tests/stages.rs`
// `s2_1_another_case_of_a_readonly_directory_refused` (it refuses,
// `not_canonical`).

/// Whether the volume of the trees finds a name in another case; says so
/// when it does not.
fn volume_folds(test: &str) -> bool {
    let p = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("one-spelling-probe-{}-{test}", std::process::id()));
    std::fs::write(&p, "x").unwrap();
    let upper = p.with_file_name(p.file_name().unwrap().to_string_lossy().to_uppercase());
    let folds = upper.exists();
    std::fs::remove_file(&p).unwrap();
    if !folds {
        eprintln!("{test}: skipped: the volume of the trees is case-sensitive");
    }
    folds
}

/// `n::f` renamed to `h` under a request owning the tree with `dep/`
/// read-only; `f` is declared in `dep/x.h`, which `a.cpp` includes as
/// `include`.
fn rename_of_a_dependency(name: &str, include: &str) -> (String, slip_reduce::Reduced) {
    let main = format!("#include \"{include}\"\nvoid g() {{ n::f(); }}\n");
    let files: [(&str, &[u8]); 2] =
        [("dep/x.h", b"namespace n { void f(); }\n"), ("a.cpp", main.as_bytes())];
    let root = std::cell::RefCell::new(String::new());
    let t = |r: &str| {
        r.clone_into(&mut root.borrow_mut());
        Transform {
            kind: TransformKind::Rename,
            ownership: Ownership {
                owned: vec![r.as_bytes().to_vec()],
                readonly: vec![format!("{r}/dep").into_bytes()],
            },
            indivisible: true,
            rename: Some(Rename {
                target: vec![b"n".to_vec(), b"f".to_vec()],
                kind: TargetKind::Function,
                new_name: b"h".to_vec(),
            }),
            script: None,
        }
    };
    let (t, shards) = map_shards(name, &files, &["a.cpp"], &t);
    let r = reduced(name, &t, shards);
    (root.into_inner(), r)
}

/// The edit set's files under `<root>/dep`, in any case (on this volume,
/// files the request makes read-only).
fn read_only_edited(root: &str, r: &slip_reduce::Reduced) -> Vec<String> {
    let dep = format!("{root}/dep/");
    let set = &r.edits;
    set.edits
        .iter()
        .map(|e| String::from_utf8_lossy(&set.files[e.file as usize].path).into_owned())
        .filter(|p| p.len() > dep.len() && p[..dep.len()].eq_ignore_ascii_case(&dep))
        .collect()
}

/// `spec/edits.md` §2.1, §9.5 (the control): `f` is declared in a file the
/// request makes read-only, and no edit of the edit set is in one.
#[test]
fn s2_1_rename_of_a_dependency_by_its_spelling() {
    if !volume_folds("by-spelling") {
        return;
    }
    let (root, r) = rename_of_a_dependency("one-spelling-dep", "dep/x.h");
    assert_eq!(read_only_edited(&root, &r), Vec::<String>::new(), "{:?}", r.edits.refusals);
}

/// `spec/edits.md` §2.1 (row R1): the same with `#include "Dep/x.h"`. Today
/// the rename claims `f` in `<root>/Dep/x.h` as owned (its real path is
/// not under `<root>/dep`), reduce accepts the edit set, and it edits the
/// dependency's header; only apply refuses it (`not_canonical`).
#[test]
#[ignore = "WP-147 item 2: the map phase and reduce see the read-only header as read-only under any spelling"]
fn s2_1_rename_of_a_dependency_through_another_case() {
    if !volume_folds("another-case") {
        return;
    }
    let (root, r) = rename_of_a_dependency("one-spelling-dep-case", "Dep/x.h");
    assert_eq!(read_only_edited(&root, &r), Vec::<String>::new(), "{:?}", r.edits.state);
}

/// `spec/edits.md` §5.1 (row R2): one owned header reached by two
/// observers under two spellings is one file of the edit set. Today it is
/// two (`<root>/h.h` and `<root>/H.h`, the same edits in each): reduce
/// compares bytes, so the two are never checked against each other, the
/// overlay holds one file twice (Kiln refuses such an overlay: two
/// entries of one file), and apply refuses the second spelling.
#[test]
#[ignore = "WP-147 item 2: inputs have the spelling on disk, so reduce sees one file"]
fn s5_1_one_header_under_two_spellings_is_one_file_of_the_edit_set() {
    if !volume_folds("two-spellings") {
        return;
    }
    let files: [(&str, &[u8]); 3] = [
        ("h.h", b"namespace n { void f(); }\n"),
        ("a.cpp", b"#include \"h.h\"\nvoid ga() { n::f(); }\n"),
        ("b.cpp", b"#include \"H.h\"\nvoid gb() { n::f(); }\n"),
    ];
    let t = |r: &str| Transform {
        kind: TransformKind::Rename,
        ownership: Ownership { owned: vec![r.as_bytes().to_vec()], readonly: Vec::new() },
        indivisible: true,
        rename: Some(Rename {
            target: vec![b"n".to_vec(), b"f".to_vec()],
            kind: TargetKind::Function,
            new_name: b"h".to_vec(),
        }),
        script: None,
    };
    let name = "one-spelling-two";
    let (t, shards) = map_shards(name, &files, &["a.cpp", "b.cpp"], &t);
    let r = reduced(name, &t, shards);
    let files: Vec<String> =
        r.edits.files.iter().map(|f| String::from_utf8_lossy(&f.path).into_owned()).collect();
    let headers: Vec<&String> =
        files.iter().filter(|p| p.to_ascii_lowercase().ends_with("/h.h")).collect();
    assert_eq!(headers.len(), 1, "one name for the header: {files:?}");
}
