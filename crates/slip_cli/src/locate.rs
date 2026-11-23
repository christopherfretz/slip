//! Names and places of findings (`spec/verify.md` §6.1, work packages
//! WP-052 and WP-054): the four items of §6.1 "Interface".
//!
//! Outside the trusted base: no verdict depends on a name or a place
//! (§5.2), and a `Where` that points at the wrong token is a defect of the
//! report, not of the verification. The one thing a verdict takes from
//! here is [`Side::tu`], for guard G3.
//!
//! [`take`] walks one image once with [`slip_digest::trace::trace`] for
//! the wanted keys, and keeps what [`fill`] reads: the names of those
//! keys and of every unit a kept trace refers to, each unit of a wanted
//! key with its group class and whether it is indeterminate, the traces,
//! and the cause of each indeterminate key. [`fill`] then makes each
//! finding's `location` from the two sides as §6.1 "The site of a
//! difference" and "Other findings" say. A unit whose stream, written
//! again, did not hash to its digest has no trace (counted by
//! [`Side::unstable`], `verify.traces.unstable`), and a finding that needs
//! its trace has no `location`.

use slip_digest::trace::{Trace, trace};
use slip_digest::{Detail, Input, Options, ReadOnly, ReadOnlyFiles};
use slip_model::Digest;
use slip_model::digests::GroupClass;
use slip_model::keys::RuleTable;
use slip_model::transform::Ownership;
use slip_model::verdict::{Finding, FindingKind, Site, Where};

/// Longest `Finding::name`, in bytes (§6.1).
const NAME_MAX: usize = 4096;

/// One unit of a wanted key in one image.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct UnitAt {
    key: Digest,
    digest: Digest,
    class: GroupClass,
    indeterminate: bool,
}

/// What localisation needs from one image, taken while its parse is in
/// memory: the `tu` digest of the walk; the names of the wanted keys and
/// of the keys the kept traces refer to (every name a unit of such a key
/// has, sorted by key then name); the units of the wanted keys; their
/// traces; the cause of each indeterminate wanted key; and how many of
/// those units have no trace.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Side {
    tu: Digest,
    names: Vec<(Digest, String)>,
    /// Sorted, without duplicates.
    units: Vec<UnitAt>,
    /// Sorted by `(key, digest)`, as `trace` gives them.
    traces: Vec<Trace>,
    /// The cause of each indeterminate wanted key as the walk prints it,
    /// sorted by key.
    causes: Vec<(Digest, String)>,
    unstable: u64,
    /// The second writing panicked: the walk stopped tracing there, and
    /// the wanted units it had not traced have no trace.
    panicked: bool,
}

impl Side {
    /// The `tu` digest of the walk that took this side.
    pub fn tu(&self) -> &Digest {
        &self.tu
    }

    /// Units of the wanted keys whose stream, written again, did not hash
    /// to their digest: they have no trace (`verify.traces.unstable`,
    /// §7.2). A unit is a key and a digest: two groups' copies of one
    /// are one stream.
    pub fn unstable(&self) -> u64 {
        self.unstable
    }

    /// Whether the second writing panicked (`verify.traces.panicked`,
    /// §7.2): the walk's state is not trusted after it, so nothing more
    /// was traced on this side; its units with no trace are not counted
    /// in [`Side::unstable`].
    pub fn panicked(&self) -> bool {
        self.panicked
    }

    /// The first name, in the order of its bytes, of a unit of `key`.
    fn name(&self, key: &Digest) -> Option<&str> {
        let start = self.names.partition_point(|(k, _)| k < key);
        self.names[start..].iter().take_while(|(k, _)| k == key).map(|(_, n)| n.as_str()).min()
    }

    /// The units of `key`.
    fn units_of(&self, key: &Digest) -> &[UnitAt] {
        let start = self.units.partition_point(|u| u.key < *key);
        let len = self.units[start..].iter().take_while(|u| u.key == *key).count();
        &self.units[start..start + len]
    }

    /// The trace of the unit of `key` with `digest`.
    fn trace_of(&self, key: &Digest, digest: &Digest) -> Option<&Trace> {
        self.traces
            .binary_search_by(|t| (&t.key, &t.digest).cmp(&(key, digest)))
            .ok()
            .map(|i| &self.traces[i])
    }

    /// The cause of the indeterminate key `key`, as the walk prints it.
    fn cause(&self, key: &Digest) -> &str {
        self.causes.binary_search_by(|(k, _)| k.cmp(key)).map_or("", |i| self.causes[i].1.as_str())
    }
}

/// One walk of `input` under `rules` and the run's `files` (names asked
/// for; read-only files as the verify job's walk of the same image
/// decides them), tracing the units whose key is in `keys` (sorted, no
/// duplicates). Detail `all`: the unit lists say which units a wanted
/// key has, in which class of group, and which are indeterminate; the
/// `tu` digest does not depend on detail.
pub fn take(
    input: &Input<'_>,
    rules: &RuleTable,
    files: Option<&Ownership>,
    keys: &[Digest],
) -> Side {
    let read_only = ReadOnlyFiles::new(files, input.sm, input.subject_path);
    let options = Options {
        rules,
        read_only: ReadOnly::files(&read_only),
        detail: Detail::All,
        names: true,
        rule_use: false,
    };
    let run = trace(input, &options, keys);
    let wanted = |k: &Digest| keys.binary_search(k).is_ok();
    let output = run.output;
    let mut units: Vec<UnitAt> = Vec::new();
    for g in &output.digests.groups {
        for u in g.units.iter().flatten().filter(|u| wanted(&u.key)) {
            units.push(UnitAt {
                key: u.key,
                digest: u.digest,
                class: g.class,
                indeterminate: u.is_indeterminate(),
            });
        }
    }
    units.sort();
    units.dedup();
    // An indeterminate unit is no defect of the walk if it has no trace
    // (its digest may be marked after it was written: a collision).
    let mut streams: Vec<(Digest, Digest)> =
        units.iter().filter(|u| !u.indeterminate).map(|u| (u.key, u.digest)).collect();
    streams.dedup();
    let (traces, panicked) = (run.traces, run.panicked);
    let unstable = if panicked {
        0
    } else {
        streams
            .iter()
            .filter(|(k, d)| traces.binary_search_by(|t| (&t.key, &t.digest).cmp(&(k, d))).is_err())
            .count() as u64
    };
    let mut shown: Vec<Digest> = keys.to_vec();
    shown.extend(traces.iter().flat_map(|t| t.refs.iter().map(|r| r.key)));
    shown.sort_unstable();
    shown.dedup();
    let mut names: Vec<(Digest, String)> =
        output.names.into_iter().filter(|(k, _)| shown.binary_search(k).is_ok()).collect();
    names.sort();
    let causes =
        output.causes.iter().filter(|(k, _)| wanted(k)).map(|(k, c)| (*k, c.to_string())).collect();
    Side { tu: output.digests.tu, names, units, traces, causes, unstable, panicked }
}

/// `verify.traces.unstable` and `verify.traces.panicked` (§7.2) of a
/// step 6: the units of the wanted keys whose second writing did not hash
/// to their digest, and the images whose second writing panicked, over
/// the pre-image (if taken) and the post-image.
pub fn unplaced(pre: Option<&Side>, post: &Side) -> (u64, u64) {
    let sides = pre.into_iter().chain([post]);
    sides.fold((0, 0), |(u, p), s| (u + s.unstable, p + u64::from(s.panicked)))
}

/// The keys of the first `locate` findings, sorted, no duplicates.
pub fn wanted(findings: &[Finding], locate: usize) -> Vec<Digest> {
    let mut keys: Vec<Digest> = findings.iter().take(locate).filter_map(|f| f.key).collect();
    keys.sort_unstable();
    keys.dedup();
    keys
}

/// Fills `name` and `location` of the first `locate` findings (§6.1);
/// changes no other field. `name`: the first name, in byte order, of a
/// unit of the key in the pre-image when it has one, else in the
/// post-image, cut to 4096 bytes at a character boundary. `location`: as
/// [`place`] makes it. `pre` is `None` when the pre-image was not parsed:
/// every name then comes from the post-image, and a finding that needs a
/// unit of the pre-image has no `location`.
pub fn fill(findings: &mut [Finding], locate: usize, pre: Option<&Side>, post: &Side) {
    for f in findings.iter_mut().take(locate) {
        let Some(key) = f.key else { continue };
        let name = pre.and_then(|p| p.name(&key)).or_else(|| post.name(&key));
        if let Some(name) = name {
            cut(name).clone_into(&mut f.name);
        }
        f.location = place(f, &key, pre, post);
    }
}

/// The `Where` of finding `f` of key `key` (§6.1 "The site of a
/// difference", "Other findings"); `None` when a unit it needs has no
/// trace in its image.
fn place(f: &Finding, key: &Digest, pre: Option<&Side>, post: &Side) -> Option<Where> {
    match f.kind {
        FindingKind::Changed | FindingKind::DerivedChanged => {
            let pre = pre?;
            match (f.pre, f.post) {
                (Some(a), Some(b)) => {
                    let (ta, tb) = (pre.trace_of(key, &a)?, post.trace_of(key, &b)?);
                    if declared_and_derived(pre, post, key, &a, &b) {
                        // §4 step 4: the two units are not compared.
                        return Some(Where { pre: first_mark(ta, ""), post: first_mark(tb, "") });
                    }
                    let d = first_difference(&ta.stream, &tb.stream);
                    Some(Where { pre: site(ta, d, pre), post: site(tb, d, post) })
                }
                _ => Some(Where {
                    pre: first_of_least(pre, key, |_| true)?,
                    post: first_of_least(post, key, |_| true)?,
                }),
            }
        }
        FindingKind::Removed => {
            Some(Where { pre: first_of_least(pre?, key, |_| true)?, post: Site::default() })
        }
        FindingKind::Added => {
            Some(Where { pre: Site::default(), post: first_of_least(post, key, |_| true)? })
        }
        FindingKind::Indeterminate | FindingKind::DerivedIndeterminate => {
            let side = |s: Option<&Side>| -> Option<Site> {
                let Some(s) = s else { return Some(Site::default()) };
                if !s.units_of(key).iter().any(|u| u.indeterminate) {
                    return Some(Site::default());
                }
                let mut site = first_of_least(s, key, |u| u.indeterminate)?;
                s.cause(key).clone_into(&mut site.what);
                Some(site)
            };
            Some(Where { pre: side(pre)?, post: side(Some(post))? })
        }
    }
}

/// Whether the units of `key` with digest `a` in `pre` and `b` in `post`
/// are a declared (or `tu`) unit on one side and a derived one on the
/// other: a `changed` of §4 step 4.
fn declared_and_derived(pre: &Side, post: &Side, key: &Digest, a: &Digest, b: &Digest) -> bool {
    let derived = |s: &Side, d: &Digest| {
        let of: Vec<&UnitAt> = s.units_of(key).iter().filter(|u| u.digest == *d).collect();
        !of.is_empty() && of.iter().all(|u| u.class == GroupClass::Derived)
    };
    derived(pre, a) != derived(post, b)
}

/// The first mark of the unit of `key` with the smallest digest among
/// those `keep` holds of, `what` empty; the empty site when the side has
/// none; `None` when that unit has no trace.
fn first_of_least(s: &Side, key: &Digest, keep: impl Fn(&UnitAt) -> bool) -> Option<Site> {
    let Some(u) = s.units_of(key).iter().filter(|u| keep(u)).min_by_key(|u| u.digest) else {
        return Some(Site::default());
    };
    Some(first_mark(s.trace_of(key, &u.digest)?, ""))
}

/// The site of a trace's first mark (offset 0), with `what`.
fn first_mark(t: &Trace, what: &str) -> Site {
    let mut site = t.marks.first().map_or_else(Site::default, |m| Site {
        file: m.file.clone(),
        line: m.line,
        column: m.column,
        node: m.node.to_owned(),
        what: String::new(),
    });
    what.clone_into(&mut site.what);
    site
}

/// The first offset at which `a` and `b` differ; the length of the
/// shorter if one is a prefix of the other.
fn first_difference(a: &[u8], b: &[u8]) -> u64 {
    let n = a.iter().zip(b).take_while(|(x, y)| x == y).count();
    n as u64
}

/// The site at offset `d` of trace `t` of side `s`: the last mark whose
/// offset is at most `d`, and `refers to <name>` when `d` lies inside a
/// reference of the trace, with the name of its key in that image (64
/// hexadecimal digits if it has no unit there).
fn site(t: &Trace, d: u64, s: &Side) -> Site {
    let end = t.marks.partition_point(|m| m.offset <= d);
    let mut site = match end.checked_sub(1).map(|i| &t.marks[i]) {
        Some(m) => Site {
            file: m.file.clone(),
            line: m.line,
            column: m.column,
            node: m.node.to_owned(),
            what: String::new(),
        },
        None => Site::default(),
    };
    let at = t.refs.partition_point(|r| r.offset + r.len <= d);
    if let Some(r) = t.refs.get(at).filter(|r| r.offset <= d) {
        let name = s.name(&r.key).map_or_else(|| r.key.to_string(), |n| cut(n).to_owned());
        site.what = format!("refers to {name}");
    }
    site
}

/// `text` cut to at most [`NAME_MAX`] bytes at a character boundary.
fn cut(text: &str) -> &str {
    let mut end = text.len().min(NAME_MAX);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

#[cfg(test)]
mod tests {
    use slip_digest::trace::{Mark, RefAt};

    use super::*;

    fn d(n: u8) -> Digest {
        Digest([n; 32])
    }

    fn finding(kind: FindingKind, key: Option<Digest>) -> Finding {
        Finding {
            kind,
            key,
            file: Vec::new(),
            pre: None,
            post: None,
            name: String::new(),
            location: None,
        }
    }

    fn changed(key: u8, pre: u8, post: u8) -> Finding {
        Finding {
            pre: Some(d(pre)),
            post: Some(d(post)),
            ..finding(FindingKind::Changed, Some(d(key)))
        }
    }

    fn side(names: &[(u8, &str)]) -> Side {
        let mut names: Vec<(Digest, String)> =
            names.iter().map(|&(k, n)| (d(k), n.to_owned())).collect();
        names.sort();
        Side {
            tu: d(0),
            names,
            units: Vec::new(),
            traces: Vec::new(),
            causes: Vec::new(),
            unstable: 0,
            panicked: false,
        }
    }

    // §7.2: `unstable` adds the units of both sides; `panicked` counts the
    // sides whose second writing panicked; a side not taken counts nothing.
    #[test]
    fn s7_2_unplaced_counts_both_sides() {
        let side_of = |unstable, panicked| Side { unstable, panicked, ..side(&[]) };
        assert_eq!(unplaced(None, &side_of(0, false)), (0, 0));
        assert_eq!(unplaced(Some(&side_of(2, false)), &side_of(1, true)), (3, 1));
        assert_eq!(unplaced(Some(&side_of(0, true)), &side_of(0, true)), (0, 2));
    }

    fn mark(offset: u64, node: &'static str, line: u64) -> Mark {
        Mark { offset, node, file: b"t.cpp".to_vec(), line, column: 1 }
    }

    /// A side holding one unit of `key` (class `class`) with the trace
    /// `stream`, `marks`, `refs`; its digest is `digest`.
    fn with_unit(
        mut s: Side,
        key: u8,
        digest: u8,
        class: GroupClass,
        stream: &[u8],
        marks: Vec<Mark>,
        refs: Vec<RefAt>,
    ) -> Side {
        s.units.push(UnitAt { key: d(key), digest: d(digest), class, indeterminate: false });
        s.units.sort();
        s.traces.push(Trace {
            key: d(key),
            digest: d(digest),
            stream: stream.to_vec(),
            marks,
            refs,
        });
        s.traces.sort_by_key(|t| (t.key, t.digest));
        s
    }

    // §6.1 "Interface": `wanted` is the keys of the first `locate`
    // findings, sorted, without duplicates; a finding without a key adds
    // none.
    #[test]
    fn s6_1_wanted_is_the_first_keys_sorted() {
        let k = |n| finding(FindingKind::Changed, n);
        let f = [k(Some(d(5))), k(Some(d(2))), k(None), k(Some(d(5)))];
        assert_eq!(wanted(&f, 4), [d(2), d(5)]);
        assert_eq!(wanted(&f, 1), [d(5)]);
        assert!(wanted(&f, 0).is_empty());
        assert_eq!(wanted(&f, 99), [d(2), d(5)]);
    }

    // §6.1 "Names": the pre-image's name when it has one, else the
    // post-image's; of several, the first by bytes; only the first
    // `locate` findings; cut to 4096 bytes at a character boundary.
    #[test]
    fn s6_1_fill_names_from_pre_then_post() {
        let pre = side(&[(1, "a::f()"), (3, "z"), (3, "b")]);
        let post = side(&[(1, "post::f()"), (2, "only_post")]);
        let k = |n| finding(FindingKind::Added, n);
        let mut f = vec![
            k(Some(d(1))),
            k(Some(d(2))),
            k(Some(d(3))),
            k(Some(d(4))),
            k(None),
            k(Some(d(1))),
        ];
        fill(&mut f, 5, Some(&pre), &post);
        let names: Vec<&str> = f.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, ["a::f()", "only_post", "b", "", "", ""]);
        assert!(f[5].location.is_none(), "past `locate`: untouched");
        let mut g = vec![k(Some(d(1)))];
        fill(&mut g, 1, None, &post);
        assert_eq!(g[0].name, "post::f()");
        let mut g = vec![k(Some(d(1)))];
        fill(&mut g, 0, Some(&pre), &post);
        assert_eq!(g[0].name, "");

        let long = format!("{}é", "x".repeat(4095));
        let mut g = vec![k(Some(d(7)))];
        fill(&mut g, 1, None, &side(&[(7, &long)]));
        assert_eq!(g[0].name, "x".repeat(4095));
        let exact = "y".repeat(4097);
        let mut g = vec![k(Some(d(7)))];
        fill(&mut g, 1, None, &side(&[(7, &exact)]));
        assert_eq!(g[0].name, "y".repeat(4096));
    }

    // §6.1 "The site of a difference": `d` is the first differing offset
    // (here 5); the site on each side is the last mark at or before it (a
    // mark at `d` itself counts; one after does not).
    #[test]
    fn s6_1_site_is_the_last_mark_at_or_before_the_first_difference() {
        let stream_a = [1u8, 2, 3, 4, 5, 6, 7, 8];
        let mut stream_b = stream_a;
        stream_b[5] = 9;
        let marks = |at_d: bool| {
            vec![
                mark(0, "function", 1),
                mark(3, "CallExpr", 2),
                if at_d { mark(5, "IntegerLiteral", 3) } else { mark(4, "DeclRefExpr", 3) },
                mark(6, "ReturnStmt", 4),
            ]
        };
        let pre = with_unit(side(&[]), 1, 10, GroupClass::Declared, &stream_a, marks(true), vec![]);
        let post =
            with_unit(side(&[]), 1, 11, GroupClass::Declared, &stream_b, marks(false), vec![]);
        let mut f = vec![changed(1, 10, 11)];
        fill(&mut f, 1, Some(&pre), &post);
        let w = f[0].location.clone().expect("a location");
        assert_eq!((w.pre.node.as_str(), w.pre.line), ("IntegerLiteral", 3), "mark at d");
        assert_eq!((w.post.node.as_str(), w.post.line), ("DeclRefExpr", 3), "mark before d");
        assert_eq!(w.pre.file, b"t.cpp");
        assert_eq!(w.pre.what, "");
        assert_eq!(first_difference(&stream_a, &stream_b), 5);
        // One a prefix of the other: the length of the shorter.
        assert_eq!(first_difference(&[1, 2], &[1, 2, 3]), 2);
        assert_eq!(first_difference(&[], &[1]), 0);
    }

    // §6.1: `what` is `refers to <name>` when `d` lies inside a reference
    // of the trace (its first and last byte), the name in that image, and
    // the key in hexadecimal when the image has no name for it; outside
    // every reference it is empty.
    #[test]
    fn s6_1_what_names_the_reference_at_the_difference() {
        let mut pre = vec![0u8; 40];
        pre[0] = 7;
        let marks = || vec![mark(0, "function", 1), mark(1, "CallExpr", 2)];
        let refs = |key: u8| vec![RefAt { offset: 4, len: 32, key: d(key) }];
        for (at, inside) in [(4usize, true), (35, true), (3, false), (36, false)] {
            let mut post = pre.clone();
            post[at] = 1;
            let pre_side = with_unit(
                side(&[(20, "lib::f(long)")]),
                1,
                10,
                GroupClass::Declared,
                &pre,
                marks(),
                refs(20),
            );
            let post_side =
                with_unit(side(&[]), 1, 11, GroupClass::Declared, &post, marks(), refs(21));
            let mut found = vec![changed(1, 10, 11)];
            fill(&mut found, 1, Some(&pre_side), &post_side);
            let w = found[0].location.clone().expect("a location");
            if inside {
                assert_eq!(w.pre.what, "refers to lib::f(long)", "byte {at}");
                assert_eq!(w.post.what, format!("refers to {}", d(21)), "byte {at}");
            } else {
                assert_eq!((w.pre.what.as_str(), w.post.what.as_str()), ("", ""), "byte {at}");
            }
            assert_eq!(w.pre.node, "CallExpr");
        }
    }

    // §6.1 "Other findings": removed and added take the unit's first mark
    // and the empty site; a changed finding with `pre` absent takes both
    // first marks of the smallest digest; a changed finding of §4 step 4
    // (declared on one side, derived on the other) takes both first marks,
    // `what` empty; an indeterminate one the first mark of the
    // indeterminate unit with the walk's cause; a unit without a trace
    // gives no location.
    #[test]
    fn s6_1_other_findings() {
        let unit = |s: Side, key: u8, digest: u8, class: GroupClass, line: u64| {
            with_unit(
                s,
                key,
                digest,
                class,
                &[digest, 1, 2],
                vec![mark(0, "function", line), mark(1, "CallExpr", line + 1)],
                vec![],
            )
        };
        let pre = unit(side(&[]), 1, 10, GroupClass::Declared, 5);
        let pre = unit(pre, 2, 12, GroupClass::Declared, 7);
        let pre = unit(pre, 2, 11, GroupClass::Declared, 9);
        let post = unit(side(&[]), 3, 13, GroupClass::Declared, 20);
        let post = unit(post, 2, 14, GroupClass::Declared, 30);
        let post = unit(post, 4, 15, GroupClass::Derived, 40);
        let pre = unit(pre, 4, 16, GroupClass::Declared, 50);

        let mut f = vec![
            finding(FindingKind::Removed, Some(d(1))),
            finding(FindingKind::Added, Some(d(3))),
            Finding { post: Some(d(14)), ..finding(FindingKind::Changed, Some(d(2))) },
            changed(4, 16, 15),
        ];
        fill(&mut f, 4, Some(&pre), &post);
        let at = |w: &Option<Where>| {
            let w = w.clone().expect("a location");
            ((w.pre.node, w.pre.line, w.pre.what), (w.post.node, w.post.line, w.post.what))
        };
        let first = |l: u64| ("function".to_owned(), l, String::new());
        let empty = (String::new(), 0, String::new());
        assert_eq!(at(&f[0].location), (first(5), empty.clone()));
        assert_eq!(at(&f[1].location), (empty.clone(), first(20)));
        assert_eq!(at(&f[2].location), (first(9), first(30)), "the smallest digest");
        assert_eq!(at(&f[3].location), (first(50), first(40)), "step 4: first marks");
        assert_eq!(f[1].location.clone().unwrap().pre, Site::default());

        // Indeterminate: the pre side has an indeterminate unit, the post
        // side none.
        let mut pre_i = unit(side(&[]), 6, 17, GroupClass::Declared, 60);
        pre_i.units[0].indeterminate = true;
        pre_i.causes = vec![(d(6), "unsupported ExprKind/CXXReflectExpr".to_owned())];
        let post_i = unit(side(&[]), 6, 18, GroupClass::Declared, 70);
        let mut g = vec![finding(FindingKind::Indeterminate, Some(d(6)))];
        fill(&mut g, 1, Some(&pre_i), &post_i);
        let w = g[0].location.clone().expect("a location");
        assert_eq!(
            (w.pre.node.as_str(), w.pre.line, w.pre.what.as_str()),
            ("function", 60, "unsupported ExprKind/CXXReflectExpr")
        );
        assert_eq!(w.post, Site::default());

        // A unit without a trace: no location.
        let mut bare = side(&[]);
        bare.units.push(UnitAt {
            key: d(1),
            digest: d(10),
            class: GroupClass::Declared,
            indeterminate: false,
        });
        let mut h = vec![finding(FindingKind::Removed, Some(d(1)))];
        fill(&mut h, 1, Some(&bare), &post);
        assert!(h[0].location.is_none());
        let mut h = vec![changed(1, 10, 11)];
        fill(&mut h, 1, Some(&bare), &unit(side(&[]), 1, 11, GroupClass::Declared, 1));
        assert!(h[0].location.is_none());
    }
}
