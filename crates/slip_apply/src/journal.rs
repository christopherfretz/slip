//! The journal of a write (`spec/edits.md` §7.3 step 2): one record in the
//! encoding of `spec/model.md` §2, in a container of kind `JRNL`, version
//! 1, whose first section declares the file's total length under a check
//! of its own, so that a journal cut short by a crash is told from a whole
//! one and from a damaged one ([`Journal::read`]).
//!
//! ```text
//! file     := head record-section end trailer        a container (model.md §5)
//! head     := magic "JRNL" 01 00 00 00                16 bytes
//!             01 10 total:u64le check:8 bytes       section 1, 18 bytes
//! check    := the first 8 bytes of content_digest(file[0..26])
//! record-section := 02 length:uv record              section 2
//! total    := the length of the whole file
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
use slip_model::wire::{Decoder, Encoder};

/// The container kind of a journal (`spec/model.md` §5.1).
pub const KIND: [u8; 4] = *b"JRNL";

/// The container version this Slip writes and reads.
pub const VERSION: u32 = 1;

/// The section declaring the file's length and the head's check.
const LENGTH: u64 = 1;

/// The section holding the record.
const RECORD: u64 = 2;

/// The bytes the head's check covers: the container's header (16), the
/// framing of section 1 (2) and the total length (8).
const CHECKED: usize = 26;

/// The head: the container's header and section 1.
pub const HEAD_LEN: usize = CHECKED + 8;

/// The check of a head whose first [`CHECKED`] bytes are `head`.
fn head_check(head: &[u8]) -> [u8; 8] {
    let d = slip_model::hash::content_digest(&head[..CHECKED]);
    let mut c = [0u8; 8];
    c.copy_from_slice(&d.0[..8]);
    c
}

/// A journal file of `version` holding `sections` after section 1 (ids
/// above 1, ascending), its head made for its length: what
/// [`Journal::to_bytes`] writes, and, for tests, any other container of
/// this form.
pub fn frame(version: u32, sections: &[(u64, &[u8])]) -> Vec<u8> {
    let build = |payload: &[u8]| {
        let mut w = Writer::new(KIND, version);
        w.section(LENGTH, payload);
        for (id, bytes) in sections {
            w.section(*id, bytes);
        }
        w.finish()
    };
    let total = build(&[0; 16]).len() as u64;
    let mut head = Vec::with_capacity(CHECKED);
    head.extend_from_slice(&MAGIC);
    head.extend_from_slice(&KIND);
    head.extend_from_slice(&version.to_le_bytes());
    head.extend_from_slice(&[1, 16]);
    head.extend_from_slice(&total.to_le_bytes());
    let mut payload = total.to_le_bytes().to_vec();
    payload.extend_from_slice(&head_check(&head));
    let bytes = build(&payload);
    debug_assert_eq!(bytes.len() as u64, total);
    debug_assert_eq!(&bytes[..CHECKED], &head[..]);
    bytes
}

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

    /// The journal's bytes: the record in its container, under its head.
    pub fn to_bytes(&self) -> Vec<u8> {
        frame(VERSION, &[(RECORD, &self.record())])
    }

    /// The record's bytes (the container's section 2).
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

    /// Reads the bytes of `write.journal` (§7.3 step 2), every case:
    ///
    /// - shorter than the head, or shorter than the total its head
    ///   declares: `Interrupted` (the publish, which syncs the whole file
    ///   before it returns, never returned, so no file was touched);
    /// - otherwise `Err`, malformed and kept: a head that is not a
    ///   journal's (magic, kind), fails its check, or is of another version;
    ///   bytes after the declared total; a container that fails
    ///   `spec/model.md` §5 rule 5 (its trailer, its framing); a record
    ///   that does not decode or breaks a rule;
    /// - else `Whole`.
    ///
    /// No damage to a head turns a whole journal into a short one: the
    /// total is trusted only under the head's check.
    pub fn read(bytes: &[u8]) -> Result<Read, String> {
        let len = bytes.len();
        if len < HEAD_LEN {
            return Ok(Read::Interrupted(format!(
                "the journal is {len} bytes, shorter than its {HEAD_LEN}-byte head"
            )));
        }
        let head = &bytes[..HEAD_LEN];
        if head[..8] != MAGIC || head[8..12] != KIND {
            return Err("journal: not a journal (its magic or kind)".to_string());
        }
        if head[16..18] != [1, 16] || head[CHECKED..] != head_check(head) {
            return Err("journal: its head fails its check".to_string());
        }
        let version = u32::from_le_bytes([head[12], head[13], head[14], head[15]]);
        if version != VERSION {
            return Err(format!("journal: version {version} (this Slip reads version {VERSION})"));
        }
        let mut t = [0u8; 8];
        t.copy_from_slice(&head[18..CHECKED]);
        let total = u64::from_le_bytes(t);
        let have = len as u64;
        if have < total {
            return Ok(Read::Interrupted(format!(
                "the journal is {len} bytes, shorter than the {total} its head declares"
            )));
        }
        if have > total {
            return Err(format!(
                "journal: {} bytes after the {total} its head declares",
                have - total
            ));
        }
        let r = Reader::open(bytes, KIND, VERSION).map_err(|e| format!("journal: {e}"))?;
        let _ = r.section(LENGTH);
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
