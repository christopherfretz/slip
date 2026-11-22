//! `spec/model.md` §3: BLAKE3 against the official hash-mode test vectors,
//! incremental updates, and the tagged digest.

use slip_model::Digest;
use slip_model::hash::{Hasher, content_digest, tagged_digest};

/// `blake3_vectors.txt`: reduced from the official BLAKE3 test vectors
/// (see the comment at the top of that file for its origin).
const VECTORS: &str = include_str!("blake3_vectors.txt");

fn vectors() -> Vec<(usize, Digest)> {
    VECTORS
        .lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .map(|l| {
            let (len, hex) = l.split_once(' ').expect("fixture line: <len> <hex>");
            (
                len.parse().expect("fixture input length"),
                Digest::from_hex(hex).expect("fixture digest"),
            )
        })
        .collect()
}

/// The vector input of length `n`: bytes `i % 251`.
fn input(n: usize) -> Vec<u8> {
    (0..n).map(|i| (i % 251) as u8).collect()
}

// §3: the fixture holds the whole published set, which spans one block, one
// chunk and many chunks.
#[test]
fn model_3_fixture_is_complete() {
    let v = vectors();
    assert_eq!(v.len(), 35);
    assert_eq!(v.first().map(|c| c.0), Some(0));
    assert_eq!(v.last().map(|c| c.0), Some(102_400));
}

// §3: every official hash-mode vector, one-shot.
#[test]
fn model_3_official_vectors_one_shot() {
    for (len, want) in vectors() {
        assert_eq!(content_digest(&input(len)), want, "input length {len}");
    }
}

// §3: the empty input, as stated in the brief independently of the fixture.
#[test]
fn model_3_empty_input() {
    assert_eq!(
        content_digest(b"").to_string(),
        "af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262"
    );
    assert_eq!(Hasher::new().finalize(), content_digest(b""));
    assert_eq!(Hasher::default().finalize(), content_digest(b""));
}

// §3: incremental updates in pieces of 1, 63, 64, 65, 1023, 1024 and 1025
// bytes equal the one-shot digest, for every vector length.
#[test]
fn model_3_incremental_equals_one_shot() {
    for (len, want) in vectors() {
        let data = input(len);
        for piece in [1, 63, 64, 65, 1023, 1024, 1025] {
            let mut h = Hasher::new();
            for chunk in data.chunks(piece) {
                h.update(chunk);
            }
            assert_eq!(h.finalize(), want, "length {len} in pieces of {piece}");
        }
    }
}

// §3: uneven splits, including empty updates, and finalizing mid-stream
// leaves the hasher usable.
#[test]
fn model_3_irregular_splits_and_repeated_finalize() {
    let data = input(8193);
    let want = content_digest(&data);
    let mut h = Hasher::new();
    let mut rest = &data[..];
    let mut step = 0;
    while !rest.is_empty() {
        let n = (step * 37 % 1500).min(rest.len());
        h.update(&rest[..n]);
        h.update(&[]);
        rest = &rest[n..];
        step += 1;
    }
    assert_eq!(h.finalize(), want);
    assert_eq!(h.finalize(), want);

    let mut h = Hasher::new();
    h.update(&data[..1024]);
    assert_eq!(h.finalize(), content_digest(&data[..1024]));
    h.update(&data[1024..]);
    assert_eq!(h.finalize(), want);
}

// §3: the tagged digest is BLAKE3(tag ‖ 0x00 ‖ encoded).
#[test]
fn model_3_tagged_digest() {
    let encoded = input(300);
    let mut framed = b"slip.observer.v1".to_vec();
    framed.push(0);
    framed.extend_from_slice(&encoded);
    assert_eq!(tagged_digest("slip.observer.v1", &encoded), content_digest(&framed));
    // The separator keeps (tag, value) pairs apart.
    assert_ne!(tagged_digest("ab", b"c"), tagged_digest("a", b"bc"));
    assert_ne!(tagged_digest("slip.observer.v1", b""), content_digest(b"slip.observer.v1"));
}

// §3: a tag with a NUL byte would make the framing ambiguous.
#[test]
#[should_panic(expected = "not ASCII without NUL")]
fn model_3_tagged_digest_rejects_nul_in_tag() {
    let _ = tagged_digest("a\0b", b"");
}
