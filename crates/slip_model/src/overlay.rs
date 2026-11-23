//! The overlay, kind `OVLY` (`spec/edits.md` §7.1): the files of a
//! post-image and its spelling obligations (`spec/verify.md` §10.1). Work
//! package WP-061.
//!
//! Plain data, with the two lookups apply's readers need. The binary and
//! JSON forms and the invariants are in `formats`
//! (`spec/landed/edits/WP-061.md`).

use std::ops::Range;

use crate::Digest;
use crate::edits::KeyId;

/// The size and content digest of one image of a file (§7.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Content {
    pub size: u64,
    pub content: Digest,
}

/// A file the overlay replaces (§7.1): `post` equal to `pre` is valid.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OverlayFile {
    /// Its real path: canonical (`transform::is_canonical`).
    pub path: Vec<u8>,
    pub pre: Content,
    pub post: Content,
}

/// A spelling obligation (`spec/verify.md` §10.1). The derived order is
/// the overlay's: `(file, lo, hi, key)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Obligation {
    /// Index into the overlay's `files`.
    pub file: u64,
    pub lo: u64,
    /// `lo < hi <=` the file's `post.size`.
    pub hi: u64,
    /// The entity meant, in the key space of the pre-image.
    pub key: KeyId,
}

/// The overlay (§7.1).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Overlay {
    /// The edit set id.
    pub edits: Digest,
    /// Sorted by path, no two equal; not empty.
    pub files: Vec<OverlayFile>,
    /// Sorted by `(file, lo, hi, key)`, no two equal.
    pub spellings: Vec<Obligation>,
}

impl Overlay {
    /// The index in `files` of the file at `real_path` (a binary search:
    /// `files` is sorted by path).
    pub fn find(&self, real_path: &[u8]) -> Option<usize> {
        self.files.binary_search_by(|f| f.path.as_slice().cmp(real_path)).ok()
    }

    /// The obligations of the file at `index`, as a range of indices of
    /// `spellings` (they are sorted by file); empty if it has none.
    pub fn obligations_of(&self, index: usize) -> Range<usize> {
        let index = index as u64;
        let lo = self.spellings.partition_point(|o| o.file < index);
        let hi = self.spellings.partition_point(|o| o.file <= index);
        lo..hi
    }
}
