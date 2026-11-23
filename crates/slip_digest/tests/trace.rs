//! Traces of a unit's stream (`spec/verify.md` §6.1 "Traces", "The site
//! of a difference"; work package WP-054).
//!
//! - [`pairs_trace_every_unit`]: over every directory of the pair tests
//!   (`tests/differ`, `tests/equal`, `tests/demand`, `tests/indeterminate`,
//!   `tests/resolved`), both sides, (1) `trace(input, options, all
//!   keys).output == digest(input, options)`: tracing changes no digest;
//!   (2) every unit that is not indeterminate has a trace whose stream
//!   hashes to its digest, with marks ascending and inside the stream and
//!   references that are the keys of units at their offsets. A unit that
//!   fails (2) is a finding about the walk; [`ALLOWED`] would hold it with
//!   its reason, and is empty.
//! - [`differ_sites`]: for each pair of `tests/differ` whose `expect.txt`
//!   says `changed <name>` and whose sides have a unit of that key with
//!   different digests, the two traces differ at an offset and the site on
//!   each side (the last mark at or before it) is in a file of the pair.
//!   A pair's sides are the branches of one `#ifdef B`; the test finds them
//!   by text and, for the pairs under the expression and statement tables
//!   (`spec/digest-kinds.md` §6, §7), asserts that side A's site is on a
//!   line of the `#else` branch and side B's on one of the `#ifdef`
//!   branch. For every pair it prints how many sites were inside the right
//!   branch and how many were not.
//! - The snippets: the site of a changed callee, a changed literal, a
//!   changed type of a declaration and a changed default argument, at
//!   their lines.
//!
//! Both sides are walked under the empty table (a `relocate.txt` is the
//! pair runner's): the properties hold under any table.

mod common;

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::Path;

use common::{Expect, Side};
use slip_digest::trace::{DECLARATIONS, Trace, trace};
use slip_digest::{Detail, Input, Options, Output, ReadOnly};
use slip_model::Digest;
use slip_model::hash::tagged_digest;
use slip_model::keys::RuleTable;

/// The tag of a unit's stream (`stream.rs`'s `UNIT_TAG`).
const UNIT_TAG: &str = "slip.unit.v1";

/// Units that fail property (2), each with its reason:
/// `(directory relative to tests/, side, unit name, reason)`. Empty: every
/// unit that is not indeterminate has a trace.
const ALLOWED: &[(&str, Side, &str, &str)] = &[];

fn options(rules: &RuleTable) -> Options<'_> {
    Options {
        rules,
        read_only: ReadOnly::NO_FILE,
        detail: Detail::All,
        names: true,
        rule_use: false,
    }
}

/// Every key of an output, sorted, once.
fn all_keys(o: &Output) -> Vec<Digest> {
    let mut keys: Vec<Digest> =
        o.digests.groups.iter().flat_map(|g| g.units.iter().flatten().map(|u| u.key)).collect();
    keys.sort_unstable();
    keys.dedup();
    keys
}

fn names(o: &Output) -> HashMap<Digest, String> {
    o.names.iter().cloned().collect()
}

/// What is wrong with the traces of one side, `""` if nothing; the units
/// that fail (2) and are not in [`ALLOWED`] are named.
fn check_side(dir: &Path, side: Side, input: &Input<'_>) -> Result<(), String> {
    let rules = RuleTable::empty();
    let out = slip_digest::digest(input, &options(&rules));
    let keys = all_keys(&out);
    let traced = trace(input, &options(&rules), &keys);
    if traced.output != out {
        return Err("tracing changed the output".into());
    }
    let names = names(&out);
    let have: HashSet<(Digest, Digest)> = traced.traces.iter().map(|t| (t.key, t.digest)).collect();
    let rel = common::rel(dir);
    let mut missing = BTreeSet::new();
    for g in &out.digests.groups {
        for u in g.units.iter().flatten() {
            if u.is_indeterminate() || have.contains(&(u.key, u.digest)) {
                continue;
            }
            let name = names.get(&u.key).cloned().unwrap_or_else(|| format!("{:?}", u.key));
            if !ALLOWED.iter().any(|(d, s, n, _)| *d == rel && *s == side && *n == name) {
                missing.insert(name);
            }
        }
    }
    if !missing.is_empty() {
        return Err(format!("units with no trace: {missing:?}"));
    }
    let unit_keys: HashSet<Digest> = keys.iter().copied().collect();
    for t in &traced.traces {
        well_formed(t, &unit_keys).map_err(|e| {
            format!("{}: {e}", names.get(&t.key).map_or("<unnamed>", String::as_str))
        })?;
    }
    Ok(())
}

/// A trace's invariants (§6.1 "Traces").
fn well_formed(t: &Trace, unit_keys: &HashSet<Digest>) -> Result<(), String> {
    if tagged_digest(UNIT_TAG, &t.stream) != t.digest {
        return Err("the stream does not hash to the digest".into());
    }
    let len = t.stream.len() as u64;
    match t.marks.first() {
        Some(m) if m.offset == 0 => {}
        _ => return Err("no first mark at 0".into()),
    }
    if !t.marks.windows(2).all(|m| m[0].offset <= m[1].offset) {
        return Err("marks not ascending".into());
    }
    if t.marks.iter().any(|m| m.offset >= len) {
        return Err("a mark outside the stream".into());
    }
    if !t.refs.windows(2).all(|r| r[0].offset + r[0].len <= r[1].offset) {
        return Err("references overlap or are not ascending".into());
    }
    for r in &t.refs {
        let at = usize::try_from(r.offset).map_err(|e| e.to_string())?;
        if r.len != 32 || t.stream.get(at..at + 32) != Some(r.key.as_bytes().as_slice()) {
            return Err(format!("a reference at {} is not its key's bytes", r.offset));
        }
        if !unit_keys.contains(&r.key) {
            return Err("a reference to no unit".into());
        }
    }
    Ok(())
}

// §6.1 "Tests": tracing changes no digest, and every unit that is not
// indeterminate has a trace hashing to its digest, over every pair test's
// sources.
#[test]
fn pairs_trace_every_unit() {
    let mut dirs = Vec::new();
    for root in ["differ", "equal", "demand", "indeterminate", "resolved"] {
        dirs.extend(common::dirs(root).into_iter().filter(|d| !common::is_pending(d)));
    }
    eprintln!("pairs traced: {} directories", dirs.len());
    let threads = common::pool_size().unwrap_or_else(|e| panic!("{e}"));
    let outcome = common::run_dirs("pairs", &dirs, threads, |dir| {
        for side in [Side::A, Side::B] {
            // A side that does not parse is the pair runner's to report.
            if let Ok(r) = common::with_side(dir, side, |input| check_side(dir, side, input)) {
                r.map_err(|e| format!("side {side:?}: {e}"))?;
            }
        }
        Ok(Vec::new())
    });
    outcome.finish();
}

/// The site of a difference on one side: the first differing offset of
/// the two streams (the shorter length if one is a prefix of the other)
/// and the last mark at or before it.
fn site<'t>(t: &'t Trace, other: &Trace) -> (u64, &'t slip_digest::trace::Mark) {
    let d = t
        .stream
        .iter()
        .zip(&other.stream)
        .position(|(a, b)| a != b)
        .unwrap_or_else(|| t.stream.len().min(other.stream.len())) as u64;
    let m = t.marks.iter().rev().find(|m| m.offset <= d).expect("the first mark is at 0");
    (d, m)
}

/// The lines of `t.cpp` (directives other than the conditionals included)
/// in the branch each side reads: side B's where `B`
/// is defined (`#ifdef B`, `#if defined(B)`, the `#else` of `#ifndef B`),
/// side A's where it is not. A line under no such branch is in neither.
fn branches(text: &str) -> (BTreeSet<u64>, BTreeSet<u64>) {
    // Per open conditional: `Some(true)` while its lines are side B's,
    // `Some(false)` while they are side A's, `None` if it is not about B.
    let mut stack: Vec<Option<bool>> = Vec::new();
    let (mut a, mut b) = (BTreeSet::new(), BTreeSet::new());
    for (i, line) in text.lines().enumerate() {
        let l = line.trim();
        let words: Vec<&str> = l.trim_start_matches('#').split_whitespace().collect();
        if l.starts_with('#') {
            match words.as_slice() {
                ["ifdef", "B", ..] | ["if", "defined(B)", ..] => stack.push(Some(true)),
                ["ifndef", "B", ..] | ["if", "!defined(B)", ..] => stack.push(Some(false)),
                ["if" | "ifdef" | "ifndef", ..] => stack.push(None),
                ["else", ..] => {
                    if let Some(top) = stack.last_mut() {
                        *top = top.map(|v| !v);
                    }
                }
                ["endif", ..] => {
                    stack.pop();
                }
                // Another directive (`#pragma`, `#define`) is a line of the
                // branch it is in: a site can be on it.
                _ => {}
            }
            if matches!(words.first(), Some(&("if" | "ifdef" | "ifndef" | "else" | "endif"))) {
                continue;
            }
        }
        let line_no = i as u64 + 1;
        match stack.iter().rev().find_map(|s| *s) {
            Some(true) => {
                b.insert(line_no);
            }
            Some(false) => {
                a.insert(line_no);
            }
            None => {}
        }
    }
    (a, b)
}

/// The short names of the expression and statement tables
/// (`spec/digest-kinds.md` §6, §7).
const STATEMENT_AND_EXPRESSION_TABLES: &[&str] = &[
    "StmtKind",
    "StmtNode",
    "IfKind",
    "IfData",
    "SwitchData",
    "CaseData",
    "LoopData",
    "RangeForData",
    "ExpansionForm",
    "ExpansionData",
    "CoroutineBodyData",
    "AsmOperand",
    "AsmData",
    "ContractKind",
    "ContractSemantic",
    "ContractSite",
    "StmtArena",
    "ExprKind",
    "ExprNode",
    "ValueCategory",
    "ObjectKind",
    "ExprFlags",
    "ExprDeps",
    "UnaryOp",
    "BinaryOp",
    "CastKind",
    "CharKind",
    "StringKind",
    "TraitKind",
    "NonOdrUse",
    "Radix",
    "ConstructionKind",
    "StorageDuration",
    "TypeTrait",
    "NewInitStyle",
    "PredefinedKind",
    "SourceLocKind",
    "SuspendKind",
    "ConstructInfo",
    "TypeidOperand",
    "GenericControl",
    "GenericAssoc",
    "CoawaitParts",
    "ExtVectorAccess",
    "DeclRefExtra",
    "CompoundAssignExtra",
    "Designator",
    "OffsetofComponent",
    "OffsetofStep",
    "InitListExtra",
    "NewExtra",
    "SourceLocValues",
    "ArrayTraitExtra",
    "ColdExprData",
    "OverloadedMember",
    "ExprArena",
];

/// Why a site is not on a line of its side's branch (§6.1 "The site of a
/// difference" says where a site is; these are the places it puts one
/// that a branch does not hold).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Outside {
    /// The mark has no location: the order unit's first mark (§6.1 "A
    /// unit's place": none), or a node the front end made with no location
    /// (`CXXDefaultArgExpr`, `ImplicitValueInitExpr`): "empty, 0, 0 if it
    /// has none".
    Unplaced,
    /// The site is the unit's first mark: the difference is in the
    /// declaration's `DeclNode` rows, which have no mark, or before the first node
    /// (§6.1 "Traces", first item), and the declaration is outside the
    /// branch.
    First,
    /// The side's branch has no line (an `#ifdef B` with no `#else`).
    EmptyBranch,
    /// A node outside the branch whose bytes depend on what the branch
    /// declares (a use of a member the branch makes a bit-field, a call
    /// the branch makes immediate): the difference is where it is written.
    Elsewhere,
    /// The site is a `declarations` mark (`trace::DECLARATIONS`): the
    /// difference is in the rows merged over an entity's declarations
    /// (its scope, attributes, template maps, module rows), placed at the
    /// entity's place, and the branch changes a declaration of the chain
    /// that is not the one the place names. Coarse and true: a mark per
    /// redeclaration is a follow-up (see the landed file).
    DeclarationRows,
}

/// The pairs whose difference is only in rows merged over the entity's
/// declarations, written after an initializer: each side's site must be
/// the declarations mark (review F2; without it the site was the
/// initializer, which did not change).
const DECLARATION_ROWS: &[&str] = &[
    // An asm label on the first declaration (line 4 or 6): the site is
    // that line, in the branch.
    "differ/ColdDeclData/asm_label",
    // `weak` on the pattern's member (line 7, side B): the site is the
    // member's line 9, outside the branch.
    "differ/_/weak-static-member-of-instantiation",
];

/// The pairs under the expression and statement tables whose sites are
/// not all in their branches, with the classes of [`Outside`] they have.
/// The test fails on such a pair not listed here, and on a listed pair
/// whose classes are not these.
const SITES_OUTSIDE: &[(&str, &[Outside])] = {
    use Outside::{Elsewhere, EmptyBranch, First, Unplaced};
    &[
        // The class's implicit constructor changes (its own rows) and so
        // does the temporary that calls it, at the use.
        ("differ/ConstructInfo/zeroing", &[First, Elsewhere]),
        // Side A writes a `CXXDefaultArgExpr`, which has no location.
        ("differ/ExprKind/CXXDefaultArgExpr", &[Unplaced]),
        ("differ/ExprNode/CXXDefaultArgExpr.a", &[Unplaced]),
        // Side A writes an `ImplicitValueInitExpr`, which has no location.
        ("differ/ExprKind/ImplicitValueInitExpr", &[Unplaced]),
        // The branch adds a member to the class (its own rows; side A's
        // branch is empty) and the uses outside it change.
        ("differ/ExprNode/ArrayInitLoopExpr.b", &[First, EmptyBranch, Elsewhere]),
        ("differ/NewExtra/operator_delete", &[First, EmptyBranch, Elsewhere]),
        ("differ/NewExtra/operator_new", &[First, EmptyBranch, Elsewhere]),
        // The branch changes a declaration; the expression that uses it,
        // outside the branch, is where the bytes differ.
        ("differ/ExprNode/CXXDeleteExpr.b", &[Elsewhere]),
        ("differ/ExprNode/DeclRefExpr.bits-immediate-escalating", &[Elsewhere]),
        ("differ/ObjectKind/BitField", &[Elsewhere]),
        ("differ/OverloadedMember/naming_class", &[Elsewhere]),
        // The branch changes a constructor of the class: the class's own
        // rows, at its first mark.
        ("differ/ExprNode/CXXParenListInitExpr.b", &[First]),
        ("differ/ExprNode/InitListExpr.b", &[First]),
        ("differ/InitListExtra/array_filler", &[First]),
    ]
};

/// One side traced: its traces of every key and the names of its keys.
struct Sided {
    traces: Vec<Trace>,
    names: HashMap<Digest, String>,
}

fn traced_side(dir: &Path, side: Side) -> Result<Sided, String> {
    common::with_side(dir, side, |input| {
        let rules = RuleTable::empty();
        let out = slip_digest::digest(input, &options(&rules));
        let t = trace(input, &options(&rules), &all_keys(&out));
        Sided { traces: t.traces, names: names(&t.output) }
    })
}

/// The files of a pair, by the names the snippet gives them.
fn pair_files(dir: &Path) -> BTreeSet<Vec<u8>> {
    let mut out = BTreeSet::from([b"main.cpp".to_vec(), b"side.h".to_vec()]);
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        out.insert(e.file_name().to_string_lossy().as_bytes().to_vec());
    }
    out
}

/// The traces of a side's units named `name`, by key.
fn of_name<'s>(s: &'s Sided, name: &str) -> BTreeMap<Digest, Vec<&'s Trace>> {
    let mut m: BTreeMap<Digest, Vec<&Trace>> = BTreeMap::new();
    for t in &s.traces {
        if s.names.get(&t.key).is_some_and(|n| n == name) {
            m.entry(t.key).or_default().push(t);
        }
    }
    m
}

/// The differ site test of one pair: a note `sites inside <n> outside
/// <m> [<classes>]` (or `sites unbranched` for an `a.cpp` / `b.cpp`
/// pair); an error if a site is in no file of the pair, or if a pair under
/// the expression and statement tables has a site outside its branch and
/// is not in [`SITES_OUTSIDE`] with exactly those classes.
fn differ_pair(dir: &Path) -> Result<Vec<String>, String> {
    let changed: Vec<String> = common::expect(dir)?
        .into_iter()
        .filter_map(|e| match e {
            Expect::Changed(n) => Some(n),
            _ => None,
        })
        .collect();
    if changed.is_empty() {
        return Ok(Vec::new());
    }
    let (Ok(a), Ok(b)) = (traced_side(dir, Side::A), traced_side(dir, Side::B)) else {
        return Ok(Vec::new());
    };
    let text = std::fs::read_to_string(dir.join("t.cpp")).ok();
    let lines = text.as_deref().map(branches).filter(|(a, b)| !a.is_empty() || !b.is_empty());
    let files = pair_files(dir);
    let rel = common::rel(dir);
    let short = rel.split('/').nth(1).unwrap_or_default();
    let strict = STATEMENT_AND_EXPRESSION_TABLES.contains(&short);
    let (mut inside, mut checked) = (0usize, 0usize);
    let mut outside: Vec<(Outside, String)> = Vec::new();
    for name in &changed {
        let (ta, tb) = (of_name(&a, name), of_name(&b, name));
        for (key, va) in &ta {
            let Some(vb) = tb.get(key) else { continue };
            let da: BTreeSet<Digest> = va.iter().map(|t| t.digest).collect();
            let db: BTreeSet<Digest> = vb.iter().map(|t| t.digest).collect();
            if da == db {
                continue;
            }
            // The units that differ: of each side, one whose digest the
            // other side has not.
            let ua = va.iter().find(|t| !db.contains(&t.digest)).unwrap_or(&va[0]);
            let ub = vb.iter().find(|t| !da.contains(&t.digest)).unwrap_or(&vb[0]);
            if ua.stream == ub.stream {
                return Err(format!("{name}: two digests of one stream"));
            }
            checked += 1;
            for (side, t, other) in [(Side::A, ua, ub), (Side::B, ub, ua)] {
                let (d, m) = site(t, other);
                if DECLARATION_ROWS.contains(&rel.as_str()) && m.node != DECLARATIONS {
                    return Err(format!(
                        "{name}: side {side:?}: site {} at line {}, not the declarations mark",
                        m.node, m.line
                    ));
                }
                let at = format!(
                    "{name}: side {side:?}: {} at {}:{}, offset {d}",
                    m.node,
                    String::from_utf8_lossy(&m.file),
                    m.line
                );
                if m.file.is_empty() {
                    outside.push((Outside::Unplaced, at));
                    continue;
                }
                if !files.contains(&m.file) {
                    return Err(format!("{at}: in no file of the pair"));
                }
                let Some((la, lb)) = &lines else { continue };
                let right = if side == Side::A { la } else { lb };
                if m.file == b"main.cpp" && right.contains(&m.line) {
                    inside += 1;
                } else if right.is_empty() {
                    outside.push((Outside::EmptyBranch, at));
                } else if m.offset == 0 {
                    outside.push((Outside::First, at));
                } else if m.node == DECLARATIONS {
                    outside.push((Outside::DeclarationRows, at));
                } else {
                    outside.push((Outside::Elsewhere, at));
                }
            }
        }
    }
    if checked == 0 {
        return Ok(Vec::new());
    }
    let mut classes: Vec<Outside> = outside.iter().map(|o| o.0).collect();
    classes.sort_unstable();
    classes.dedup();
    if strict {
        let listed = SITES_OUTSIDE.iter().find(|(d, _)| *d == rel).map(|(_, c)| *c);
        if listed.unwrap_or_default() != classes.as_slice() {
            let sites: Vec<String> = outside.iter().map(|(c, at)| format!("{c:?} {at}")).collect();
            return Err(format!(
                "sites outside the branch {classes:?}, listed {listed:?}: {}",
                sites.join("; ")
            ));
        }
    }
    if lines.is_none() {
        let n = outside.len();
        return Ok(vec![format!("sites unbranched (unplaced {n})")]);
    }
    Ok(vec![format!("sites inside {inside} outside {} {classes:?}", outside.len())])
}

// §6.1 "The site of a difference", over the pairs of `tests/differ`.
#[test]
fn differ_sites() {
    let dirs: Vec<_> =
        common::dirs("differ").into_iter().filter(|d| !common::is_pending(d)).collect();
    let threads = common::pool_size().unwrap_or_else(|e| panic!("{e}"));
    let outcome = common::run_dirs("differ", &dirs, threads, differ_pair);
    let (mut pairs, mut unbranched, mut inside, mut outside) = (0, 0, 0, 0);
    let mut by_class: BTreeMap<String, usize> = BTreeMap::new();
    for n in &outcome.notes {
        let note = n.rsplit(": ").next().unwrap_or_default();
        pairs += 1;
        if note.starts_with("sites unbranched") {
            unbranched += 1;
            continue;
        }
        let nums: Vec<usize> = note.split_whitespace().filter_map(|w| w.parse().ok()).collect();
        if let [i, o] = nums.as_slice() {
            inside += i;
            outside += o;
        }
        if let Some(c) = note.split_once('[').map(|(_, c)| c.trim_end_matches(']')) {
            for c in c.split(", ").filter(|c| !c.is_empty()) {
                *by_class.entry(c.to_owned()).or_default() += 1;
            }
        }
    }
    eprintln!(
        "differ sites: {pairs} pairs with a changed unit ({unbranched} without branches); \
         sites inside the right branch {inside}, not inside {outside}; pairs per class \
         {by_class:?}"
    );
    if std::env::var_os("SLIP_TRACE_OUTSIDE").is_some() {
        for n in outcome.notes.iter().filter(|n| !n.ends_with("[]")) {
            eprintln!("  {n}");
        }
    }
    if let Some(f) = outcome.report().1 {
        panic!("{f}");
    }
    assert!(pairs > 0, "no pair was checked");
}

/// Traces of every unit of `src` read as a snippet's `main.cpp`.
fn snippet(src: &str) -> Sided {
    snippet_std(src, "c++20")
}

/// [`snippet`] under the standard `std`.
fn snippet_std(src: &str, std: &str) -> Sided {
    let files = [("main.cpp", src)];
    let snippet =
        slip_kiln::snippet::Snippet { files: &files, main: "main.cpp", include: &[""], std };
    slip_kiln::snippet::parse(&snippet, |p| {
        assert_eq!(p.errors, 0, "{}", p.diagnostics);
        let (ast, sm) = (p.ast, p.sm);
        let paths: Vec<Vec<u8>> = sm
            .file_ids()
            .map(|f| p.path(f).map(|s: &str| s.as_bytes().to_vec()).unwrap_or_default())
            .collect();
        common::on_walk_stack(|| {
            let path = |f: slip_kiln::kiln::source::FileId| {
                paths.get(f.index()).cloned().unwrap_or_default()
            };
            let input = Input { ast, sm, subject_path: &path };
            let rules = RuleTable::empty();
            let out = slip_digest::digest(&input, &options(&rules));
            let t = trace(&input, &options(&rules), &all_keys(&out));
            Sided { traces: t.traces, names: names(&t.output) }
        })
    })
    .expect("the snippet parses")
}

/// The sites `(name, line A, node A, line B, node B)` of every unit whose
/// key both sides have with different digests.
fn sites(a: &str, b: &str) -> Vec<(String, u64, &'static str, u64, &'static str)> {
    let (a, b) = (snippet(a), snippet(b));
    let mut out = Vec::new();
    for ta in &a.traces {
        let Some(tb) = b.traces.iter().find(|t| t.key == ta.key) else { continue };
        if ta.digest == tb.digest {
            continue;
        }
        let ((_, ma), (_, mb)) = (site(ta, tb), site(tb, ta));
        assert_eq!((ma.file.as_slice(), mb.file.as_slice()), (&b"main.cpp"[..], &b"main.cpp"[..]));
        let name = a.names.get(&ta.key).cloned().unwrap_or_default();
        out.push((name, ma.line, ma.node, mb.line, mb.node));
    }
    out
}

/// Asserts that the unit `name` changed, with its site at `line` on both
/// sides, at a node of the kind `node`.
fn assert_site(a: &str, b: &str, name: &str, line: u64, node: &str) {
    let s = sites(a, b);
    let Some(u) = s.iter().find(|u| u.0 == name) else {
        panic!("no changed unit {name}: {s:?}");
    };
    assert_eq!((u.1, u.2, u.3, u.4), (line, node, line, node), "{name}: {s:?}");
}

// §6.1 "Tests": the site of a changed callee is its reference, on the line
// of the call.
#[test]
fn s6_1_site_of_a_changed_callee() {
    let src = |callee: &str| {
        format!(
            "int f(int);\nint g(int);\nint h(int x) {{\n  int y = x + 1;\n  return {callee}(y);\n}}\n"
        )
    };
    assert_site(&src("f"), &src("g"), "h(int)", 5, "DeclRefExpr");
}

// A changed literal: the literal, on its line.
#[test]
fn s6_1_site_of_a_changed_literal() {
    let src = |n: u32| format!("int h(int x) {{\n  int y = x;\n  return y\n    + {n};\n}}\n");
    assert_site(&src(1), &src(2), "h(int)", 4, "IntegerLiteral");
}

// A changed type of a declaration: the declaration statement that holds
// it, on its line.
#[test]
fn s6_1_site_of_a_changed_declaration_type() {
    let src = |t: &str| format!("int h(int x) {{\n  x += 1;\n  {t} y = x;\n  return y;\n}}\n");
    assert_site(&src("long"), &src("short"), "h(int)", 3, "DeclStmt");
}

// A changed default argument: the default argument's unit, at its
// expression's line.
#[test]
fn s6_1_site_of_a_changed_default_argument() {
    let src = |n: u32| format!("int g(int a,\n      int b =\n        {n} + 1);\n");
    let s = sites(&src(1), &src(2));
    assert!(
        s.iter().any(|u| (u.1, u.2, u.3, u.4) == (3, "IntegerLiteral", 3, "IntegerLiteral")),
        "{s:?}"
    );
}

// A difference in the rows merged over an entity's declarations (an asm
// label, an attribute on a redeclaration) is at the declarations mark, at
// the entity's place: not at the initializer written before those rows
// (review F2). The pairs `differ/ColdDeclData/asm_label` and
// `differ/_/weak-static-member-of-instantiation`.
#[test]
fn s6_1_site_of_a_declaration_row_is_the_entity() {
    // The place is the first declaration: line 2 on both sides.
    let asm = |label: &str| format!("int f();\nextern int x{label};\nextern int x;\nint x = 1;\n");
    let s = sites(&asm(""), &asm(" asm(\"y\")"));
    assert!(s.contains(&("x".to_owned(), 2, DECLARATIONS, 2, DECLARATIONS)), "{s:?}");
}

#[test]
fn s6_1_site_of_an_attribute_on_a_member_is_the_entity() {
    // `weak` on the pattern's member: the place is the member (line 3),
    // not the out-of-class definition's initializer (line 5).
    let weak = |attr: &str| {
        format!(
            "template <class T> struct S {{\n  {attr}\n  static int v;\n}};\n\
             template <class T> int S<T>::v = 1;\nint h() {{ return S<int>::v; }}\n"
        )
    };
    let s = sites(&weak(""), &weak("__attribute__((weak))"));
    assert!(s.contains(&("S::v".to_owned(), 3, DECLARATIONS, 3, DECLARATIONS)), "{s:?}");
}

// A changed operand in parentheses: the site is the literal, not the
// parentheses (the hook marks the node after `strip_parens`).
#[test]
fn s6_1_site_of_a_changed_operand_in_parentheses() {
    let src = |n: u32| format!("int h(int x) {{\n  int y = x;\n  return y\n    * ({n});\n}}\n");
    assert_site(&src(1), &src(2), "h(int)", 4, "IntegerLiteral");
}

// A place is in the physical file at its physical line: a `#line`
// directive moves neither (§6.1).
#[test]
fn s6_1_site_under_a_line_directive() {
    let src = |n: u32| {
        format!(
            "#line 500 \"elsewhere.cpp\"\nint h(int x) {{\n  int y = x;\n  return y\n    + {n};\n}}\n"
        )
    };
    assert_site(&src(1), &src(2), "h(int)", 5, "IntegerLiteral");
    let a = snippet(&src(1));
    let h = a.traces.iter().find(|t| a.names.get(&t.key).is_some_and(|n| n == "h(int)")).unwrap();
    assert_eq!((h.marks[0].file.as_slice(), h.marks[0].line), (&b"main.cpp"[..], 2));
}

// A statement written again as a back-reference (nested destructuring
// expansions share their inner pattern: two of its compound statements
// are written as back-references) has a mark each time: the hook is
// before the back-reference. 11 marks on the inner line; 9 if the
// back-referenced ones had none.
#[test]
fn s6_1_back_referenced_statement_is_marked() {
    let src = "struct P { int a; int b; };\nint g(int);\nvoid f() {\n  P p{1, 2};\n  \
               template for (auto e : p) {\n    template for (auto k : p) {\n      \
               g(e + k);\n    }\n  }\n}\n";
    let a = snippet_std(src, "c++26");
    let f = a.traces.iter().find(|t| a.names.get(&t.key).is_some_and(|n| n == "f()")).unwrap();
    let inner = f.marks.iter().filter(|m| m.node == "CompoundStmt" && m.line == 6).count();
    assert_eq!(inner, 11, "{:#?}", f.marks);
}
