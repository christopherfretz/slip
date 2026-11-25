//! `plan` and `build` (`spec/edits.md` §7.1): the layout of an accepted
//! edit set (the nesting of its edits, where each copy lands among them,
//! an order in which every expansion follows those it draws on, and the
//! length of every expansion), then the post-images built in that order.
//!
//! Nothing here recurses on the call stack: the nesting is built with a
//! stack of open edits, the order by an iterative depth-first search, and
//! the expansions in that order (`spec/landed/edits/WP-063.md` §1).

use std::cmp::Reverse;
use std::collections::BTreeSet;

use slip_model::Digest;
use slip_model::edits::Piece;
use slip_model::editset::Edit;
use slip_model::editset::{EditSet, SetState};
use slip_model::formats;
use slip_model::hash::content_digest;
use slip_model::overlay::{Content, Obligation, Overlay, OverlayFile};

use crate::compose::Work;
use crate::{ApplyError, Built, Files};

/// The largest post-image apply builds, in bytes (§7.1 step 3).
pub const POST_MAX: u64 = 1 << 31;

/// One copy piece, placed among the edits of the file it copies from.
#[derive(Clone, Copy, Debug)]
pub(crate) struct CopyRef {
    /// Index into the set's `files`.
    pub file: usize,
    pub lo: u64,
    pub hi: u64,
    /// The container (an edit, or a file's root) whose children
    /// `first..end` are the edits properly inside `[lo, hi)` and within no
    /// other edit properly inside it.
    pub container: usize,
    pub first: usize,
    pub end: usize,
}

/// The layout of an accepted edit set: what `plan` computes and `build`
/// uses. Containers `0..E` are the edits, `E + f` the root of file `f`.
#[derive(Debug)]
pub(crate) struct Layout<'a> {
    pub set: &'a EditSet,
    /// For each container, the edits directly inside it, by `lo`.
    pub children: Vec<Vec<usize>>,
    /// The copy pieces of edit `e` are `copies[copy_at[e]..copy_at[e + 1]]`,
    /// in piece order.
    pub copy_at: Vec<usize>,
    pub copies: Vec<CopyRef>,
    /// Every edit, each after the edits it draws on and those inside it.
    pub order: Vec<usize>,
    /// The length of each edit's expansion (saturating; at most
    /// [`POST_MAX`] once `plan` has returned).
    pub lengths: Vec<u64>,
    /// The post-image size of each file with an edit.
    pub post_sizes: Vec<Option<u64>>,
}

impl Layout<'_> {
    pub fn edits(&self) -> usize {
        self.set.edits.len()
    }

    pub fn edit(&self, e: usize) -> &Edit {
        &self.set.edits[e]
    }

    pub fn root(&self, file: usize) -> usize {
        self.edits() + file
    }

    pub fn copies_of(&self, e: usize) -> &[CopyRef] {
        &self.copies[self.copy_at[e]..self.copy_at[e + 1]]
    }

    /// The edits a copy carries at its own level: properly inside its
    /// range and within no other edit properly inside it.
    pub fn carried(&self, c: &CopyRef) -> &[usize] {
        &self.children[c.container][c.first..c.end]
    }
}

fn invalid_edit(e: usize, what: &str) -> ApplyError {
    ApplyError::Invalid(format!("sections.set.edits[{e}]: {what}"))
}

/// Builds the layout and checks §7.1 step 2 and the length rule of step 3.
#[allow(clippy::many_single_char_names, reason = "the names of the definitions")]
pub(crate) fn layout(set: &EditSet) -> Result<Layout<'_>, ApplyError> {
    if set.state != SetState::Accepted {
        return Err(ApplyError::NotAccepted);
    }
    set.validate().map_err(ApplyError::Invalid)?;
    let n = set.edits.len();
    let ed = &set.edits;
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); n + set.files.len()];

    // The copy pieces, in edit and piece order, and their target ranges.
    let mut copy_at = Vec::with_capacity(n + 1);
    let mut copies = Vec::new();
    for e in ed {
        copy_at.push(copies.len());
        for p in &e.pieces {
            if let Piece::Copy(place) = p {
                copies.push(CopyRef {
                    file: place.file as usize,
                    lo: place.lo,
                    hi: place.hi,
                    container: usize::MAX,
                    first: 0,
                    end: 0,
                });
            }
        }
    }
    copy_at.push(copies.len());
    let mut by_target: Vec<usize> = (0..copies.len()).collect();
    by_target.sort_by_key(|&c| (copies[c].file, copies[c].lo, Reverse(copies[c].hi)));

    // The nesting of each file's edits, and the innermost edit containing
    // each copied range: one sweep per file over its edits in pre-order
    // (lo ascending, hi descending) merged with the copied ranges in the
    // same order, an edit before a copy of its own range. Validation made
    // the edits of a file disjoint or nested and no copied range crossed
    // by an edit, so the open edits form a chain, and once those that end
    // at or before a start are closed, every open edit contains what
    // starts there.
    let mut next_copy = 0;
    let mut start = 0;
    while start < n || next_copy < by_target.len() {
        let file = match (ed.get(start), by_target.get(next_copy)) {
            (Some(e), Some(&c)) => (e.file as usize).min(copies[c].file),
            (Some(e), None) => e.file as usize,
            (None, Some(&c)) => copies[c].file,
            (None, None) => unreachable!("the loop runs while one list remains"),
        };
        let end = start + ed[start..].partition_point(|e| e.file as usize == file);
        let mut pre: Vec<usize> = (start..end).collect();
        pre.sort_by_key(|&x| (ed[x].lo, Reverse(ed[x].hi)));
        let copy_end =
            next_copy + by_target[next_copy..].partition_point(|&c| copies[c].file == file);
        let root = n + file;
        let mut open: Vec<usize> = Vec::new();
        let (mut i, mut j) = (0, next_copy);
        while i < pre.len() || j < copy_end {
            let take_edit = match (pre.get(i), by_target[..copy_end].get(j)) {
                (Some(&x), Some(&c)) => {
                    (ed[x].lo, Reverse(ed[x].hi)) <= (copies[c].lo, Reverse(copies[c].hi))
                }
                (Some(_), None) => true,
                _ => false,
            };
            let at = if take_edit { ed[pre[i]].lo } else { copies[by_target[j]].lo };
            while open.last().is_some_and(|&t| ed[t].hi <= at) {
                open.pop();
            }
            let container = open.last().copied().unwrap_or(root);
            if take_edit {
                children[container].push(pre[i]);
                open.push(pre[i]);
                i += 1;
            } else {
                copies[by_target[j]].container = container;
                j += 1;
            }
        }
        start = end;
        next_copy = copy_end;
    }
    for c in &mut copies {
        let sibs = &children[c.container];
        c.first = sibs.partition_point(|&m| ed[m].lo < c.lo);
        c.end = sibs.partition_point(|&m| ed[m].lo < c.hi);
        debug_assert!(
            sibs[c.first..c.end].iter().all(|&m| c.lo <= ed[m].lo
                && ed[m].hi <= c.hi
                && (ed[m].lo, ed[m].hi) != (c.lo, c.hi)),
            "a carried edit is properly inside its copy"
        );
    }
    let mut lay = Layout {
        set,
        children,
        copy_at,
        copies,
        order: Vec::with_capacity(n),
        lengths: vec![0; n],
        post_sizes: vec![None; set.files.len()],
    };
    lay.order = order(&lay)?;
    effective(&lay)?;
    lengths(&mut lay)?;
    Ok(lay)
}

/// An iterative depth-first search over "is inside" and "carries in a
/// copy": its post-order puts each edit after everything it draws on.
/// "Draws on" (§5.4) is "carries" followed by any number of "is inside",
/// so a cycle of it is exactly a cycle of this graph (`spec/landed/edits/
/// WP-063.md` §1); the first edit found on one is reported.
fn order(lay: &Layout<'_>) -> Result<Vec<usize>, ApplyError> {
    let n = lay.edits();
    let mut color = vec![0u8; n]; // 0 unseen, 1 on the path, 2 done
    let mut out = Vec::with_capacity(n);
    // A frame: the edit, the list it is in (0 its children, k its k-th copy), the position.
    let mut stack: Vec<(usize, usize, usize)> = Vec::new();
    for s in 0..n {
        if color[s] != 0 {
            continue;
        }
        color[s] = 1;
        stack.push((s, 0, 0));
        while let Some(top) = stack.last_mut() {
            let e = top.0;
            let ncopies = lay.copy_at[e + 1] - lay.copy_at[e];
            let next = loop {
                let list: &[usize] = if top.1 == 0 {
                    &lay.children[e]
                } else {
                    lay.carried(&lay.copies[lay.copy_at[e] + top.1 - 1])
                };
                if let Some(&m) = list.get(top.2) {
                    top.2 += 1;
                    break Some(m);
                }
                if top.1 == ncopies {
                    break None;
                }
                top.1 += 1;
                top.2 = 0;
            };
            match next {
                Some(m) if color[m] == 1 => {
                    return Err(invalid_edit(
                        m,
                        "the edit draws on itself, directly or through a chain of copies",
                    ));
                }
                Some(m) if color[m] == 0 => {
                    color[m] = 1;
                    stack.push((m, 0, 0));
                }
                Some(_) => {}
                None => {
                    color[e] = 2;
                    out.push(e);
                    stack.pop();
                }
            }
        }
    }
    Ok(out)
}

/// §5.5: an edit is effective if it is outermost, or carried by a copy of
/// an effective edit. Every edit must be.
fn effective(lay: &Layout<'_>) -> Result<(), ApplyError> {
    let n = lay.edits();
    let mut eff = vec![false; n];
    let mut queue: Vec<usize> = Vec::new();
    for f in 0..lay.set.files.len() {
        for &r in &lay.children[lay.root(f)] {
            eff[r] = true;
            queue.push(r);
        }
    }
    while let Some(e) = queue.pop() {
        for c in lay.copies_of(e) {
            for &m in lay.carried(c) {
                if !eff[m] {
                    eff[m] = true;
                    queue.push(m);
                }
            }
        }
    }
    match eff.iter().position(|&x| !x) {
        Some(e) => Err(invalid_edit(
            e,
            "the edit is not effective: it is inside another edit and no copy carries it",
        )),
        None => Ok(()),
    }
}

/// The length of a copy's text: its range less the edits it carries, plus
/// their expansions.
fn copy_length(lay: &Layout<'_>, c: &CopyRef) -> u64 {
    let mut len = c.hi - c.lo;
    let mut grown = 0u64;
    for &m in lay.carried(c) {
        let e = lay.edit(m);
        len -= e.hi - e.lo;
        grown = grown.saturating_add(lay.lengths[m]);
    }
    len.saturating_add(grown)
}

/// The expansion lengths in `order`, then each post-image's size;
/// `TooLarge` for the first file, by path, whose post-image passes
/// [`POST_MAX`]. Every edit is effective, so every expansion is part of
/// some post-image, and one that passes the bound makes its file pass it.
fn lengths(lay: &mut Layout<'_>) -> Result<(), ApplyError> {
    for k in 0..lay.order.len() {
        let e = lay.order[k];
        let mut len = 0u64;
        let mut copy = lay.copy_at[e];
        for p in &lay.set.edits[e].pieces {
            let add = match p {
                Piece::Literal(b) => b.len() as u64,
                Piece::Spelled { text, .. } | Piece::Identifier { text, .. } => text.len() as u64,
                Piece::Copy(_) => {
                    copy += 1;
                    copy_length(lay, &lay.copies[copy - 1])
                }
            };
            len = len.saturating_add(add);
        }
        lay.lengths[e] = len;
    }
    for f in 0..lay.set.files.len() {
        let roots = &lay.children[lay.root(f)];
        if roots.is_empty() {
            continue;
        }
        let file = &lay.set.files[f];
        let size = copy_length(
            lay,
            &CopyRef {
                file: f,
                lo: 0,
                hi: file.size,
                container: lay.root(f),
                first: 0,
                end: roots.len(),
            },
        );
        if size > POST_MAX {
            return Err(ApplyError::TooLarge(file.path.clone()));
        }
        lay.post_sizes[f] = Some(size);
    }
    Ok(())
}

/// §7.1 steps 2 and 3 without the files' bytes.
pub fn plan(set: &EditSet) -> Result<Vec<u64>, ApplyError> {
    Ok(layout(set)?.lengths)
}

/// An overlay names a file once (`spec/edits.md` §7.1; WP-074's second
/// review, F1): two files of `set` that are one file now (the same device
/// and inode: two hard links) would give it two post-images. `Invalid`
/// naming both paths, in path order, for the first such pair; a path that
/// finds nothing is left to the reads after (`StaleFile`). One `stat` per
/// file of the set. What the builder asks first, before any post-image is
/// made or handed to its sink, and what `slip reduce` asks of the edit
/// set it makes before publishing it.
pub fn one_name_per_file(set: &EditSet, files: &dyn Files) -> Result<(), ApplyError> {
    let mut seen: std::collections::HashMap<(u64, u64), usize> = std::collections::HashMap::new();
    for (f, file) in set.files.iter().enumerate() {
        let id = files
            .identity(&file.path)
            .map_err(|e| ApplyError::Io(file.path.clone(), e.to_string()))?;
        let Some(id) = id else { continue };
        if let Some(&g) = seen.get(&id) {
            return Err(ApplyError::Invalid(format!(
                "{} and {} are one file (device {}, inode {}): an overlay names a file once \
                 (spec/edits.md §7.1)",
                String::from_utf8_lossy(&set.files[g].path),
                String::from_utf8_lossy(&file.path),
                id.0,
                id.1
            )));
        }
        seen.insert(id, f);
    }
    Ok(())
}

/// §7.1: the overlay and the post-image bytes, collected.
pub fn build(set: &EditSet, set_id: Digest, files: &dyn Files) -> Result<Built, ApplyError> {
    let mut blobs = Vec::new();
    let overlay = build_into(set, set_id, files, &mut |f, bytes| {
        blobs.push((f.post.content, bytes.to_vec()));
        Ok(())
    })?;
    Ok(Built { overlay, blobs })
}

/// `build` with a sink: called once per overlay file, in path order, with
/// the post-image bytes. On an error the sink may have been called for
/// the files before (`spec/landed/edits/WP-063.md` §2); never for a set
/// two of whose files are one file ([`one_name_per_file`], asked first).
pub fn build_into(
    set: &EditSet,
    set_id: Digest,
    files: &dyn Files,
    sink: &mut dyn FnMut(&OverlayFile, &[u8]) -> std::io::Result<()>,
) -> Result<Overlay, ApplyError> {
    let lay = layout(set)?;
    match formats::content_id(&set.to_bytes()) {
        Ok(id) if id == set_id => {}
        _ => {
            return Err(ApplyError::Invalid(
                "the edit set id is not the content id of the edit set".to_string(),
            ));
        }
    }
    one_name_per_file(set, files)?;
    let mut work = Work::new(&lay);
    let mut out_files = Vec::new();
    let mut spellings = Vec::new();
    for f in 0..set.files.len() {
        let Some(size) = lay.post_sizes[f] else { continue };
        let (post, spells) = work.post_image(f, files)?;
        assert_eq!(post.len() as u64, size, "a post-image has the length plan computed");
        let file = &set.files[f];
        let entry = OverlayFile {
            path: file.path.clone(),
            pre: Content { size: file.size, content: file.content },
            post: Content { size, content: content_digest(&post) },
        };
        sink(&entry, &post).map_err(|e| ApplyError::Io(file.path.clone(), e.to_string()))?;
        let index = out_files.len() as u64;
        spellings.extend(spells.into_iter().map(|(lo, hi, key)| Obligation {
            file: index,
            lo,
            hi,
            key,
        }));
        out_files.push(entry);
    }
    spellings.sort_unstable();
    assert!(
        spellings.windows(2).all(|w| w[0] < w[1]),
        "two landings of spelled text are distinct ranges of a post-image"
    );
    let overlay = Overlay { edits: set_id, files: out_files, spellings };
    debug_assert_eq!(overlay.validate(), Ok(()), "build makes a valid overlay");
    Ok(overlay)
}

/// Reads one file of the pre-image and checks it against its `FileRef`.
pub(crate) fn read_checked(
    set: &EditSet,
    f: usize,
    files: &dyn Files,
) -> Result<Vec<u8>, ApplyError> {
    let file = &set.files[f];
    match files.read(&file.path) {
        Ok(bytes) if bytes.len() as u64 == file.size && content_digest(&bytes) == file.content => {
            Ok(bytes)
        }
        _ => Err(ApplyError::StaleFile(file.path.clone())),
    }
}

/// The files a post-image needs: its own and those its edits copy from,
/// through every carried edit.
pub(crate) fn files_needed(lay: &Layout<'_>, f: usize, needed: &[usize]) -> BTreeSet<usize> {
    let mut out = BTreeSet::from([f]);
    for &e in needed {
        out.extend(lay.copies_of(e).iter().map(|c| c.file));
    }
    out
}
