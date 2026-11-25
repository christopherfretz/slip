//! The journal of a write (`spec/edits.md` §7.3 step 2): one record in the
//! encoding of `spec/model.md` §2, the one section (id 1) of a container of
//! kind `JRNL`, version 1, so that a journal cut short by a crash is told
//! from a whole one ([`Journal::read`]).
//!
//! ```text
//! Journal      := overlay: digest, files: list<JournalEntry>     in path order
//! JournalEntry := path: bytes, staged: bytes, pre: Content, post: Content
//! Content      := size: uv, content: digest
//! ```
//!
//! The reader refuses a journal that a write could not have made
//! (`spec/landed/edits/WP-063.md` §4): recovery removes and renames the
//! paths a journal names, so a journal that named any other path would
//! make recovery touch a file the write never created.

use slip_model::Digest;
use slip_model::container::{MAGIC, Reader, Writer};
use slip_model::overlay::Content;
use slip_model::transform::is_file_path;
use slip_model::wire::{Decoder, Encoder, ErrorKind};

/// The container kind of a journal (`spec/model.md` §5.1).
pub const KIND: [u8; 4] = *b"JRNL";

/// The container version this Slip writes and reads.
pub const VERSION: u32 = 1;

/// The section holding the record.
const RECORD: u64 = 1;

/// What the bytes of `write.journal` are (§7.3 step 2).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Read {
    /// A whole journal that passes the reader's rules.
    Whole(Journal),
    /// A journal cut short before its publish returned, so before any file
    /// of the tree was touched: removed by `recover`. The reason, for a
    /// message.
    Interrupted(String),
}

/// The journal (§7.3 step 2).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Journal {
    /// The overlay id.
    pub overlay: Digest,
    /// The files the write replaces, in path order; not empty.
    pub files: Vec<JournalEntry>,
}

/// One file of a write.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JournalEntry {
    /// Its real path.
    pub path: Vec<u8>,
    /// `staged_name(path, overlay)`.
    pub staged: Vec<u8>,
    pub pre: Content,
    /// Differs from `pre`.
    pub post: Content,
}

/// The staged name of §7.3 for a real path and an overlay id: in the
/// file's own directory, `.<file name>.slip-<the first 16 hexadecimal
/// digits of the overlay id>`.
pub fn staged_name(path: &[u8], overlay: Digest) -> Vec<u8> {
    let cut = path.iter().rposition(|&b| b == b'/').map_or(0, |i| i + 1);
    let mut out = path[..cut].to_vec();
    out.push(b'.');
    out.extend_from_slice(&path[cut..]);
    out.extend_from_slice(b".slip-");
    out.extend_from_slice(&overlay.to_string().as_bytes()[..16]);
    out
}

/// The directory of a real path: everything before its last `/`, or `/`.
pub(crate) fn parent(path: &[u8]) -> &[u8] {
    match path.iter().rposition(|&b| b == b'/') {
        Some(0) | None => b"/",
        Some(i) => &path[..i],
    }
}

impl Journal {
    /// The journal a write of `overlay` (id `overlay_id`) publishes: one
    /// entry per overlay file whose `post` differs from its `pre`, in the
    /// overlay's (path) order, with its staged name. `recover`'s caller
    /// compares a journal with it to know that the journal is that write's.
    pub fn of_write(overlay: &slip_model::overlay::Overlay, overlay_id: Digest) -> Journal {
        Journal {
            overlay: overlay_id,
            files: overlay
                .files
                .iter()
                .filter(|f| f.pre != f.post)
                .map(|f| JournalEntry {
                    path: f.path.clone(),
                    staged: staged_name(&f.path, overlay_id),
                    pre: f.pre,
                    post: f.post,
                })
                .collect(),
        }
    }

    /// The journal's bytes: the record in its container.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut w = Writer::new(KIND, VERSION);
        w.section(RECORD, &self.record());
        w.finish()
    }

    /// The record's bytes (the container's section; an earlier Slip's
    /// whole journal).
    pub fn record(&self) -> Vec<u8> {
        let mut e = Encoder::new();
        e.digest(&self.overlay).count(self.files.len());
        for f in &self.files {
            e.bytes(&f.path)
                .bytes(&f.staged)
                .uv(f.pre.size)
                .digest(&f.pre.content)
                .uv(f.post.size)
                .digest(&f.post.content);
        }
        e.into_bytes()
    }

    /// Reads the bytes of `write.journal` (§7.3 step 2). A container that
    /// fails `spec/model.md` §5 rule 5 for its length, trailer or framing,
    /// or bytes no longer than a prefix of its header, is `Interrupted`;
    /// one of a newer version is `Err` (kept for that Slip). Bytes that do
    /// not start with the magic are an earlier Slip's record: `Whole` if it
    /// decodes and passes the rules, else `Interrupted`. A whole container
    /// whose record does not decode or breaks a rule is `Err`: malformed,
    /// refused and kept.
    pub fn read(bytes: &[u8]) -> Result<Read, String> {
        let with_magic = bytes.iter().zip(MAGIC.iter()).all(|(a, b)| a == b);
        if !with_magic {
            return Ok(match decode(bytes).map_err(|e| e.to_string()).and_then(checked) {
                Ok(j) => Read::Whole(j),
                Err(e) => Read::Interrupted(format!("journal without a container: {e}")),
            });
        }
        let r = match Reader::open(bytes, KIND, VERSION) {
            Ok(r) => r,
            Err(e) => {
                return match e.kind {
                    ErrorKind::Truncated | ErrorKind::Corrupt | ErrorKind::Malformed => {
                        Ok(Read::Interrupted(format!("journal: {e}")))
                    }
                    _ => Err(format!("journal: {e}")),
                };
            }
        };
        let Some(record) = r.section(RECORD) else {
            return Err("journal: the record section is missing".to_string());
        };
        r.finish().map_err(|e| format!("journal: {e}"))?;
        let j = decode(record).map_err(|e| format!("journal: {e}"))?;
        checked(j).map(Read::Whole)
    }

    /// Reads a whole journal ([`Journal::read`]): an interrupted one is an
    /// error here too.
    pub fn from_bytes(bytes: &[u8]) -> Result<Journal, String> {
        match Journal::read(bytes)? {
            Read::Whole(j) => Ok(j),
            Read::Interrupted(why) => Err(why),
        }
    }
}

/// The reader's rules, each over the whole list in turn: not empty; every
/// path a real path of a file; strictly ascending by path; every staged
/// name the one of its path; `pre` differs from `post`.
fn checked(j: Journal) -> Result<Journal, String> {
    let at = |i: usize, what: &str| Err(format!("journal: files[{i}]: {what}"));
    if j.files.is_empty() {
        return Err("journal: files: a journal names no file".to_string());
    }
    if let Some(i) = j.files.iter().position(|f| !is_file_path(&f.path)) {
        return at(i, "not a canonical absolute path of a file");
    }
    if let Some(i) = (1..j.files.len()).find(|&i| j.files[i - 1].path >= j.files[i].path) {
        return at(i, "files are not strictly ascending by path");
    }
    if let Some(i) = j.files.iter().position(|f| f.staged != staged_name(&f.path, j.overlay)) {
        return at(i, "the staged name is not the one of its path and overlay");
    }
    if let Some(i) = j.files.iter().position(|f| f.pre == f.post) {
        return at(i, "pre and post are equal");
    }
    Ok(j)
}

fn decode(bytes: &[u8]) -> Result<Journal, slip_model::wire::Error> {
    let mut d = Decoder::new(bytes);
    let overlay = d.digest()?;
    let n = d.count()?;
    let mut files = Vec::with_capacity(n);
    for _ in 0..n {
        files.push(JournalEntry {
            path: d.bytes()?.to_vec(),
            staged: d.bytes()?.to_vec(),
            pre: Content { size: d.uv()?, content: d.digest()? },
            post: Content { size: d.uv()?, content: d.digest()? },
        });
    }
    d.finish()?;
    Ok(Journal { overlay, files })
}
