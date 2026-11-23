//! The overlay's guards O1 and O2 (`spec/verify.md` §5.1 "What guards
//! it", §5.3), as one pure function the verify job calls in step 2. Work
//! package WP-070.
//!
//! Trusted base: a guard that passes when it should not lets a parse that
//! was not of the post-image the overlay describes, or of a pre-image the
//! map phase saw, be compared as if it were. So the function depends on no
//! order of its inputs: it sorts what it reads, and a path listed twice
//! must agree with every entry of it.

use std::fmt::Write as _;

use slip_model::{FileRef, Overlay};

/// The guard that failed, and the path its reason names (§5.3).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum OverlayGuard {
    /// An entry of the post-image parse's `inputs` whose path the overlay
    /// names does not have the overlay's `post` size and content digest.
    O1(Vec<u8>),
    /// An entry whose path the overlay does not name, and that the pre
    /// shard's `inputs` list, does not have the size and content digest the
    /// pre shard gives it.
    O2(Vec<u8>),
}

impl OverlayGuard {
    /// The `reason` of the `unverifiable` verdict, as §5.3's table gives
    /// it; the caller cuts it as §6 says (`compare::unverifiable`).
    pub fn reason(&self) -> String {
        match self {
            OverlayGuard::O1(p) => {
                format!("post-image: {} was not read through the overlay", reason_path(p))
            }
            OverlayGuard::O2(p) => {
                format!("pre-image: {} changed since the map phase", reason_path(p))
            }
        }
    }
}

/// A path in a reason (§5.3): its bytes where they are UTF-8, each other
/// byte as `\xNN` in lower-case hexadecimal.
fn reason_path(bytes: &[u8]) -> String {
    let mut out = String::new();
    for chunk in bytes.utf8_chunks() {
        out.push_str(chunk.valid());
        for b in chunk.invalid() {
            let _ = write!(out, "\\x{b:02x}");
        }
    }
    out
}

/// O1 over every entry of `post`, then O2: the first failure in that
/// order, each in path order. `pre` and `post` are `inputs` lists: the
/// pre shard's and the post-image parse's (§5.2 step 1).
///
/// - **O1**: every entry of `post` whose path the overlay names has the
///   overlay's `post` size and content digest. An overlay entry with
///   `post == pre` is no exception.
/// - **O2**: every entry of `post` whose path the overlay does not name and
///   that `pre` lists has the size and content digest `pre` gives it.
///
/// A file in only one of the two lists is not an error, and an overlay
/// file the post-image did not read is not one. Neither list need be
/// sorted: the entries are taken in path order, and a path that `pre`
/// lists more than once must agree with each of them (one that `post`
/// lists more than once is checked once per entry).
pub fn overlay_inputs(
    pre: &[FileRef],
    post: &[FileRef],
    overlay: &Overlay,
) -> Result<(), OverlayGuard> {
    let mut post: Vec<&FileRef> = post.iter().collect();
    post.sort_by(|a, b| a.path.cmp(&b.path));
    let named = |f: &FileRef| overlay.files.iter().find(|o| o.path == f.path);
    // The overlay's files are sorted by path when it is valid; a search
    // that does not rely on it costs a scan per entry only when it is not.
    let sorted = overlay.files.windows(2).all(|w| w[0].path < w[1].path);
    let find = |f: &FileRef| {
        if sorted { overlay.find(&f.path).map(|i| &overlay.files[i]) } else { named(f) }
    };
    for f in &post {
        if let Some(o) = find(f)
            && (f.size != o.post.size || f.content != o.post.content)
        {
            return Err(OverlayGuard::O1(f.path.clone()));
        }
    }
    let mut pre: Vec<&FileRef> = pre.iter().collect();
    pre.sort_by(|a, b| a.path.cmp(&b.path));
    for f in &post {
        if find(f).is_some() {
            continue;
        }
        let lo = pre.partition_point(|p| p.path < f.path);
        let hi = pre.partition_point(|p| p.path <= f.path);
        if pre[lo..hi].iter().any(|p| p.size != f.size || p.content != f.content) {
            return Err(OverlayGuard::O2(f.path.clone()));
        }
    }
    Ok(())
}
