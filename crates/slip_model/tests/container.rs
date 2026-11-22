//! `spec/model.md` §5 and §5.2: the container, one test per rule.

use slip_model::container::{HEADER_LEN, MAGIC, MIN_LEN, Reader, TRAILER_LEN, Writer};
use slip_model::hash::content_digest;
use slip_model::wire::{Encoder, ErrorKind};

const KIND: [u8; 4] = *b"TEST";

fn header(kind: [u8; 4], version: u32) -> Vec<u8> {
    let mut out = MAGIC.to_vec();
    out.extend_from_slice(&kind);
    out.extend_from_slice(&version.to_le_bytes());
    out
}

/// `body` (everything before the trailer) with a correct trailer appended:
/// for files whose framing is wrong but whose integrity is intact.
fn sealed(mut body: Vec<u8>) -> Vec<u8> {
    let d = content_digest(&body);
    body.extend_from_slice(d.as_bytes());
    body
}

/// A header followed by raw `(id, payload)` sections in the order given,
/// then `end`, sealed.
fn raw(sections: &[(u64, &[u8])]) -> Vec<u8> {
    let mut e = Encoder::new();
    for &(id, p) in sections {
        e.uv(id).bytes(p);
    }
    e.u8(0);
    let mut body = header(KIND, 1);
    body.extend_from_slice(&e.into_bytes());
    sealed(body)
}

fn small() -> Vec<u8> {
    let mut w = Writer::new(KIND, 2);
    w.section(1, b"one").section(3, b"").section(70, b"ignorable");
    w.finish()
}

fn open_err(bytes: &[u8]) -> (ErrorKind, usize) {
    let e = Reader::open(bytes, KIND, 2).unwrap_err();
    (e.kind, e.offset)
}

// §5: the exact bytes of a file, and reading them back (§5.2).
#[test]
fn model_5_layout_and_round_trip() {
    let f = small();
    let mut want = header(KIND, 2);
    want.extend_from_slice(&[1, 3, b'o', b'n', b'e', 3, 0, 70, 9]);
    want.extend_from_slice(b"ignorable");
    want.push(0);
    let trailer = content_digest(&want);
    want.extend_from_slice(trailer.as_bytes());
    assert_eq!(f, want);
    assert_eq!(&f[..8], b"SLIP\x1a\x0a\x00\x00");

    let r = Reader::open(&f, KIND, 2).unwrap();
    assert_eq!(r.version(), 2);
    // §5: the content id is the trailer digest.
    assert_eq!(r.content_id(), trailer);
    assert_eq!(r.ids().collect::<Vec<_>>(), [1, 3, 70]);
    assert_eq!(r.section(1), Some(&b"one"[..]));
    assert_eq!(r.section(2), None);
    assert_eq!(r.section(3), Some(&b""[..]));
    assert_eq!(r.section(70), Some(&b"ignorable"[..]));
    r.finish().unwrap();
}

// §5: a file with no sections is a header, `end` and the trailer.
#[test]
fn model_5_empty_container() {
    let f = Writer::new(KIND, 1).finish();
    assert_eq!(f.len(), MIN_LEN);
    let r = Reader::open(&f, KIND, 1).unwrap();
    assert_eq!(r.ids().count(), 0);
    r.finish().unwrap();
}

// §5 rule 1: sections out of order are `Malformed`, at the later id.
#[test]
fn model_5_rule_1_order() {
    let f = raw(&[(2, b"a"), (1, b"b")]);
    assert_eq!(open_err(&f), (ErrorKind::Malformed, HEADER_LEN + 3));
}

// §5 rule 1: a duplicate id is `Malformed`, at the second occurrence.
#[test]
fn model_5_rule_1_duplicate_id() {
    let f = raw(&[(5, b"a"), (5, b"a")]);
    assert_eq!(open_err(&f), (ErrorKind::Malformed, HEADER_LEN + 3));
}

// §5 rule 1: the writer refuses a non-ascending or repeated id.
#[test]
#[should_panic(expected = "not ascending")]
fn model_5_rule_1_writer_rejects_non_ascending() {
    let mut w = Writer::new(KIND, 1);
    w.section(4, b"").section(3, b"");
}

#[test]
#[should_panic(expected = "not ascending")]
fn model_5_rule_1_writer_rejects_duplicate() {
    let mut w = Writer::new(KIND, 1);
    w.section(4, b"").section(4, b"");
}

// §5 grammar: id 0 is `end`, so the writer refuses it.
#[test]
#[should_panic(expected = "end marker")]
fn model_5_writer_rejects_zero_id() {
    Writer::new(KIND, 1).section(0, b"");
}

// §5 rule 2: a critical section (1–63) the reader never asked for is
// `UnknownSection` at `finish`, at the offset of its id.
#[test]
fn model_5_rule_2_critical_unknown() {
    let f = raw(&[(1, b"x"), (63, b"y")]);
    let r = Reader::open(&f, KIND, 1).unwrap();
    assert!(r.section(1).is_some());
    let e = r.finish().unwrap_err();
    assert_eq!((e.kind, e.offset), (ErrorKind::UnknownSection, HEADER_LEN + 3));

    // Asking for it, even without using the payload, makes it known.
    let r = Reader::open(&f, KIND, 1).unwrap();
    r.section(1);
    r.section(63);
    r.finish().unwrap();
}

// §5 rule 2: an ignorable section (64 and above) not asked for is skipped.
#[test]
fn model_5_rule_2_ignorable_unknown_skipped() {
    let f = raw(&[(2, b"x"), (64, b"y"), (u64::MAX, b"z")]);
    let r = Reader::open(&f, KIND, 1).unwrap();
    assert_eq!(r.ids().collect::<Vec<_>>(), [2, 64, u64::MAX]);
    r.section(2);
    r.finish().unwrap();
}

// §5 rule 3: a section decoder that leaves bytes is `Malformed`; the
// section decoder reports offsets into the whole file.
#[test]
fn model_5_rule_3_payload_consumed_exactly() {
    let f = raw(&[(1, &[5, 6])]);
    let r = Reader::open(&f, KIND, 1).unwrap();
    let mut d = r.decoder(1).unwrap();
    assert_eq!(d.offset(), HEADER_LEN + 2);
    assert_eq!(d.u8(), Ok(5));
    let e = d.finish().unwrap_err();
    assert_eq!((e.kind, e.offset), (ErrorKind::Malformed, HEADER_LEN + 3));
    assert!(r.decoder(2).is_none());
    r.finish().unwrap();
}

// §5 rule 4: bytes after the trailer. Appended to a valid file they make the
// last 32 bytes the wrong trailer (`Corrupt`); resealed, they sit between
// `end` and the trailer (`Malformed`).
#[test]
fn model_5_rule_4_bytes_after_trailer() {
    let mut f = small();
    f.push(0);
    assert_eq!(open_err(&f), (ErrorKind::Corrupt, f.len() - TRAILER_LEN));

    let f = small();
    let mut body = f[..f.len() - TRAILER_LEN].to_vec();
    let end = body.len() - 1;
    body.extend_from_slice(&[0, 0]);
    let f = sealed(body);
    assert_eq!(open_err(&f), (ErrorKind::Malformed, end + 1));
}

// §5 rule 5: magic (`BadMagic` at the first wrong byte).
#[test]
fn model_5_rule_5_bad_magic() {
    let mut f = small();
    f[4] = 0x0a;
    assert_eq!(open_err(&f), (ErrorKind::BadMagic, 4));
    assert_eq!(open_err(b"PK\x03\x04"), (ErrorKind::BadMagic, 0));
}

// §5 rule 5: kind (`BadMagic` at the first wrong byte of the kind).
#[test]
fn model_5_rule_5_wrong_kind() {
    let f = small();
    let e = Reader::open(&f, *b"TESS", 2).unwrap_err();
    assert_eq!((e.kind, e.offset), (ErrorKind::BadMagic, 11));
}

// §5 rule 5: version newer than supported, or zero (`UnsupportedVersion`).
#[test]
fn model_5_rule_5_version() {
    let f = small();
    let e = Reader::open(&f, KIND, 1).unwrap_err();
    assert_eq!((e.kind, e.offset), (ErrorKind::UnsupportedVersion, 12));
    assert!(Reader::open(&f, KIND, 2).is_ok());
    assert!(Reader::open(&f, KIND, u32::MAX).is_ok());

    let mut body = header(KIND, 0);
    body.push(0);
    let f = sealed(body);
    let e = Reader::open(&f, KIND, u32::MAX).unwrap_err();
    assert_eq!((e.kind, e.offset), (ErrorKind::UnsupportedVersion, 12));
}

// §5 rule 5: the trailer (`Corrupt`), checked before any section framing.
#[test]
fn model_5_rule_5_trailer() {
    let mut f = small();
    let last = f.len() - 1;
    f[last] ^= 1;
    assert_eq!(open_err(&f), (ErrorKind::Corrupt, f.len() - TRAILER_LEN));
    // A broken section framing under a bad trailer reports the trailer.
    let mut f = raw(&[(2, b"a"), (1, b"b")]);
    f[HEADER_LEN + 3] = 0xff;
    assert_eq!(open_err(&f).0, ErrorKind::Corrupt);
}

// §5 rule 5: the order of the checks. Each file fails several checks and
// must report the earliest.
#[test]
fn model_5_rule_5_check_order() {
    // Bad magic and a bad version and a bad trailer: BadMagic.
    let mut f = small();
    f[0] = b'X';
    f[12] = 9;
    assert_eq!(open_err(&f).0, ErrorKind::BadMagic);
    // Wrong kind and a bad version: BadMagic.
    let mut f = small();
    f[8] = b'X';
    f[12] = 9;
    assert_eq!(open_err(&f).0, ErrorKind::BadMagic);
    // Bad version and a bad trailer: UnsupportedVersion.
    let mut f = small();
    f[12] = 9;
    assert_eq!(open_err(&f).0, ErrorKind::UnsupportedVersion);
    // Bad version and too short: UnsupportedVersion.
    let f = header(KIND, 9);
    assert_eq!(open_err(&f).0, ErrorKind::UnsupportedVersion);
    // Bad magic and too short: BadMagic.
    assert_eq!(open_err(b"SLIQ").0, ErrorKind::BadMagic);
    // Bad trailer and bad framing: Corrupt (tested above); good trailer and
    // bad framing: the framing error.
    let f = raw(&[(2, b"a"), (1, b"b")]);
    assert_eq!(open_err(&f).0, ErrorKind::Malformed);
}

// §5 rule 5: shorter than a header, `end` and trailer is `Truncated`; every
// prefix of a valid file is rejected, never accepted.
#[test]
fn model_5_rule_5_truncated_at_every_length() {
    let f = small();
    for n in 0..f.len() {
        let got = open_err(&f[..n]);
        if n < MIN_LEN {
            assert_eq!(got, (ErrorKind::Truncated, n), "prefix of {n} bytes");
        } else {
            assert_eq!(got, (ErrorKind::Corrupt, n - TRAILER_LEN), "prefix of {n} bytes");
        }
    }
}

// §5 grammar: a sealed file whose sections run into the trailer without an
// `end` byte, or whose length overruns it.
#[test]
fn model_5_sealed_framing_errors() {
    // No `end` byte: the input ends where the next id would be.
    let mut e = Encoder::new();
    e.uv(1).bytes(b"abc");
    let mut body = header(KIND, 1);
    body.extend_from_slice(e.as_bytes());
    let body_len = body.len();
    let f = sealed(body);
    assert!(f.len() >= MIN_LEN);
    assert_eq!(open_err(&f), (ErrorKind::Truncated, body_len));

    // A length larger than what is left before the trailer.
    let mut body = header(KIND, 1);
    body.extend_from_slice(&[1, 50, b'a', 0]);
    let f = sealed(body);
    assert_eq!(open_err(&f), (ErrorKind::TooLarge, HEADER_LEN + 1));

    // A non-minimal id.
    let mut body = header(KIND, 1);
    body.extend_from_slice(&[0x81, 0x00, 0, 0]);
    let f = sealed(body);
    assert_eq!(open_err(&f), (ErrorKind::Malformed, HEADER_LEN + 1));
}

// §5 rule 5 / §1 rule 3: flipping any single bit of a valid file never
// yields a file that opens: the error is `Corrupt` or an earlier check.
#[test]
fn model_5_flipped_bit_never_succeeds() {
    let f = small();
    for byte in 0..f.len() {
        for bit in 0..8 {
            let mut g = f.clone();
            g[byte] ^= 1 << bit;
            let e = Reader::open(&g, KIND, 2).unwrap_err();
            assert!(
                matches!(
                    e.kind,
                    ErrorKind::Corrupt | ErrorKind::BadMagic | ErrorKind::UnsupportedVersion
                ),
                "byte {byte} bit {bit}: {e}"
            );
            if byte >= HEADER_LEN {
                assert_eq!(e.kind, ErrorKind::Corrupt, "byte {byte} bit {bit}");
            }
        }
    }
}
