//! The container format: writer and reader (`spec/model.md` §5). Work package WP-001.
//!
//! ```text
//! file    := magic kind version section* end trailer
//! magic   := 53 4C 49 50 1A 0A 00 00
//! kind    := 4 upper-case ASCII letters
//! version := u32le, at least 1
//! section := id:uv length:uv payload
//! end     := 00
//! trailer := content digest of every byte before it
//! ```
//!
//! [`Reader::open`] checks, in this order (§5 rule 5): magic and kind
//! (`BadMagic`), version (`UnsupportedVersion`), length and trailer
//! (`Truncated`, `Corrupt`), then the section framing. A header field that
//! the input is too short to hold is not judged; the length check then
//! reports `Truncated`. Offsets are into the whole file; the conventions are
//! in `spec/model.md` §2.2 and §5.3.

use std::cell::Cell;

use crate::Digest;
use crate::hash::content_digest;
use crate::wire::{Decoder, Encoder, Error, ErrorKind};

/// The eight magic bytes: "SLIP", 0x1A, 0x0A, 0x00, 0x00.
pub const MAGIC: [u8; 8] = *b"SLIP\x1a\x0a\x00\x00";

/// Magic, kind and version.
pub const HEADER_LEN: usize = 16;

/// The digest that ends every file.
pub const TRAILER_LEN: usize = 32;

/// The shortest valid file: a header, the `end` byte and a trailer.
pub const MIN_LEN: usize = HEADER_LEN + 1 + TRAILER_LEN;

/// The first ignorable section id (§5 rule 2); ids below it are critical.
pub const FIRST_IGNORABLE: u64 = 64;

/// Builds a container in memory.
///
/// Calling [`Writer::section`] with a zero or non-ascending id, or
/// [`Writer::new`] with a kind that is not four upper-case ASCII letters or
/// a zero version, is a programming error and panics.
#[derive(Clone, Debug)]
pub struct Writer {
    out: Encoder,
    last_id: u64,
}

impl Writer {
    pub fn new(kind: [u8; 4], version: u32) -> Writer {
        assert!(
            kind.iter().all(u8::is_ascii_uppercase),
            "container::Writer: kind {kind:?} is not four upper-case ASCII letters"
        );
        assert!(version >= 1, "container::Writer: version must be at least 1");
        let mut out = Encoder::new();
        for b in MAGIC.iter().chain(kind.iter()).chain(&version.to_le_bytes()) {
            out.u8(*b);
        }
        Writer { out, last_id: 0 }
    }

    /// Appends a section. Ids must be at least 1 and strictly ascending.
    pub fn section(&mut self, id: u64, payload: &[u8]) -> &mut Self {
        assert!(id >= 1, "container::Writer: section id 0 is the end marker");
        assert!(
            id > self.last_id,
            "container::Writer: section id {id} after {} is not ascending",
            self.last_id
        );
        self.last_id = id;
        self.out.uv(id).bytes(payload);
        self
    }

    /// The finished file: the `end` byte and the trailer appended.
    pub fn finish(self) -> Vec<u8> {
        let mut out = self.out.into_bytes();
        out.push(0);
        let trailer = content_digest(&out);
        out.extend_from_slice(trailer.as_bytes());
        out
    }
}

#[derive(Clone, Copy, Debug)]
struct Section<'a> {
    id: u64,
    /// Offset of the section's id in the file.
    offset: usize,
    /// Offset of the payload's first byte in the file.
    payload_offset: usize,
    payload: &'a [u8],
}

/// A validated container. Sections are looked up by id; [`Reader::finish`]
/// then checks that every critical section present was asked for.
#[derive(Debug)]
pub struct Reader<'a> {
    version: u32,
    content_id: Digest,
    sections: Vec<Section<'a>>,
    /// Bit `id` set once critical section `id` (1..=63) has been requested.
    requested: Cell<u64>,
}

impl<'a> Reader<'a> {
    /// Opens `bytes` as a container of `kind` with a version in
    /// `1..=max_version`, performing every check of §5 rule 5 in order.
    pub fn open(bytes: &'a [u8], kind: [u8; 4], max_version: u32) -> Result<Reader<'a>, Error> {
        // Magic and kind, as far as the input reaches.
        let expected = MAGIC.iter().chain(kind.iter());
        for (i, (have, want)) in bytes.iter().zip(expected).enumerate() {
            if have != want {
                let what = if i < MAGIC.len() {
                    "not a Slip container (magic)"
                } else {
                    "container kind is not the expected one"
                };
                return Err(Error::new(ErrorKind::BadMagic, i, what));
            }
        }
        // Version, if the input holds it.
        let mut version = 0;
        if let Some(v) = bytes.get(12..HEADER_LEN) {
            version = u32::from_le_bytes([v[0], v[1], v[2], v[3]]);
            if version == 0 {
                return Err(Error::new(ErrorKind::UnsupportedVersion, 12, "container version 0"));
            }
            if version > max_version {
                return Err(Error::new(
                    ErrorKind::UnsupportedVersion,
                    12,
                    "container version newer than supported",
                ));
            }
        }
        // Length, then the trailer.
        if bytes.len() < MIN_LEN {
            return Err(Error::new(
                ErrorKind::Truncated,
                bytes.len(),
                "shorter than a header, end byte and trailer",
            ));
        }
        let body_len = bytes.len() - TRAILER_LEN;
        let body = &bytes[..body_len];
        let mut trailer = [0u8; TRAILER_LEN];
        trailer.copy_from_slice(&bytes[body_len..]);
        let content_id = Digest(trailer);
        if content_digest(body) != content_id {
            return Err(Error::new(
                ErrorKind::Corrupt,
                body_len,
                "integrity trailer does not match",
            ));
        }
        // Sections, up to the end byte, which must be the last before the
        // trailer.
        let mut dec = Decoder::with_base(&body[HEADER_LEN..], HEADER_LEN);
        let mut sections: Vec<Section<'a>> = Vec::new();
        loop {
            let offset = dec.offset();
            let id = dec.uv()?;
            if id == 0 {
                break;
            }
            if sections.last().is_some_and(|s| s.id >= id) {
                return Err(Error::new(
                    ErrorKind::Malformed,
                    offset,
                    "section ids not strictly ascending",
                ));
            }
            let payload = dec.bytes()?;
            let payload_offset = dec.offset() - payload.len();
            sections.push(Section { id, offset, payload_offset, payload });
        }
        dec.finish()?;
        Ok(Reader { version, content_id, sections, requested: Cell::new(0) })
    }

    /// The file's format version.
    pub fn version(&self) -> u32 {
        self.version
    }

    /// The content id: the trailer digest (§5).
    pub fn content_id(&self) -> Digest {
        self.content_id
    }

    fn find(&self, id: u64) -> Option<&Section<'a>> {
        if id < FIRST_IGNORABLE {
            self.requested.set(self.requested.get() | (1u64 << id));
        }
        self.sections.binary_search_by_key(&id, |s| s.id).ok().map(|i| &self.sections[i])
    }

    /// The payload of section `id`, if present. Marks the id as known to
    /// the caller for [`Reader::finish`].
    pub fn section(&self, id: u64) -> Option<&'a [u8]> {
        self.find(id).map(|s| s.payload)
    }

    /// Section `id` with a [`Decoder`] over its payload whose offsets are
    /// into the whole file. Marks the id as known, like [`Reader::section`].
    pub fn decoder(&self, id: u64) -> Option<Decoder<'a>> {
        self.find(id).map(|s| Decoder::with_base(s.payload, s.payload_offset))
    }

    /// The ids of every section present, ascending.
    pub fn ids(&self) -> impl Iterator<Item = u64> + '_ {
        self.sections.iter().map(|s| s.id)
    }

    /// `UnknownSection` at the first critical section present that was
    /// never requested (§5 rule 2). Unrequested ignorable sections are
    /// skipped.
    pub fn finish(self) -> Result<(), Error> {
        let requested = self.requested.get();
        for s in &self.sections {
            if s.id < FIRST_IGNORABLE && requested & (1u64 << s.id) == 0 {
                return Err(Error::new(
                    ErrorKind::UnknownSection,
                    s.offset,
                    "unknown critical section",
                ));
            }
        }
        Ok(())
    }
}
