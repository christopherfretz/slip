//! `spec/edits.md` §7.1: splicing one file, and `build` on edit sets
//! without copies or nesting: where edits sit, the bytes that pass
//! through, a stale file, a set that changes nothing.

mod common;

use common::{Mem, Rng, edit, lit, set, spell};
use slip_apply::{ApplyError, build, splice};
use slip_model::hash::content_digest;

fn post(files: &[(&str, &[u8])], edits: Vec<slip_model::editset::Edit>) -> Vec<Vec<u8>> {
    let (s, id, mem) = set(files, edits);
    let built = build(&s, id, &mem).expect("builds");
    built
        .blobs
        .into_iter()
        .map(|(d, b)| {
            assert_eq!(d, content_digest(&b));
            b
        })
        .collect()
}

// §7.1 step 3: an empty list returns the input; empty input.
#[test]
fn s7_1_splice_nothing_and_empty_input() {
    assert_eq!(splice(b"abc", &[]).unwrap(), b"abc");
    assert_eq!(splice(b"", &[]).unwrap(), b"");
    assert_eq!(splice(b"", &[(0, 0, b"x")]).unwrap(), b"x");
}

// §7.1 step 3: at the start, at the end, adjacent ranges.
#[test]
fn s7_1_splice_start_end_adjacent() {
    assert_eq!(splice(b"abcdef", &[(0, 1, b"X")]).unwrap(), b"Xbcdef");
    assert_eq!(splice(b"abcdef", &[(5, 6, b"XY")]).unwrap(), b"abcdeXY");
    assert_eq!(splice(b"abcdef", &[(1, 3, b"1"), (3, 4, b"22"), (4, 6, b"")]).unwrap(), b"a122");
    assert_eq!(splice(b"abcdef", &[(0, 6, b"")]).unwrap(), b"");
    // An insertion (empty range) before a replacement at the same offset.
    assert_eq!(splice(b"abc", &[(1, 1, b"+"), (1, 2, b"B")]).unwrap(), b"a+Bc");
}

// §7.4: edits out of order, overlapping, past the end, or two at one
// empty range are `Invalid`.
#[test]
fn s7_4_splice_refuses_what_is_not_ascending_and_disjoint() {
    let bad: [&[(u64, u64, &[u8])]; 6] = [
        &[(2, 3, b""), (0, 1, b"")],
        &[(0, 2, b""), (1, 3, b"")],
        &[(0, 4, b"")],
        &[(4, 4, b"")],
        &[(2, 1, b"")],
        &[(1, 1, b"a"), (1, 1, b"b")],
    ];
    for edits in bad {
        assert!(matches!(splice(b"abc", edits), Err(ApplyError::Invalid(_))), "{edits:?}");
    }
    // Touching is not overlapping.
    assert!(splice(b"abc", &[(0, 1, b""), (1, 3, b"")]).is_ok());
}

// §7.1 step 3: no byte is added, removed or recoded: CRLF, a byte-order
// mark, no final newline, invalid UTF-8 and NUL pass through.
#[test]
fn s7_1_bytes_outside_the_edits_pass_through() {
    let pre: &[u8] = b"\xEF\xBB\xBFint a;\r\nint \xFF\xFE b;\r\n\0x = a;";
    let a = pre.windows(1).position(|w| w == b"a").unwrap() as u64;
    let out = post(&[("/t/f.cc", pre)], vec![edit(0, a, a + 1, vec![spell(1, b"alpha")])]);
    let mut want = pre.to_vec();
    want.splice(a as usize..a as usize + 1, b"alpha".iter().copied());
    assert_eq!(out, vec![want]);
    // The last byte, with no newline after it.
    let n = pre.len() as u64;
    let out = post(&[("/t/f.cc", pre)], vec![edit(0, n - 1, n, vec![lit(b"b")])]);
    assert_eq!(&out[0][..out[0].len() - 1], &pre[..pre.len() - 1]);
    assert_eq!(out[0].last(), Some(&b'b'));
}

// §7.1 step 1: a file that cannot be read, or whose size or digest is not
// the FileRef's, is `stale_file`, and nothing is built.
#[test]
fn s7_1_stale_file() {
    let (s, id, mem) = set(
        &[("/t/a", b"hello"), ("/t/b", b"world")],
        vec![edit(0, 0, 1, vec![lit(b"H")]), edit(1, 0, 1, vec![lit(b"W")])],
    );
    let mut changed = mem.clone();
    changed.0.insert(b"/t/b".to_vec(), b"worle".to_vec());
    assert_eq!(build(&s, id, &changed), Err(ApplyError::StaleFile(b"/t/b".to_vec())));
    let mut longer = mem.clone();
    longer.0.insert(b"/t/a".to_vec(), b"hello!".to_vec());
    assert_eq!(build(&s, id, &longer), Err(ApplyError::StaleFile(b"/t/a".to_vec())));
    let mut gone = mem.clone();
    gone.0.remove(b"/t/a".as_slice());
    assert_eq!(build(&s, id, &gone), Err(ApplyError::StaleFile(b"/t/a".to_vec())));
    assert!(build(&s, id, &mem).is_ok());
    // The sink is called in path order; a stale second file fails after
    // the first was given (spec/landed/edits/WP-063.md §2).
    let mut seen = Vec::new();
    let r = slip_apply::build_into(&s, id, &changed, &mut |f, _| {
        seen.push(f.path.clone());
        Ok(())
    });
    assert_eq!(r, Err(ApplyError::StaleFile(b"/t/b".to_vec())));
    assert_eq!(seen, vec![b"/t/a".to_vec()]);
}

// §7.1: a file whose edits leave its bytes as they were is an entry with
// post equal to pre, not an error.
#[test]
fn s7_1_a_set_that_changes_nothing() {
    let (s, id, mem) = set(&[("/t/a", b"keep")], vec![edit(0, 1, 3, vec![lit(b"ee")])]);
    let built = build(&s, id, &mem).unwrap();
    assert_eq!(built.overlay.files.len(), 1);
    assert_eq!(built.overlay.files[0].pre, built.overlay.files[0].post);
    assert_eq!(built.blobs[0].1, b"keep");
    assert_eq!(built.overlay.edits, id);
}

// §7.4: build checks the id it is given, and refuses a refused set.
#[test]
fn s7_4_build_checks_the_set_id_and_state() {
    let (mut s, id, mem) = set(&[("/t/a", b"x")], vec![edit(0, 0, 1, vec![lit(b"y")])]);
    assert!(matches!(build(&s, slip_model::Digest([9; 32]), &mem), Err(ApplyError::Invalid(_))));
    s.state = slip_model::editset::SetState::Refused;
    assert_eq!(build(&s, id, &mem), Err(ApplyError::NotAccepted));
    assert_eq!(slip_apply::plan(&s), Err(ApplyError::NotAccepted));
}

/// The splice of disjoint edits, byte by byte: each offset of `pre` is
/// copied unless an edit's range holds it, and an edit's text is written
/// when its range starts. An independent reading of §7.1 step 3.
fn naive(pre: &[u8], edits: &[(u64, u64, Vec<u8>)]) -> Vec<u8> {
    let mut out = Vec::new();
    for at in 0..=pre.len() as u64 {
        for (lo, _, t) in edits.iter().filter(|e| e.0 == at) {
            let _ = lo;
            out.extend_from_slice(t);
        }
        if at < pre.len() as u64 && !edits.iter().any(|&(lo, hi, _)| lo <= at && at < hi) {
            out.push(pre[at as usize]);
        }
    }
    out
}

// §7.1 step 3, property: random disjoint edits over random bytes (CR, LF,
// NUL, invalid UTF-8 included) against the byte-by-byte reference.
#[test]
fn s7_1_splice_property_against_a_reference() {
    let mut rng = Rng(0x5eed_0063);
    for _ in 0..3000 {
        let len = rng.below(40) as usize;
        let pre = rng.bytes(len, b"ab\r\n\0\xff ");
        let mut edits = Vec::new();
        let mut at = 0u64;
        while at <= len as u64 && rng.chance(2, 3) {
            let lo = at + rng.below(4);
            if lo > len as u64 {
                break;
            }
            let hi = (lo + rng.below(5)).min(len as u64);
            if hi == lo
                && edits.last().is_some_and(|e: &(u64, u64, Vec<u8>)| e.0 == lo && e.1 == lo)
            {
                break;
            }
            let text = rng.bytes_n(0, 4, b"XY\n");
            edits.push((lo, hi, text));
            at = hi.max(lo + u64::from(hi == lo));
        }
        let view: Vec<(u64, u64, &[u8])> =
            edits.iter().map(|(l, h, t)| (*l, *h, t.as_slice())).collect();
        assert_eq!(splice(&pre, &view).unwrap(), naive(&pre, &edits), "{pre:?} {edits:?}");
    }
}

// The pre-image is a map in these tests (§7.4 `Files`).
#[test]
fn s7_4_files_is_a_map_in_tests() {
    let m = Mem::default();
    assert!(slip_apply::Files::read(&m, b"/none").is_err());
}
