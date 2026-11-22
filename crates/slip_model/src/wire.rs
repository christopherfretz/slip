//! Primitive encodings: `Encoder` and `Decoder` (`spec/model.md` §2). Work package WP-001.
//!
//! Every value has exactly one valid encoding (§1 rule 1), so the decoder
//! rejects anything the encoder would not have produced. Errors carry the
//! byte offset at which they were detected (§2.1); the conventions for which
//! offset are in `spec/model.md` §2.2:
//!
//! - `Truncated`: the offset at which the input ended (its length).
//! - `Malformed` in a value: the offset of the offending byte (the redundant
//!   last byte of a non-minimal `uv`, the tenth byte of an overlong `uv`,
//!   the `bool` byte, the first invalid byte of a `str`); leftover bytes at
//!   [`Decoder::finish`]: the offset of the first leftover byte.
//! - `TooLarge`: the offset of the first byte of the length or count.

use std::fmt;

use crate::Digest;

/// The errors of `spec/model.md` §2.1.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ErrorKind {
    /// The input ends inside a value.
    Truncated,
    /// A rule of the spec is violated.
    Malformed,
    /// A length or count exceeds the bytes remaining.
    TooLarge,
    /// The container magic or kind is not the expected one.
    BadMagic,
    /// The container version is greater than the reader supports, or zero.
    UnsupportedVersion,
    /// A critical section id the reader does not know.
    UnknownSection,
    /// The integrity trailer does not match.
    Corrupt,
}

impl ErrorKind {
    /// The name used in the spec.
    pub fn name(self) -> &'static str {
        match self {
            ErrorKind::Truncated => "Truncated",
            ErrorKind::Malformed => "Malformed",
            ErrorKind::TooLarge => "TooLarge",
            ErrorKind::BadMagic => "BadMagic",
            ErrorKind::UnsupportedVersion => "UnsupportedVersion",
            ErrorKind::UnknownSection => "UnknownSection",
            ErrorKind::Corrupt => "Corrupt",
        }
    }
}

impl fmt::Display for ErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// A decoding error: its kind, the byte offset at which it was detected and
/// a short description of what was being decoded or which rule failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Error {
    pub kind: ErrorKind,
    pub offset: usize,
    pub what: &'static str,
}

impl Error {
    pub fn new(kind: ErrorKind, offset: usize, what: &'static str) -> Error {
        Error { kind, offset, what }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} at byte {}: {}", self.kind, self.offset, self.what)
    }
}

impl std::error::Error for Error {}

/// Appends §2 encodings to a byte vector.
#[derive(Clone, Debug, Default)]
pub struct Encoder {
    buf: Vec<u8>,
}

impl Encoder {
    pub fn new() -> Encoder {
        Encoder { buf: Vec::new() }
    }

    /// `u8`: one byte.
    pub fn u8(&mut self, v: u8) -> &mut Self {
        self.buf.push(v);
        self
    }

    /// `bool`: 0 or 1.
    pub fn bool(&mut self, v: bool) -> &mut Self {
        self.buf.push(u8::from(v));
        self
    }

    /// `uv`: minimal unsigned LEB128.
    pub fn uv(&mut self, mut v: u64) -> &mut Self {
        loop {
            let low = (v & 0x7f) as u8;
            v >>= 7;
            if v == 0 {
                self.buf.push(low);
                return self;
            }
            self.buf.push(low | 0x80);
        }
    }

    /// `sv`: zigzag, then `uv`.
    pub fn sv(&mut self, v: i64) -> &mut Self {
        self.uv(((v << 1) ^ (v >> 63)) as u64)
    }

    /// `bytes`: `uv` length, then the bytes.
    pub fn bytes(&mut self, v: &[u8]) -> &mut Self {
        self.count(v.len());
        self.buf.extend_from_slice(v);
        self
    }

    /// `str`: `bytes` holding UTF-8.
    pub fn str(&mut self, v: &str) -> &mut Self {
        self.bytes(v.as_bytes())
    }

    /// `digest`: 32 bytes.
    pub fn digest(&mut self, v: &Digest) -> &mut Self {
        self.buf.extend_from_slice(v.as_bytes());
        self
    }

    /// The count of a `list<T>`, as `uv`.
    pub fn count(&mut self, n: usize) -> &mut Self {
        self.uv(n as u64)
    }

    /// The bytes written so far.
    pub fn as_bytes(&self) -> &[u8] {
        &self.buf
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.buf
    }
}

/// Reads §2 encodings from a byte slice, validating everything.
#[derive(Clone, Debug)]
pub struct Decoder<'a> {
    data: &'a [u8],
    pos: usize,
    base: usize,
}

impl<'a> Decoder<'a> {
    /// A decoder whose error offsets are relative to the start of `data`.
    pub fn new(data: &'a [u8]) -> Decoder<'a> {
        Decoder::with_base(data, 0)
    }

    /// A decoder whose error offsets (and [`Decoder::offset`]) are `base`
    /// plus the position in `data`: for decoding a part of a larger file,
    /// such as a container section, with offsets into the whole file.
    pub fn with_base(data: &'a [u8], base: usize) -> Decoder<'a> {
        Decoder { data, pos: 0, base }
    }

    /// The offset of the next byte to be read.
    pub fn offset(&self) -> usize {
        self.base + self.pos
    }

    /// The number of bytes not yet read.
    pub fn remaining(&self) -> usize {
        self.data.len() - self.pos
    }

    fn err(&self, kind: ErrorKind, pos: usize, what: &'static str) -> Error {
        Error::new(kind, self.base + pos, what)
    }

    fn truncated(&self, what: &'static str) -> Error {
        self.err(ErrorKind::Truncated, self.data.len(), what)
    }

    fn take(&mut self, n: usize, what: &'static str) -> Result<&'a [u8], Error> {
        if self.remaining() < n {
            return Err(self.truncated(what));
        }
        let out = &self.data[self.pos..self.pos + n];
        self.pos += n;
        Ok(out)
    }

    /// `u8`.
    pub fn u8(&mut self) -> Result<u8, Error> {
        Ok(self.take(1, "u8")?[0])
    }

    /// `bool`: `Malformed` unless the byte is 0 or 1.
    pub fn bool(&mut self) -> Result<bool, Error> {
        let at = self.pos;
        match self.take(1, "bool")?[0] {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(self.err(ErrorKind::Malformed, at, "bool is not 0 or 1")),
        }
    }

    /// `uv`: at most ten bytes, the tenth 0x00 or 0x01, minimal.
    pub fn uv(&mut self) -> Result<u64, Error> {
        let mut value = 0u64;
        for i in 0..10 {
            let at = self.pos;
            let b = self.take(1, "uv")?[0];
            if i == 9 && b > 1 {
                return Err(self.err(ErrorKind::Malformed, at, "uv exceeds 64 bits"));
            }
            value |= u64::from(b & 0x7f) << (7 * i);
            if b & 0x80 == 0 {
                if b == 0 && i > 0 {
                    return Err(self.err(ErrorKind::Malformed, at, "uv is not minimal"));
                }
                return Ok(value);
            }
        }
        unreachable!("uv: the tenth byte is at most 1, so the loop returns")
    }

    /// `sv`: `uv`, then zigzag.
    pub fn sv(&mut self) -> Result<i64, Error> {
        let u = self.uv()?;
        Ok(((u >> 1) as i64) ^ -((u & 1) as i64))
    }

    /// A `uv` length or count, `TooLarge` if it exceeds the bytes remaining
    /// after it.
    fn length(&mut self, what: &'static str) -> Result<usize, Error> {
        let at = self.pos;
        let n = self.uv()?;
        match usize::try_from(n) {
            Ok(n) if n <= self.remaining() => Ok(n),
            _ => Err(self.err(ErrorKind::TooLarge, at, what)),
        }
    }

    /// `bytes`.
    pub fn bytes(&mut self) -> Result<&'a [u8], Error> {
        let n = self.length("bytes length exceeds the input")?;
        self.take(n, "bytes")
    }

    /// `str`: `Malformed` at the first invalid byte unless UTF-8.
    pub fn str(&mut self) -> Result<&'a str, Error> {
        let n = self.length("str length exceeds the input")?;
        let start = self.pos;
        let raw = self.take(n, "str")?;
        std::str::from_utf8(raw).map_err(|e| {
            self.err(ErrorKind::Malformed, start + e.valid_up_to(), "str is not UTF-8")
        })
    }

    /// `digest`: 32 bytes.
    pub fn digest(&mut self) -> Result<Digest, Error> {
        let raw = self.take(32, "digest")?;
        let mut out = [0u8; 32];
        out.copy_from_slice(raw);
        Ok(Digest(out))
    }

    /// The count of a `list<T>`. Every element takes at least one byte, so
    /// a count above the bytes remaining is rejected (`TooLarge`) before
    /// anything is allocated for it (§1 rule 3).
    pub fn count(&mut self) -> Result<usize, Error> {
        self.length("list count exceeds the input")
    }

    /// Ends decoding: `Malformed` if any byte remains (§5 rule 3).
    pub fn finish(self) -> Result<(), Error> {
        if self.remaining() == 0 {
            Ok(())
        } else {
            Err(self.err(ErrorKind::Malformed, self.pos, "bytes after the end"))
        }
    }
}
