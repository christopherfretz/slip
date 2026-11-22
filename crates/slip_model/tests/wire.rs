//! `spec/model.md` §2 and §2.1: primitive encodings and their rejections.

use slip_model::Digest;
use slip_model::wire::{Decoder, Encoder, Error, ErrorKind};

fn enc(f: impl FnOnce(&mut Encoder)) -> Vec<u8> {
    let mut e = Encoder::new();
    f(&mut e);
    e.into_bytes()
}

fn err(kind: ErrorKind, offset: usize) -> (ErrorKind, usize) {
    (kind, offset)
}

fn of(e: Error) -> (ErrorKind, usize) {
    (e.kind, e.offset)
}

// §2 `u8`, `bool`, `digest`, `bytes`, `str`, `uv`, `sv`, list count: one
// value of each round-trips, and `finish` accepts the exact end.
#[test]
fn model_2_every_primitive_round_trips() {
    let d = Digest([7; 32]);
    let bytes = enc(|e| {
        e.u8(0xfe)
            .bool(true)
            .bool(false)
            .uv(300)
            .sv(-5)
            .bytes(b"\x00\xffab")
            .str("h\u{e9}llo")
            .digest(&d)
            .count(3);
    });
    let mut dec = Decoder::new(&bytes);
    assert_eq!(dec.u8(), Ok(0xfe));
    assert_eq!(dec.bool(), Ok(true));
    assert_eq!(dec.bool(), Ok(false));
    assert_eq!(dec.uv(), Ok(300));
    assert_eq!(dec.sv(), Ok(-5));
    assert_eq!(dec.bytes(), Ok(&b"\x00\xffab"[..]));
    assert_eq!(dec.str(), Ok("h\u{e9}llo"));
    assert_eq!(dec.digest(), Ok(d));
    // A count is checked against the bytes remaining; none remain here.
    assert_eq!(of(dec.clone().count().unwrap_err()).0, ErrorKind::TooLarge);
    assert_eq!(dec.uv(), Ok(3));
    assert_eq!(dec.remaining(), 0);
    assert_eq!(dec.offset(), bytes.len());
    assert_eq!(dec.finish(), Ok(()));
}

// §2 `bytes`/`str`: the empty value is a single zero length byte.
#[test]
fn model_2_empty_bytes_and_str() {
    let bytes = enc(|e| {
        e.bytes(b"").str("");
    });
    assert_eq!(bytes, [0, 0]);
    let mut dec = Decoder::new(&bytes);
    assert_eq!(dec.bytes(), Ok(&b""[..]));
    assert_eq!(dec.str(), Ok(""));
    dec.finish().unwrap();
}

// §2 `uv`: exact encodings at the boundaries.
#[test]
fn model_2_uv_boundaries() {
    let cases: &[(u64, &[u8])] = &[
        (0, &[0x00]),
        (1, &[0x01]),
        (127, &[0x7f]),
        (128, &[0x80, 0x01]),
        (16383, &[0xff, 0x7f]),
        (16384, &[0x80, 0x80, 0x01]),
        (1 << 63, &[0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x01]),
        (u64::MAX, &[0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x01]),
    ];
    for &(v, want) in cases {
        let got = enc(|e| {
            e.uv(v);
        });
        assert_eq!(got, want, "encoding of {v}");
        let mut dec = Decoder::new(&got);
        assert_eq!(dec.uv(), Ok(v));
        dec.finish().unwrap();
    }
}

// §2 `uv`: every length from one to ten bytes round-trips.
#[test]
fn model_2_uv_every_length() {
    for bits in 0..64 {
        for v in [1u64 << bits, (1u64 << bits) - 1, (1u64 << bits) + 1] {
            let got = enc(|e| {
                e.uv(v);
            });
            let groups = (64 - v.leading_zeros()).div_ceil(7).max(1) as usize;
            assert_eq!(got.len(), groups, "length for {v}");
            let mut dec = Decoder::new(&got);
            assert_eq!(dec.uv(), Ok(v));
            dec.finish().unwrap();
        }
    }
}

// §2 `sv`: zigzag mapping and the extremes.
#[test]
fn model_2_sv_boundaries() {
    let cases: &[(i64, u64)] = &[
        (0, 0),
        (-1, 1),
        (1, 2),
        (-2, 3),
        (63, 126),
        (-64, 127),
        (64, 128),
        (i64::MAX, u64::MAX - 1),
        (i64::MIN, u64::MAX),
    ];
    for &(v, zz) in cases {
        let got = enc(|e| {
            e.sv(v);
        });
        let want = enc(|e| {
            e.uv(zz);
        });
        assert_eq!(got, want, "zigzag of {v}");
        let mut dec = Decoder::new(&got);
        assert_eq!(dec.sv(), Ok(v));
        dec.finish().unwrap();
    }
}

// §2 `uv` / §2.1 `Malformed`: not minimal (a redundant zero last byte), at
// the offset of that byte.
#[test]
fn model_2_1_uv_non_minimal() {
    for (input, at) in [
        (&[0x80, 0x00][..], 1),
        (&[0x81, 0x80, 0x00], 2),
        (&[0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x00], 9),
    ] {
        assert_eq!(
            of(Decoder::new(input).uv().unwrap_err()),
            err(ErrorKind::Malformed, at),
            "{input:x?}"
        );
    }
    // The single byte 0x00 is the minimal encoding of zero.
    assert_eq!(Decoder::new(&[0x00]).uv(), Ok(0));
}

// §2 `uv` / §2.1 `Malformed`: an 11-byte `uv` (the tenth byte has its
// continuation bit set), at the offset of the tenth byte.
#[test]
fn model_2_1_uv_eleven_bytes() {
    let mut input = vec![0x80; 10];
    input.push(0x01);
    assert_eq!(of(Decoder::new(&input).uv().unwrap_err()), err(ErrorKind::Malformed, 9));
    // Also with the continuation set and no eleventh byte at all.
    assert_eq!(of(Decoder::new(&[0x80; 10]).uv().unwrap_err()), err(ErrorKind::Malformed, 9));
}

// §2 `uv` / §2.1 `Malformed`: a tenth byte above 1 (value beyond 2^64).
#[test]
fn model_2_1_uv_tenth_byte_above_one() {
    for tenth in [0x02, 0x7f] {
        let mut input = vec![0xff; 9];
        input.push(tenth);
        assert_eq!(
            of(Decoder::new(&input).uv().unwrap_err()),
            err(ErrorKind::Malformed, 9),
            "tenth byte {tenth:#x}"
        );
    }
    // Offsets are relative to where decoding started.
    let mut input = vec![0x05, 0x06];
    input.extend([0xff; 9]);
    input.push(0x02);
    let mut dec = Decoder::new(&input);
    dec.u8().unwrap();
    dec.u8().unwrap();
    assert_eq!(of(dec.uv().unwrap_err()), err(ErrorKind::Malformed, 11));
}

// §2 `bool` / §2.1 `Malformed`: any byte but 0 or 1.
#[test]
fn model_2_1_bad_bool() {
    for b in [2u8, 0x80, 0xff] {
        let input = [1, b];
        let mut dec = Decoder::new(&input);
        assert_eq!(dec.bool(), Ok(true));
        assert_eq!(of(dec.bool().unwrap_err()), err(ErrorKind::Malformed, 1));
    }
}

// §2 `str` / §2.1 `Malformed`: invalid UTF-8, at the first invalid byte.
#[test]
fn model_2_1_invalid_utf8() {
    // length 4: "ab", a lone continuation byte, "c".
    let input = [4, b'a', b'b', 0x80, b'c'];
    assert_eq!(of(Decoder::new(&input).str().unwrap_err()), err(ErrorKind::Malformed, 3));
    // A multi-byte sequence cut by the length.
    let input = [1, 0xc3, 0xa9];
    assert_eq!(of(Decoder::new(&input).str().unwrap_err()), err(ErrorKind::Malformed, 1));
    // An encoded surrogate.
    let input = [3, 0xed, 0xa0, 0x80];
    assert_eq!(of(Decoder::new(&input).str().unwrap_err()), err(ErrorKind::Malformed, 1));
    // The same bytes are fine as `bytes`.
    assert_eq!(Decoder::new(&input).bytes(), Ok(&input[1..]));
}

// §2.1 `Truncated`: the input ends inside each kind of value; the offset is
// where the input ended.
#[test]
fn model_2_1_truncated_values() {
    assert_eq!(of(Decoder::new(&[]).u8().unwrap_err()), err(ErrorKind::Truncated, 0));
    assert_eq!(of(Decoder::new(&[]).bool().unwrap_err()), err(ErrorKind::Truncated, 0));
    assert_eq!(of(Decoder::new(&[]).uv().unwrap_err()), err(ErrorKind::Truncated, 0));
    assert_eq!(of(Decoder::new(&[0x80, 0x80]).uv().unwrap_err()), err(ErrorKind::Truncated, 2));
    assert_eq!(of(Decoder::new(&[0x80]).sv().unwrap_err()), err(ErrorKind::Truncated, 1));
    assert_eq!(of(Decoder::new(&[0u8; 31]).digest().unwrap_err()), err(ErrorKind::Truncated, 31));
    // A length that is itself cut off.
    assert_eq!(of(Decoder::new(&[0x85]).bytes().unwrap_err()), err(ErrorKind::Truncated, 1));
    assert_eq!(of(Decoder::new(&[0x85]).str().unwrap_err()), err(ErrorKind::Truncated, 1));
    assert_eq!(of(Decoder::new(&[0x85]).count().unwrap_err()), err(ErrorKind::Truncated, 1));
}

// §2.1 `TooLarge`: a length or count larger than the bytes remaining, at
// the offset of the length.
#[test]
fn model_2_1_length_or_count_too_large() {
    // `bytes`/`str` of length 3 with two bytes after it.
    let input = [9, 3, b'a', b'b'];
    let mut dec = Decoder::new(&input);
    dec.u8().unwrap();
    assert_eq!(of(dec.clone().bytes().unwrap_err()), err(ErrorKind::TooLarge, 1));
    assert_eq!(of(dec.str().unwrap_err()), err(ErrorKind::TooLarge, 1));
    // A list count of 3 with two bytes after it; 2 is accepted.
    let input = [3, 0, 0];
    assert_eq!(of(Decoder::new(&input).count().unwrap_err()), err(ErrorKind::TooLarge, 0));
    let input = [2, 0, 0];
    assert_eq!(Decoder::new(&input).count(), Ok(2));
    // The largest possible count is rejected without allocating.
    let input = [0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x01];
    assert_eq!(of(Decoder::new(&input).count().unwrap_err()), err(ErrorKind::TooLarge, 0));
    assert_eq!(of(Decoder::new(&input).bytes().unwrap_err()), err(ErrorKind::TooLarge, 0));
}

// §2.1 `Malformed` / §5 rule 3: bytes left over at `finish`, at the first
// leftover byte.
#[test]
fn model_2_1_leftover_bytes_at_finish() {
    let input = [1, 2, 3];
    let mut dec = Decoder::new(&input);
    dec.u8().unwrap();
    assert_eq!(of(dec.finish().unwrap_err()), err(ErrorKind::Malformed, 1));
    assert_eq!(Decoder::new(&[]).finish(), Ok(()));
}

// §2.1: `with_base` shifts every reported offset, so a decoder over part of
// a file reports offsets into the whole file.
#[test]
fn model_2_1_offsets_with_base() {
    let input = [0x80, 0x00];
    let mut dec = Decoder::with_base(&input, 100);
    assert_eq!(dec.offset(), 100);
    assert_eq!(of(dec.uv().unwrap_err()), err(ErrorKind::Malformed, 101));
    let e = Decoder::with_base(&[], 7).u8().unwrap_err();
    assert_eq!(of(e), err(ErrorKind::Truncated, 7));
    assert_eq!(e.to_string(), "Truncated at byte 7: u8");
}
