//! The blobs of an overlay image, loaded for one observer
//! (`spec/verify.md` §5.1 "Loading"; `spec/exec.md` §8.4). Work package
//! WP-074, item 2.
//!
//! A verify job through an overlay gives the front end the post-image
//! bytes of the overlay files the observer reads, and no others: those of
//! the pre shard's `inputs` first, then, while a parse read an overlay
//! file that was not given, those too. [`Loaded`] is that state: the
//! overlay, the blobs read so far (each checked against its name and its
//! size when it is read), and the questions the loop and the guards ask
//! about a parse's inputs.
//!
//! **Which input is an overlay file.** Kiln matches an overlay entry to a
//! file by identity, and an input keeps the path the unit reached the file
//! by (`slip_kiln::Unit::overlaid`; WP-066's F8). Here an input *is* the
//! entry whose path is the input's path, or, failing that, whose identity
//! (device, inode, read when first asked, once per job) is the identity of
//! the file at the input's path now. Whether the unit read it through the
//! overlay is never decided here from the path: the caller says, per
//! input, how the unit read it ([`Read`], from `Unit::overlaid`).

use std::collections::HashMap;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::MetadataExt;
use std::sync::{Arc, OnceLock};

use slip_digest::spelling::Obligation;
use slip_exec::store::Store;
use slip_model::{Digest, FileRef, Overlay};

/// How the unit read one of its inputs (`slip_kiln::Unit::overlaid` for a
/// buffer).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Read {
    /// A buffer the unit read through the overlay.
    Overlay,
    /// A buffer the unit read from the file system.
    Disk,
    /// A path the unit consulted without entering it (`__has_include`):
    /// Kiln gives it the overlay's bytes when the overlay replaces it.
    Consulted,
}

/// The overlay of a job and the blobs loaded so far for one observer.
#[derive(Debug)]
pub struct Loaded<'o> {
    overlay: &'o Overlay,
    id: Digest,
    /// The entries given so far, `(path, post-image bytes)`, sorted by
    /// path: what `ParseOptions::overlay` takes.
    entries: Vec<(&'o [u8], Arc<[u8]>)>,
    /// By index in the overlay's `files`: whether it is in `entries`.
    given: Vec<bool>,
    /// The bytes of the blobs loaded (the test of "no others").
    bytes: u64,
    /// The identity of each entry's file now, by identity; made once, when
    /// an input's path is not an entry's.
    identities: OnceLock<HashMap<(u64, u64), usize>>,
}

/// The identity (device, inode) of the file at `path`, links followed.
fn identity(path: &[u8]) -> Option<(u64, u64)> {
    std::fs::metadata(std::ffi::OsStr::from_bytes(path)).ok().map(|m| (m.dev(), m.ino()))
}

impl<'o> Loaded<'o> {
    /// Nothing loaded yet. `id` is the overlay's content id (for messages).
    pub fn new(overlay: &'o Overlay, id: Digest) -> Loaded<'o> {
        Loaded {
            overlay,
            id,
            entries: Vec::new(),
            given: vec![false; overlay.files.len()],
            bytes: 0,
            identities: OnceLock::new(),
        }
    }

    /// The overlay.
    pub fn overlay(&self) -> &'o Overlay {
        self.overlay
    }

    /// Loads the blobs of the overlay files at `indices` (into the
    /// overlay's `files`) that are not loaded yet; returns how many it
    /// loaded.
    ///
    /// # Errors
    ///
    /// A failure of the run, never a shard (`spec/exec.md` §5.4): an index
    /// that is not the overlay's, a blob that cannot be read, is not in
    /// the store, does not hash to its name, or is not of the size the
    /// overlay gives.
    pub fn load_indices(
        &mut self,
        store: &Store,
        indices: &mut dyn Iterator<Item = usize>,
    ) -> Result<usize, String> {
        let mut n = 0;
        for i in indices {
            let Some(file) = self.overlay.files.get(i) else {
                return Err(format!("the overlay {} has no file {i}", self.id));
            };
            if self.given[i] {
                continue;
            }
            let blob = file.post.content;
            let path = store.blob_path(&blob);
            let bytes = store
                .read_blob(&blob)
                .map_err(|e| format!("cannot read the overlay's blob {blob}: {e}"))?
                .ok_or_else(|| {
                    format!(
                        "the blob {blob} of {} (overlay {}) is not in the store ({})",
                        crate::image::show_path(&file.path),
                        self.id,
                        path.display()
                    )
                })?;
            if bytes.len() as u64 != file.post.size {
                return Err(format!(
                    "the blob {} has {} bytes; the overlay {} gives {} for {}",
                    path.display(),
                    bytes.len(),
                    self.id,
                    file.post.size,
                    crate::image::show_path(&file.path)
                ));
            }
            self.bytes += bytes.len() as u64;
            self.given[i] = true;
            self.entries.push((&file.path, Arc::from(bytes)));
            n += 1;
        }
        if n > 0 {
            self.entries.sort_by(|a, b| a.0.cmp(b.0));
        }
        Ok(n)
    }

    /// Loads the blobs of the overlay files among `paths` (by path: those
    /// the overlay names) that are not loaded yet; returns how many it
    /// loaded. A path the overlay does not name is skipped.
    ///
    /// # Errors
    ///
    /// As [`Loaded::load_indices`].
    pub fn load(
        &mut self,
        store: &Store,
        paths: &mut dyn Iterator<Item = &[u8]>,
    ) -> Result<usize, String> {
        let indices: Vec<usize> = paths.filter_map(|p| self.overlay.find(p)).collect();
        self.load_indices(store, &mut indices.into_iter())
    }

    /// Every file of the overlay (the pre shard is not usable).
    ///
    /// # Errors
    ///
    /// As [`Loaded::load_indices`].
    pub fn load_all(&mut self, store: &Store) -> Result<usize, String> {
        self.load_indices(store, &mut (0..self.overlay.files.len()))
    }

    /// The overlay file `path` is, by path or else by identity (the module
    /// documentation): its index in the overlay's `files`.
    pub fn entry_of(&self, path: &[u8]) -> Option<usize> {
        if let Some(i) = self.overlay.find(path) {
            return Some(i);
        }
        let id = identity(path)?;
        let index = self.identities.get_or_init(|| {
            self.overlay
                .files
                .iter()
                .enumerate()
                .filter_map(|(i, f)| identity(&f.path).map(|id| (id, i)))
                .collect()
        });
        index.get(&id).copied()
    }

    /// The overlay files that a parse with `inputs` (each read as `read`
    /// says, parallel) read from the file system although the overlay
    /// names them, and that are not loaded: the files to give and parse
    /// again for. Ascending by index (by path), no two equal.
    pub fn missing(&self, inputs: &[FileRef], read: &[Read]) -> Vec<usize> {
        let mut out: Vec<usize> = inputs
            .iter()
            .zip(read)
            .filter(|(_, r)| **r != Read::Overlay)
            .filter_map(|(f, _)| self.entry_of(&f.path))
            .filter(|&i| !self.given[i])
            .collect();
        out.sort_unstable();
        out.dedup();
        out
    }

    /// Guard O1's part that only the unit can answer (§5.3): the first
    /// input, in path order, that is an overlay file and was not read
    /// through the overlay: a buffer read from the file system, or a path
    /// consulted that is an overlay file not given (Kiln gives a consulted
    /// path the overlay's bytes when it is given).
    pub fn not_overlaid<'a>(&self, inputs: &'a [FileRef], read: &[Read]) -> Option<&'a [u8]> {
        inputs
            .iter()
            .zip(read)
            .find(|(f, r)| match r {
                Read::Overlay => false,
                Read::Disk => self.entry_of(&f.path).is_some(),
                Read::Consulted => self.entry_of(&f.path).is_some_and(|i| !self.given[i]),
            })
            .map(|(f, _)| f.path.as_slice())
    }

    /// The entries given so far, sorted by path, as
    /// `slip_kiln::ParseOptions::overlay` takes them.
    pub fn entries(&self) -> &[(&'o [u8], Arc<[u8]>)] {
        &self.entries
    }

    /// `slip_kiln::ParseOptions` with the entries given so far, and
    /// `written` when the overlay has obligations (the checks of
    /// `spec/verify.md` §10.2 read Kiln's written-reference table).
    pub fn parse_options(&self) -> slip_kiln::ParseOptions<'_> {
        let mut options = slip_kiln::ParseOptions::default();
        options.overlay = &self.entries;
        options.written = !self.overlay.spellings.is_empty();
        options
    }

    /// The obligations of the overlay files in `inputs` (by path, as the
    /// fold counts `seen`, `spec/verify.md` §10.4), as `check_spellings`
    /// takes them (the file by its subject path, which in the overlay form
    /// is the real path), with each one's index in the overlay's
    /// `spellings`. Ascending by index.
    ///
    /// # Errors
    ///
    /// An obligation whose bounds do not fit the checker's `u32` (a file of
    /// 4 GiB or more): a failure of the job, never an obligation dropped.
    pub fn obligations(&self, inputs: &[FileRef]) -> Result<(Vec<Obligation>, Vec<u64>), String> {
        let mut out = Vec::new();
        let mut indices = Vec::new();
        let mut files: Vec<usize> =
            inputs.iter().filter_map(|f| self.overlay.find(&f.path)).collect();
        files.sort_unstable();
        files.dedup();
        for file in files {
            for index in self.overlay.obligations_of(file) {
                let o = &self.overlay.spellings[index];
                let bound = |n: u64| {
                    u32::try_from(n).map_err(|_| {
                        format!("an obligation of the overlay {} ends past 4 GiB", self.id)
                    })
                };
                out.push(Obligation {
                    file: self.overlay.files[file].path.clone(),
                    lo: bound(o.lo)?,
                    hi: bound(o.hi)?,
                    key: o.key,
                });
                indices.push(index as u64);
            }
        }
        Ok((out, indices))
    }

    /// The bytes of the blobs loaded so far.
    pub fn bytes_loaded(&self) -> u64 {
        self.bytes
    }

    /// The number of overlay files loaded so far.
    pub fn files_loaded(&self) -> usize {
        self.entries.len()
    }
}

/// How `unit` read each of its inputs (parallel to `unit.inputs()`): a
/// buffer through the overlay or from the file system, as
/// `slip_kiln::Unit::overlaid` says, else a path consulted. A path that is
/// two buffers, one read from the file system, is read from it.
pub fn reads(unit: &slip_kiln::Unit<'_>) -> Vec<Read> {
    let inputs = unit.inputs();
    let mut read = vec![Read::Consulted; inputs.len()];
    for file in unit.sm().file_ids() {
        let Some(path) = unit.real_path(file) else { continue };
        let Ok(i) = inputs.binary_search_by(|f| f.path.as_slice().cmp(path)) else { continue };
        read[i] = match (read[i], unit.overlaid(file)) {
            (Read::Disk, _) | (_, false) => Read::Disk,
            (_, true) => Read::Overlay,
        };
    }
    read
}
