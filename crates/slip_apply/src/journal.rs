//! The journal of a write (`spec/edits.md` §7.3 step 2): one record in the
//! encoding of `spec/model.md` §2, with no container.
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
use slip_model::overlay::Content;
use slip_model::transform::is_file_path;
use slip_model::wire::{Decoder, Encoder};

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
    /// The record's bytes.
    pub fn to_bytes(&self) -> Vec<u8> {
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

    /// Reads a journal: the encoding, then the rules, each over the whole
    /// list in turn: not empty; every path a real path of a file; strictly
    /// ascending by path; every staged name the one of its path; `pre`
    /// differs from `post`.
    pub fn from_bytes(bytes: &[u8]) -> Result<Journal, String> {
        let j = decode(bytes).map_err(|e| format!("journal: {e}"))?;
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
