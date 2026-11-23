//! `spec/edits.md` §7.1 "What the edits mean": the text of a range, the
//! exchange, the wrap, copies across files, obligations; a chain of copies
//! a thousand deep, a doubling chain that is too large, a cycle and a
//! buried edit; and a model: the definition's own recursion on generated
//! sets, compared with `build`.

mod common;

use std::collections::BTreeSet;

use common::{Mem, Rng, copy, edit, id_of, key_id, lit, set, spell};
use slip_apply::{ApplyError, Built, POST_MAX, build, plan};
use slip_model::edits::{KeyId, Piece};
use slip_model::editset::{Edit, EditSet};
use slip_model::overlay::Obligation;
use slip_model::{Digest, FileRef};

fn built(files: &[(&str, &[u8])], edits: Vec<Edit>) -> Built {
    let (s, id, mem) = set(files, edits);
    build(&s, id, &mem).expect("builds")
}

fn ob(file: u64, lo: u64, hi: u64, key: u8) -> Obligation {
    Obligation { file, lo, hi, key: key_id(key) }
}

// §7.1: the text of a copied range has the edits properly inside it
// applied; an edit of exactly the copied range is not applied to the copy.
#[test]
fn s7_1_text_of_a_range_with_edits_inside() {
    // "f(a + b);": the call's argument copied after a rename inside it.
    let pre: &[u8] = b"f(a + b); g();";
    let b = built(
        &[("/t/x", pre)],
        vec![edit(0, 2, 3, vec![spell(1, b"alpha")]), edit(0, 10, 11, vec![copy(0, 2, 7)])],
    );
    assert_eq!(b.blobs[0].1, b"f(alpha + b); alpha + b();");
    assert_eq!(b.overlay.spellings, vec![ob(0, 2, 7, 1), ob(0, 14, 19, 1)]);
}

// §7.1: the exchange of two ranges, with an edit inside one of them; the
// edit is applied where its range is carried.
#[test]
fn s7_1_exchange() {
    let pre: &[u8] = b"f(aa, bb)";
    let b = built(
        &[("/t/x", pre)],
        vec![
            edit(0, 2, 4, vec![copy(0, 6, 8)]),
            edit(0, 6, 8, vec![copy(0, 2, 4)]),
            edit(0, 2, 3, vec![spell(7, b"X")]),
        ],
    );
    assert_eq!(b.blobs[0].1, b"f(bb, Xa)");
    assert_eq!(b.overlay.spellings, vec![ob(0, 6, 7, 7)]);
}

// §7.1: the wrap: an edit that copies its own range, with an edit inside.
#[test]
fn s7_1_wrap() {
    let pre: &[u8] = b"x = h(y);";
    let b = built(
        &[("/t/x", pre)],
        vec![
            edit(0, 4, 8, vec![lit(b"g("), copy(0, 4, 8), lit(b")")]),
            edit(0, 6, 7, vec![spell(2, b"z")]),
        ],
    );
    assert_eq!(b.blobs[0].1, b"x = g(h(z));");
    assert_eq!(b.overlay.spellings, vec![ob(0, 8, 9, 2)]);
}

// §7.1: a copy from another file reads that file's pre-image with its
// edits applied; a file only copied from is no overlay file.
#[test]
fn s7_1_copy_from_another_file() {
    let b = built(
        &[("/t/a", b"int v;"), ("/t/b", b"#X#"), ("/t/c", b"long w;")],
        vec![
            edit(0, 4, 5, vec![spell(1, b"value")]),
            edit(1, 1, 2, vec![copy(0, 0, 6), lit(b" "), copy(2, 0, 7)]),
        ],
    );
    let paths: Vec<_> = b.overlay.files.iter().map(|f| f.path.clone()).collect();
    assert_eq!(paths, vec![b"/t/a".to_vec(), b"/t/b".to_vec()]);
    assert_eq!(b.blobs[0].1, b"int value;");
    assert_eq!(b.blobs[1].1, b"#int value; long w;#");
    assert_eq!(b.overlay.spellings, vec![ob(0, 4, 9, 1), ob(1, 5, 10, 1)]);
}

// §7.1 step 5: one edit carried to three places gives three obligations,
// each in its post-image's offsets.
#[test]
fn s7_1_one_edit_carried_to_three_places() {
    let b = built(
        &[("/t/a", b"(n)"), ("/t/b", b"1 2")],
        vec![
            edit(0, 1, 2, vec![spell(5, b"name")]),
            edit(1, 0, 1, vec![copy(0, 0, 3)]),
            edit(1, 2, 3, vec![copy(0, 0, 3)]),
        ],
    );
    assert_eq!(b.blobs[0].1, b"(name)");
    assert_eq!(b.blobs[1].1, b"(name) (name)");
    assert_eq!(b.overlay.spellings, vec![ob(0, 1, 5, 5), ob(1, 1, 5, 5), ob(1, 8, 12, 5)]);
}

// §7.1 step 5: a spelled piece after a copy whose length depends on the
// edits the copy carries.
#[test]
fn s7_1_obligation_after_a_copy_of_unknown_length() {
    // [3,5) is outermost, and also carried by the copy of [2,5).
    let b = built(
        &[("/t/a", b"ab cd")],
        vec![
            edit(0, 0, 1, vec![copy(0, 2, 5), spell(3, b"K")]),
            edit(0, 3, 5, vec![lit(b"longer")]),
        ],
    );
    assert_eq!(b.blobs[0].1, b" longerKb longer");
    assert_eq!(b.overlay.spellings, vec![ob(0, 7, 8, 3)]);
}

/// A chain of `depth` nested edits, each wrapping a copy of its own range.
fn wrap_chain(depth: u64) -> (EditSet, Digest, Mem) {
    let size = 2 * depth;
    let pre = vec![b'.'; size as usize];
    let edits = (0..depth)
        .map(|k| edit(0, k, size - k, vec![lit(b"("), copy(0, k, size - k), lit(b")")]))
        .collect();
    let leaked: &'static [u8] = Box::leak(pre.into_boxed_slice());
    set(&[("/t/deep", leaked)], edits)
}

// §7.1 step 3: a chain of copies a thousand deep is built without
// recursion on the call stack; deeper still on a small stack.
#[test]
fn s7_1_chain_of_copies_a_thousand_deep() {
    let (s, id, mem) = wrap_chain(1000);
    let b = build(&s, id, &mem).unwrap();
    let post = &b.blobs[0].1;
    assert_eq!(post.len(), 4000);
    let want = [b"(.".repeat(999), b"(..)".to_vec(), b".)".repeat(999)].concat();
    assert_eq!(post, &want);
    let lengths = plan(&s).unwrap();
    assert_eq!(lengths[0], 4000);
    assert_eq!(lengths[999], 4);
    let deep = std::thread::Builder::new()
        .stack_size(256 * 1024)
        .spawn(|| {
            let (s, id, mem) = wrap_chain(20_000);
            build(&s, id, &mem).map(|b| b.blobs[0].1.len())
        })
        .unwrap()
        .join()
        .expect("no stack overflow");
    assert_eq!(deep, Ok(80_000));
}

// §7.1 step 3: a chain of forty doubling copies is `too_large` from
// lengths alone, before anything is read or allocated.
#[test]
fn s7_1_doubling_chain_is_too_large_without_being_built() {
    let depth = 40u64;
    let size = 2 * depth;
    let pre = vec![b'.'; size as usize];
    let leaked: &'static [u8] = Box::leak(pre.into_boxed_slice());
    let edits = (0..depth)
        .map(|k| edit(0, k, size - k, vec![copy(0, k, size - k), copy(0, k, size - k)]))
        .collect();
    let (s, id, _) = set(&[("/t/double", leaked)], edits);
    assert_eq!(plan(&s), Err(ApplyError::TooLarge(b"/t/double".to_vec())));
    // An empty pre-image: build fails before it reads a file.
    assert_eq!(build(&s, id, &Mem::default()), Err(ApplyError::TooLarge(b"/t/double".to_vec())));
}

// §7.1 step 3: the bound is 2^31 bytes; exactly 2^31 passes. Decided from
// the FileRef's size, without bytes.
#[test]
fn s7_1_too_large_bound_is_two_to_the_31() {
    let mk = |text: &[u8]| {
        let (mut s, _, _) = set(&[("/t/big", b"x")], vec![edit(0, 0, 1, vec![lit(text)])]);
        s.files[0] = FileRef { path: b"/t/big".to_vec(), size: POST_MAX, content: Digest([0; 32]) };
        s
    };
    assert_eq!(plan(&mk(b"a")), Ok(vec![1]));
    assert_eq!(plan(&mk(b"ab")), Err(ApplyError::TooLarge(b"/t/big".to_vec())));
}

fn invalid(s: &EditSet) -> String {
    match plan(s) {
        Err(ApplyError::Invalid(m)) => m,
        other => panic!("expected Invalid, got {other:?}"),
    }
}

// §7.1 step 2: a cycle of "draws on", of one edit and of two, is Invalid.
#[test]
fn s7_1_cycles_are_invalid() {
    let (one, ..) = set(&[("/t/a", b"abc")], vec![edit(0, 1, 2, vec![copy(0, 0, 3)])]);
    assert!(invalid(&one).contains("draws on itself"), "{}", invalid(&one));
    let (two, ..) = set(
        &[("/t/a", b"abc"), ("/t/b", b"xyz")],
        vec![edit(0, 0, 1, vec![copy(1, 0, 3)]), edit(1, 1, 2, vec![copy(0, 0, 2)])],
    );
    assert!(invalid(&two).contains("draws on itself"));
}

// §5.4: "draws on" reaches every edit properly inside a copied range, the
// nested ones too: a cycle through an edit inside a carried edit.
#[test]
fn s7_1_cycle_through_nesting_is_invalid() {
    let (s, ..) = set(
        &[("/t/a", b"abcdef"), ("/t/b", b"uv")],
        vec![
            edit(0, 0, 6, vec![copy(1, 0, 2)]),
            edit(0, 1, 4, vec![lit(b"Y")]),
            edit(0, 2, 3, vec![copy(1, 0, 2)]),
            edit(1, 0, 1, vec![copy(0, 0, 5)]),
        ],
    );
    assert!(invalid(&s).contains("draws on itself"), "{}", invalid(&s));
}

// §7.1 step 2, §5.5: an edit inside another that no copy carries is not
// effective; so is one carried only by an edit that is not effective.
#[test]
fn s7_1_buried_edits_are_invalid() {
    let (s, ..) = set(
        &[("/t/a", b"abcde")],
        vec![edit(0, 0, 5, vec![lit(b"x")]), edit(0, 1, 2, vec![lit(b"y")])],
    );
    let m = invalid(&s);
    assert!(m.contains("edits[1]") && m.contains("not effective"), "{m}");
    // [2,3) is carried by [1,4)'s copy, but [1,4) is buried in [0,5).
    let (s, ..) = set(
        &[("/t/a", b"abcde")],
        vec![
            edit(0, 0, 5, vec![lit(b"x")]),
            edit(0, 1, 4, vec![copy(0, 1, 4)]),
            edit(0, 2, 3, vec![lit(b"y")]),
        ],
    );
    assert!(invalid(&s).contains("not effective"));
    // A copy of a range carries only the edits within no other edit inside
    // it: [2,3) inside [1,4) is not carried by a copy of [0,5).
    let (s, ..) = set(
        &[("/t/a", b"abcde"), ("/t/b", b"Z")],
        vec![
            edit(0, 1, 4, vec![lit(b"q")]),
            edit(0, 2, 3, vec![lit(b"y")]),
            edit(1, 0, 1, vec![copy(0, 0, 5)]),
        ],
    );
    assert!(invalid(&s).contains("edits[1]"), "{}", invalid(&s));
}

// §7.1 step 2: an edit set that breaks §6.1 is Invalid with its reader's
// message.
#[test]
fn s7_1_invariants_are_checked() {
    let (mut s, ..) = set(&[("/t/a", b"abc")], vec![edit(0, 0, 2, vec![lit(b"x")])]);
    s.edits.push(edit(0, 1, 3, vec![lit(b"y")]));
    assert!(invalid(&s).contains("crosses"), "{}", invalid(&s));
}

// ---------------------------------------------------------------------------
// The model: §7.1's definition by its own recursion.

struct Model<'a> {
    set: &'a EditSet,
    pre: &'a [Vec<u8>],
}

type Landing = (u64, u64, KeyId);

impl Model<'_> {
    fn inside(&self, d: &Edit, file: u64, lo: u64, hi: u64) -> bool {
        d.file == file && lo <= d.lo && d.hi <= hi && (d.lo, d.hi) != (lo, hi)
    }

    /// "draws on": `e` copies a range that `d` is properly inside.
    fn draws_on(&self, e: &Edit, d: &Edit) -> bool {
        e.pieces.iter().any(|p| matches!(p, Piece::Copy(r) if self.inside(d, r.file, r.lo, r.hi)))
    }

    fn cyclic(&self) -> bool {
        let n = self.set.edits.len();
        // Transitive closure, then a self-loop.
        let mut reach = vec![vec![false; n]; n];
        for (i, e) in self.set.edits.iter().enumerate() {
            for (j, d) in self.set.edits.iter().enumerate() {
                reach[i][j] = self.draws_on(e, d);
            }
        }
        for k in 0..n {
            for i in 0..n {
                if reach[i][k] {
                    for j in 0..n {
                        reach[i][j] |= reach[k][j];
                    }
                }
            }
        }
        (0..n).any(|i| reach[i][i])
    }

    /// The edits properly inside the range, within no other edit properly inside it.
    fn top(&self, file: u64, lo: u64, hi: u64) -> Vec<usize> {
        let es = &self.set.edits;
        (0..es.len())
            .filter(|&i| self.inside(&es[i], file, lo, hi))
            .filter(|&i| {
                !(0..es.len()).any(|j| {
                    j != i
                        && self.inside(&es[j], file, lo, hi)
                        && self.inside(&es[i], file, es[j].lo, es[j].hi)
                })
            })
            .collect()
    }

    fn effective(&self) -> Vec<bool> {
        let es = &self.set.edits;
        let outer: Vec<bool> =
            es.iter().map(|e| !es.iter().any(|d| self.inside(e, d.file, d.lo, d.hi))).collect();
        let mut eff = outer;
        loop {
            let mut changed = false;
            for i in 0..es.len() {
                if !eff[i] {
                    continue;
                }
                for p in &es[i].pieces {
                    if let Piece::Copy(r) = p {
                        for m in self.top(r.file, r.lo, r.hi) {
                            if !eff[m] {
                                eff[m] = true;
                                changed = true;
                            }
                        }
                    }
                }
            }
            if !changed {
                return eff;
            }
        }
    }

    fn text(&self, file: u64, lo: u64, hi: u64) -> (Vec<u8>, Vec<Landing>) {
        let mut tops = self.top(file, lo, hi);
        tops.sort_by_key(|&i| self.set.edits[i].lo);
        let mut out = Vec::new();
        let mut spells = Vec::new();
        let mut at = lo;
        let bytes = &self.pre[file as usize];
        for i in tops {
            let e = &self.set.edits[i];
            out.extend_from_slice(&bytes[at as usize..e.lo as usize]);
            let (x, xs) = self.expansion(i);
            let shift = out.len() as u64;
            spells.extend(xs.into_iter().map(|(l, h, k)| (l + shift, h + shift, k)));
            out.extend(x);
            at = e.hi;
        }
        out.extend_from_slice(&bytes[at as usize..hi as usize]);
        (out, spells)
    }

    /// The file's bytes with each outermost edit replaced by its expansion.
    fn post_image(&self, file: u64) -> (Vec<u8>, Vec<Landing>) {
        let es = &self.set.edits;
        let mut outer: Vec<usize> = (0..es.len())
            .filter(|&i| {
                es[i].file == file && !es.iter().any(|d| self.inside(&es[i], d.file, d.lo, d.hi))
            })
            .collect();
        outer.sort_by_key(|&i| es[i].lo);
        let bytes = &self.pre[file as usize];
        let (mut out, mut spells, mut at) = (Vec::new(), Vec::new(), 0);
        for i in outer {
            out.extend_from_slice(&bytes[at as usize..es[i].lo as usize]);
            let (x, xs) = self.expansion(i);
            let shift = out.len() as u64;
            spells.extend(xs.into_iter().map(|(l, h, k)| (l + shift, h + shift, k)));
            out.extend(x);
            at = es[i].hi;
        }
        out.extend_from_slice(&bytes[at as usize..]);
        (out, spells)
    }

    fn expansion(&self, i: usize) -> (Vec<u8>, Vec<Landing>) {
        let mut out = Vec::new();
        let mut spells = Vec::new();
        for p in &self.set.edits[i].pieces {
            match p {
                Piece::Literal(b) => out.extend_from_slice(b),
                Piece::Spelled { key, text } => {
                    spells.push((out.len() as u64, (out.len() + text.len()) as u64, *key));
                    out.extend_from_slice(text);
                }
                Piece::Copy(r) => {
                    let (t, ts) = self.text(r.file, r.lo, r.hi);
                    let shift = out.len() as u64;
                    spells.extend(ts.into_iter().map(|(l, h, k)| (l + shift, h + shift, k)));
                    out.extend(t);
                }
            }
        }
        (out, spells)
    }
}

/// A random accepted edit set: files of random bytes, a laminar family of
/// edits per file, pieces of every kind, copies of ranges no edit crosses.
/// Nested edits are often carried by a copy of the range around them.
fn generate(rng: &mut Rng) -> Option<(EditSet, Digest, Mem, Vec<Vec<u8>>)> {
    let nfiles = 1 + rng.below(3) as usize;
    let names = ["/t/a.cc", "/t/b.h", "/u/c.h"];
    let pre: Vec<Vec<u8>> = (0..nfiles).map(|_| rng.bytes_n(1, 24, b"ab\n\r\0\xff")).collect();
    let crosses = |a: (u64, u64), b: (u64, u64)| {
        a.0 < b.1 && b.0 < a.1 && !(b.0 <= a.0 && a.1 <= b.1) && !(a.0 <= b.0 && b.1 <= a.1)
    };
    let mut ranges: Vec<Vec<(u64, u64)>> = vec![Vec::new(); nfiles];
    for (f, bytes) in pre.iter().enumerate() {
        let n = bytes.len() as u64;
        for _ in 0..rng.below(6) {
            let lo = rng.below(n);
            let hi = lo + 1 + rng.below(n - lo);
            if ranges[f].iter().all(|&r| r != (lo, hi) && !crosses(r, (lo, hi))) {
                ranges[f].push((lo, hi));
            }
        }
    }
    let random_copy = |rng: &mut Rng| -> Option<Piece> {
        let f = rng.below(nfiles as u64) as usize;
        let n = pre[f].len() as u64;
        let lo = rng.below(n);
        let hi = lo + 1 + rng.below(n - lo);
        ranges[f].iter().all(|&r| !crosses(r, (lo, hi))).then(|| copy(f as u64, lo, hi))
    };
    let mut edits = Vec::new();
    for f in 0..nfiles {
        for &(lo, hi) in &ranges[f] {
            let mut pieces: Vec<Piece> = Vec::new();
            let has_inner = ranges[f].iter().any(|&r| r != (lo, hi) && lo <= r.0 && r.1 <= hi);
            for _ in 0..rng.below(4) {
                let p = match rng.below(4) {
                    0 => lit(&rng.bytes_n(1, 3, b"XY\n")),
                    1 => spell(1 + rng.below(3) as u8, &rng.bytes_n(1, 3, b"kq")),
                    2 if has_inner => copy(f as u64, lo, hi),
                    _ => match random_copy(rng) {
                        Some(p) => p,
                        None => continue,
                    },
                };
                if matches!((&p, pieces.last()), (Piece::Literal(_), Some(Piece::Literal(_)))) {
                    continue;
                }
                pieces.push(p);
            }
            edits.push(edit(f as u64, lo, hi, pieces));
        }
    }
    // Every file must be named by a record (§6.1).
    let mut named = BTreeSet::new();
    for e in &edits {
        named.insert(e.file);
        for p in &e.pieces {
            if let Piece::Copy(r) = p {
                named.insert(r.file);
            }
        }
    }
    if edits.is_empty() || named.len() != nfiles {
        return None;
    }
    let files: Vec<(&str, &[u8])> = (0..nfiles).map(|f| (names[f], pre[f].as_slice())).collect();
    let (s, id, mem) = set(&files, edits);
    let _ = id_of(&s);
    Some((s, id, mem, pre))
}

// §7.1, the model: for generated sets (fixed seed), `plan` and `build`
// agree with the definition's own recursion: the same refusals (a cycle,
// then an edit that is not effective), the same expansion lengths, the
// same post-images and the same obligations.
#[test]
fn s7_1_model_agrees_with_build() {
    let mut rng = Rng(0x0063_c0de);
    let (mut ok, mut cyclic, mut buried, mut copies) = (0, 0, 0, 0);
    let mut cases = 0;
    while cases < 4000 {
        let Some((s, id, mem, pre)) = generate(&mut rng) else { continue };
        cases += 1;
        let m = Model { set: &s, pre: &pre };
        let got = build(&s, id, &mem);
        if m.cyclic() {
            cyclic += 1;
            assert!(
                matches!(&got, Err(ApplyError::Invalid(t)) if t.contains("draws on itself")),
                "{s:?} {got:?}"
            );
            continue;
        }
        if m.effective().iter().any(|&x| !x) {
            buried += 1;
            assert!(
                matches!(&got, Err(ApplyError::Invalid(t)) if t.contains("not effective")),
                "{s:?} {got:?}"
            );
            continue;
        }
        ok += 1;
        if s.edits.iter().any(|e| e.pieces.iter().any(|p| matches!(p, Piece::Copy(_)))) {
            copies += 1;
        }
        let b = got.unwrap_or_else(|e| panic!("{s:?}: {e:?}"));
        let lengths = plan(&s).unwrap();
        for i in 0..s.edits.len() {
            assert_eq!(lengths[i], m.expansion(i).0.len() as u64, "edit {i} of {s:?}");
        }
        let mut want_spells = Vec::new();
        let mut k = 0;
        for (f, file) in s.files.iter().enumerate() {
            if !s.edits.iter().any(|e| e.file == f as u64) {
                continue;
            }
            let (post, spells) = m.post_image(f as u64);
            assert_eq!(b.overlay.files[k].path, file.path);
            assert_eq!(b.blobs[k].1, post, "post-image of {:?} in {s:?}", file.path);
            want_spells.extend(spells.into_iter().map(|(lo, hi, key)| Obligation {
                file: k as u64,
                lo,
                hi,
                key,
            }));
            k += 1;
        }
        assert_eq!(b.overlay.files.len(), k);
        want_spells.sort();
        assert_eq!(b.overlay.spellings, want_spells, "{s:?}");
    }
    eprintln!("model: {ok} built ({copies} with copies), {cyclic} cyclic, {buried} buried");
    assert!(
        ok > 1000 && copies > 300 && cyclic > 50 && buried > 50,
        "{ok} {copies} {cyclic} {buried}"
    );
}
