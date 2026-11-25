//! The post-image of a verify job (`spec/verify.md` §5.1; `spec/exec.md`
//! §8.6). Work package WP-052: the **tree form**, a set of root maps
//! `(subject, post)`; the overlay form is `docs/wp/WP-074.md`'s.
//!
//! - [`Image::new`] makes the checks of §5.1 that need no file system;
//!   [`Image::from_args`] (the command line) and [`Image::from_post`] (a
//!   job) check that every root is the real path of a directory first.
//! - [`Image::command`] is the post command, [`Image::to_subject`] the
//!   subject path of a real path, [`Image::under_subject`] guard G1's test.
//! - [`Image::parse`] is one parse of an observer's pre- or post-image
//!   with the walk's input.
//!
//! Every path here is absolute: a parse changes the process's current
//! directory (`slip_kiln::parse_observer_with`).

use std::collections::{HashMap, VecDeque};
use std::fmt;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use slip_digest::Input;
use slip_digest::map::with_input;
use slip_exec::store::{Object, Store};
use slip_exec::worker::PostImage;
use slip_kiln::{Lookups, ParseOptions, Unit, parse_observer_with};
use slip_model::formats::content_id;
use slip_model::{Command, Digest, Observer, Overlay, Phase, Shard, Status};

use crate::args::ImageArgs;

mod maps;

/// The post command of an observer (§5.1) and what guard G5 (§5.3) reads
/// of it ([`Image::post_command`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PostCommand {
    pub command: Command,
    /// The map pairs `(post prefix, prefix as written)` the command ends
    /// with, as its `-fmacro-prefix-map=` words give them, longest post
    /// prefix first; the identity pairs are not among them.
    pub maps: Vec<(Vec<u8>, Vec<u8>)>,
    /// Guard G5's reason, when the pairs cannot give every spelling of a
    /// file the post parse makes the spelling it has when the post tree
    /// stands in the subject's place.
    pub conflict: Option<String>,
}

/// One root map: the files under `subject` are, in the post-image, the
/// files at the same place under `post` (§5.1). Both are real paths of
/// directories, as bytes.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RootMap {
    pub subject: Vec<u8>,
    pub post: Vec<u8>,
}

/// A post-image (§5.1). Of the tree form: root maps sorted by `subject`,
/// none of whose roots equals or lies under another; no root map is the
/// identity. Of the overlay form (`docs/wp/WP-074.md`): no root map, and
/// the overlay ([`Image::overlay`]).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Image {
    maps: Vec<RootMap>,
    overlay: Option<OverlayImage>,
}

/// The overlay of an image of the overlay form, with its content id; equal
/// and hashed by the id (the content id of the `OVLY` it was read from).
#[derive(Clone, Debug)]
struct OverlayImage {
    id: Digest,
    overlay: Arc<Overlay>,
    /// A test's hook: the parses the loading loop may make (§5.1
    /// "Loading"), `None` for as many as it needs. Not part of the image's
    /// identity.
    parses: Option<usize>,
}

impl PartialEq for OverlayImage {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for OverlayImage {}

impl std::hash::Hash for OverlayImage {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

/// Why root maps are not an image (§5.1). Each names the root it is about.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ImageError {
    /// A root does not start with `/`.
    NotAbsolute(Vec<u8>),
    /// A root is `/`.
    IsRoot,
    /// A root is not in the form of a real path: it ends with `/`, or has
    /// an empty, `.` or `..` component. (Not in `spec/exec.md` §8.6's
    /// list: guard G1 compares roots as bytes with real paths, so a root
    /// in another form would let a read of the subject through.)
    NotNormal(Vec<u8>),
    /// `inner` lies under `outer` (`/a/b` under `/a`; `/a/bc` is not).
    Nested { inner: Vec<u8>, outer: Vec<u8> },
    /// Two roots of the image, subject or post, are equal.
    Duplicate(Vec<u8>),
    /// A root is not UTF-8.
    NotUtf8(Vec<u8>),
    /// A post root contains `=` (it would end the `-fmacro-prefix-map=`
    /// word's first half).
    EqualsInPost(Vec<u8>),
}

impl fmt::Display for ImageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let p = |b: &[u8]| String::from_utf8_lossy(b).into_owned();
        match self {
            ImageError::NotAbsolute(r) => write!(f, "{} is not an absolute path", p(r)),
            ImageError::IsRoot => f.write_str("/ cannot be a root of an image"),
            ImageError::NotNormal(r) => write!(f, "{} is not a real path", p(r)),
            ImageError::Nested { inner, outer } => {
                write!(f, "{} lies under {}", p(inner), p(outer))
            }
            ImageError::Duplicate(r) => write!(f, "{} is a root of the image twice", p(r)),
            ImageError::NotUtf8(r) => write!(f, "{} is not UTF-8", p(r)),
            ImageError::EqualsInPost(r) => write!(f, "the post root {} contains `=`", p(r)),
        }
    }
}

impl std::error::Error for ImageError {}

/// Which image of an observer [`Image::parse`] parses.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Side {
    /// The observer's own command; real paths are subject paths.
    Pre,
    /// The post command (§5.1); subject paths by [`Image::to_subject`].
    Post,
}

/// `a` is `b` or lies under it, as bytes (`/a/b` under `/a`; `/a/bc` not).
fn at_or_under(a: &[u8], b: &[u8]) -> bool {
    a.strip_prefix(b).is_some_and(|rest| rest.is_empty() || rest[0] == b'/')
}

/// A root may lie under another only when it is a post root and the other
/// a subject root (§5.1).
fn may_nest(inner_post: bool, outer_post: bool) -> bool {
    inner_post && !outer_post
}

/// The checks of one root (§5.1), in the order of the variants.
fn check_root(root: &[u8], post: bool) -> Result<(), ImageError> {
    if root.first() != Some(&b'/') {
        return Err(ImageError::NotAbsolute(root.to_vec()));
    }
    if root == b"/" {
        return Err(ImageError::IsRoot);
    }
    let normal = !root.ends_with(b"/")
        && root[1..].split(|&b| b == b'/').all(|c| !c.is_empty() && c != b"." && c != b"..");
    if !normal {
        return Err(ImageError::NotNormal(root.to_vec()));
    }
    if std::str::from_utf8(root).is_err() {
        return Err(ImageError::NotUtf8(root.to_vec()));
    }
    if post && root.contains(&b'=') {
        return Err(ImageError::EqualsInPost(root.to_vec()));
    }
    Ok(())
}

#[cfg(unix)]
fn path_bytes(p: &Path) -> Vec<u8> {
    use std::os::unix::ffi::OsStrExt;
    p.as_os_str().as_bytes().to_vec()
}

#[cfg(unix)]
fn bytes_path(b: &[u8]) -> PathBuf {
    use std::os::unix::ffi::OsStrExt;
    PathBuf::from(std::ffi::OsStr::from_bytes(b))
}

/// The real path of `given`, which must be a directory: links resolved
/// and each component spelled as its directory lists it
/// (`slip_spelling::real_path`, `spec/edits.md` §2.1 "One spelling").
fn real_dir(given: &Path) -> Result<Vec<u8>, String> {
    let real = slip_spelling::real_path(given).map_err(|e| e.detail())?;
    let meta = std::fs::metadata(&real).map_err(|e| e.to_string())?;
    if !meta.is_dir() {
        return Err("not a directory".to_owned());
    }
    Ok(path_bytes(&real))
}

impl Image {
    /// The identity: the post-image is the subject as it is on disk.
    pub fn identity() -> Image {
        Image { maps: Vec::new(), overlay: None }
    }

    /// The image of the overlay form (§5.1, `docs/wp/WP-074.md`): the
    /// subject with the content of the files `overlay` names replaced,
    /// `id` being the content id of its `OVLY`. The post command is the
    /// observer's, and the subject path of a real path is the path.
    pub fn overlay(id: Digest, overlay: Arc<Overlay>) -> Image {
        Image { maps: Vec::new(), overlay: Some(OverlayImage { id, overlay, parses: None }) }
    }

    /// The content id of the image's `OVLY`, for an image of the overlay
    /// form; `None` for a tree (the `overlay` of the shard's `binding`,
    /// `spec/verify.md` §9.1).
    pub fn overlay_id(&self) -> Option<Digest> {
        self.overlay.as_ref().map(|o| o.id)
    }

    /// The overlay and its content id, for an image of the overlay form.
    pub fn overlay_of(&self) -> Option<(Digest, &Overlay)> {
        self.overlay.as_ref().map(|o| (o.id, &*o.overlay))
    }

    /// A test's hook (`docs/wp/WP-074.md` "Tests"): the loading loop of an
    /// overlay image stops after `parses` parses, whatever it would load
    /// next, so that the last parse can read an overlay file from disk
    /// (guard O1). No effect on a tree.
    #[doc(hidden)]
    #[must_use]
    pub fn with_loading_limit(mut self, parses: usize) -> Image {
        if let Some(o) = &mut self.overlay {
            o.parses = Some(parses);
        }
        self
    }

    /// The test hook's limit, `None` without one.
    pub(crate) fn loading_limit(&self) -> Option<usize> {
        self.overlay.as_ref().and_then(|o| o.parses)
    }

    /// The checks of §5.1 that need no file system, then the maps sorted
    /// by `subject`. Each root, subject then post, map by map in the order
    /// given: absolute, not `/`, in the form of a real path, UTF-8, and no
    /// `=` in a post root; then over every pair of roots: none equals or
    /// lies under another.
    pub fn new(maps: Vec<RootMap>) -> Result<Image, ImageError> {
        for m in &maps {
            check_root(&m.subject, false)?;
            check_root(&m.post, true)?;
        }
        // Each root with whether it is a post root.
        let roots: Vec<(&[u8], bool)> = maps
            .iter()
            .flat_map(|m| [(m.subject.as_slice(), false), (m.post.as_slice(), true)])
            .collect();
        for (i, &(a, a_post)) in roots.iter().enumerate() {
            for &(b, b_post) in &roots[i + 1..] {
                if a == b {
                    return Err(ImageError::Duplicate(a.to_vec()));
                }
                // A post root may lie under a subject root (§5.1).
                if at_or_under(a, b) && !may_nest(a_post, b_post) {
                    return Err(ImageError::Nested { inner: a.to_vec(), outer: b.to_vec() });
                }
                if at_or_under(b, a) && !may_nest(b_post, a_post) {
                    return Err(ImageError::Nested { inner: b.to_vec(), outer: a.to_vec() });
                }
            }
        }
        let mut maps = maps;
        maps.sort();
        Ok(Image { maps, overlay: None })
    }

    /// The image of `--root`/`--post` (`spec/exec.md` §8.7): each
    /// directory made a real path (`slip_spelling::real_path`: links
    /// resolved, spelled on disk; it must be a directory), then
    /// [`Image::new`]. `Err` is `image: <path as given>:
    /// <error>` or `image: <ImageError>`.
    pub fn from_args(args: &ImageArgs) -> Result<Image, String> {
        if args.roots.len() != args.posts.len() {
            return Err(format!(
                "image: {} --root and {} --post: they come in pairs",
                args.roots.len(),
                args.posts.len()
            ));
        }
        let real = |p: &PathBuf| real_dir(p).map_err(|e| format!("image: {}: {e}", p.display()));
        let mut maps = Vec::with_capacity(args.roots.len());
        for (subject, post) in args.roots.iter().zip(&args.posts) {
            maps.push(RootMap { subject: real(subject)?, post: real(post)? });
        }
        Image::new(maps).map_err(|e| format!("image: {e}"))
    }

    /// The image of a job (`spec/exec.md` §8.4). A tree's roots must each
    /// be the real path of a directory, spelled on disk (the coordinator
    /// made them so; one that is not, now, is refused, a root in another
    /// case than the disk's as one through a link is: guard G1 compares
    /// real paths with them), then [`Image::new`]. An overlay is read from the store
    /// (`overlays/<id>.ovly`), decoded with every rule of `OVLY` and
    /// checked to have the content id `id` (`docs/wp/WP-074.md`): one that
    /// is not there or does not validate is a failure of the run, never a
    /// shard.
    pub fn from_post(store: &Store, post: &PostImage) -> Result<Image, String> {
        let maps = match post {
            PostImage::Tree(maps) => maps,
            PostImage::Overlay(id) => return read_overlay(store, id),
        };
        let maps: Vec<RootMap> = maps
            .iter()
            .map(|(subject, post)| RootMap { subject: subject.clone(), post: post.clone() })
            .collect();
        let image = Image::new(maps).map_err(|e| format!("image: {e}"))?;
        for root in image.maps.iter().flat_map(|m| [&m.subject, &m.post]) {
            match real_dir(&bytes_path(root)) {
                Ok(real) if real == *root => {}
                Ok(_) => {
                    return Err(format!(
                        "image: {} is not the real path of a directory",
                        String::from_utf8_lossy(root)
                    ));
                }
                Err(e) => return Err(format!("image: {}: {e}", String::from_utf8_lossy(root))),
            }
        }
        Ok(image)
    }

    /// What the coordinator puts in a job (`spec/exec.md` §5.2).
    pub fn to_post(&self) -> PostImage {
        if let Some(o) = &self.overlay {
            return PostImage::Overlay(o.id);
        }
        PostImage::Tree(self.maps.iter().map(|m| (m.subject.clone(), m.post.clone())).collect())
    }

    /// The root maps, sorted by `subject`.
    pub fn maps(&self) -> &[RootMap] {
        &self.maps
    }

    /// No root map and no overlay.
    pub fn is_identity(&self) -> bool {
        self.maps.is_empty() && self.overlay.is_none()
    }

    /// The post command of §5.1: [`Image::post_command`]'s command.
    pub fn command(&self, command: &Command) -> Command {
        self.post_command(command).command
    }

    /// The post command of §5.1, with its map pairs and guard G5's
    /// finding (§5.3). `argv[0]` and `env` are kept. Every path-bearing
    /// word (§5.1, "Path-bearing words") is resolved against the
    /// command's `directory`, by identity, and its leading part that
    /// reaches the subject is replaced by the same place in the post tree
    /// ([`Resolver::resolve_part`]); `directory` is resolved the same way.
    /// Every other word, and a path that cannot be resolved, takes the
    /// textual rule: each occurrence of a subject root followed by `/` or
    /// the end of the word replaced by its post root, from the left, not
    /// overlapping. Then one `-fmacro-prefix-map=<post prefix>=<prefix as
    /// written>` word per pair, the root maps' `<post>/=<subject>/`
    /// included, longest post prefix first. The identity's is the command
    /// itself.
    pub fn post_command(&self, command: &Command) -> PostCommand {
        if self.maps.is_empty() {
            return PostCommand { command: command.clone(), maps: Vec::new(), conflict: None };
        }
        let mut r = self.resolver();
        let root = Path::new("/");
        let directory = match r.resolve_part(&command.directory, PathKind::Dir, root, root) {
            Part::Rewrite { new, .. } => new,
            Part::Keep { .. } => command.directory.clone(),
            Part::Textual | Part::Undecided(_) => {
                match self.maps.iter().find(|m| at_or_under(&command.directory, &m.subject)) {
                    Some(m) => [&m.post[..], &command.directory[m.subject.len()..]].concat(),
                    None => command.directory.clone(),
                }
            }
        };
        let (from, to) = (bytes_path(&command.directory), bytes_path(&directory));
        // G5 (f): the directory, then each word's path with a `..`, walks
        // the post tree as it walks the subject.
        let differs = |what: &str, (path, ours, theirs): (PathBuf, Kind, Kind)| {
            format!(
                "post-image: {what} walks {}, which is {} in the subject and {} in the post tree",
                path.display(),
                ours.describe(),
                theirs.describe()
            )
        };
        let mut walked: Option<String> = r
            .walk_differs(root, &command.directory)
            .map(|d| differs(&format!("the directory {}", show_path(&command.directory)), d));
        let mut argv: Vec<Vec<u8>> = Vec::with_capacity(command.argv.len() + self.maps.len());
        let mut pairs: Vec<maps::Pair> = Vec::new();
        let mut next = Next::None;
        for (i, word) in command.argv.iter().enumerate() {
            if i == 0 {
                argv.push(word.clone());
                continue;
            }
            let Some((at, kind)) = path_part(word, &mut next).filter(|&(at, _)| at < word.len())
            else {
                argv.push(self.rewrite(word));
                continue;
            };
            let part = &word[at..];
            if walked.is_none() && part.split(|&b| b == b'/').any(|c| c == b"..") {
                walked = r.walk_differs(&from, part).map(|d| differs(&show_path(word), d));
            }
            match r.resolve_part(part, kind, &from, &to) {
                Part::Undecided(at) => {
                    walked.get_or_insert_with(|| {
                        format!(
                            "post-image: {} has a `..` after {}, which does not exist; slip \
                             cannot tell where it ends",
                            show_path(word),
                            show_path(&at)
                        )
                    });
                    argv.push(word.clone());
                }
                Part::Rewrite { new, map } => {
                    pairs.push(maps::Pair::new(map, word));
                    argv.push([&word[..at], &new[..]].concat());
                }
                Part::Keep { map } => {
                    pairs.push(maps::Pair::new(map, word));
                    argv.push(word.clone());
                }
                Part::Textual => argv.push(self.rewrite(word)),
            }
        }
        for m in &self.maps {
            let word = [b"-fmacro-prefix-map=", &m.post[..], b"/=", &m.subject[..], b"/"].concat();
            pairs.push(maps::Pair::new(
                ([&m.post[..], b"/"].concat(), [&m.subject[..], b"/"].concat()),
                &word,
            ));
        }
        // The pairs of the words; the root pairs are the roots themselves.
        let words = pairs.len() - self.maps.len();
        let conflict = maps::leaves(&pairs[..words], |post| r.in_post(post, &to))
            .or_else(|| maps::conflict(&pairs, &argv))
            .or(walked);
        let maps = maps::emitted(&pairs);
        for (post, written) in &maps {
            argv.push([b"-fmacro-prefix-map=", &post[..], b"=", &written[..]].concat());
        }
        PostCommand {
            command: Command { directory, argv, env: command.env.clone() },
            maps,
            conflict,
        }
    }

    /// Every occurrence of a subject root in `word` followed by `/` or the
    /// end of the word replaced by its post root.
    fn rewrite(&self, word: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(word.len());
        let mut at = 0;
        while at < word.len() {
            let rest = &word[at..];
            let hit = self.maps.iter().find(|m| {
                rest.strip_prefix(&m.subject[..])
                    .is_some_and(|after| after.is_empty() || after[0] == b'/')
            });
            if let Some(m) = hit {
                out.extend_from_slice(&m.post);
                at += m.subject.len();
            } else {
                out.push(word[at]);
                at += 1;
            }
        }
        out
    }

    /// The subject path of a real path (§5.1): `s/x` for `p/x` under the
    /// post root `p` of the subject root `s` (`s` for `p`), else the path.
    pub fn to_subject(&self, real: &[u8]) -> Vec<u8> {
        match self.maps.iter().find(|m| at_or_under(real, &m.post)) {
            Some(m) => [&m.subject[..], &real[m.post.len()..]].concat(),
            None => real.to_vec(),
        }
    }

    /// Guard G1's test (§5.3) for one path: [`Resolver::under_subject`].
    pub fn under_subject(&self, real: &[u8]) -> bool {
        self.resolver().under_subject(real)
    }

    /// A [`Resolver`] for this image's roots: each root's identity read
    /// now, and empty caches.
    pub fn resolver(&self) -> Resolver<'_> {
        let id = |root: &[u8]| {
            std::fs::metadata(bytes_path(root))
                .ok()
                .filter(std::fs::Metadata::is_dir)
                .map(|m| (m.dev(), m.ino()))
        };
        Resolver {
            maps: &self.maps,
            ids: self.maps.iter().map(|m| (id(&m.subject), id(&m.post))).collect(),
            regions: HashMap::new(),
            kinds: HashMap::new(),
            walks: HashMap::new(),
            canon: HashMap::new(),
        }
    }

    /// One parse of an observer's image with the walk's input. `Pre` is
    /// the observer's command and real paths as subject paths, `Post` the
    /// post command and [`Image::to_subject`]; the observer's id is kept
    /// on both sides. `f` runs on the front end's thread when the unit has
    /// no error, through `slip_digest::map::with_input`, whose `Err` is
    /// this function's. The shard is the parse task's; `f`'s result is
    /// returned only with a shard of status `ok` (otherwise it is
    /// dropped, as the map task drops the walk's).
    ///
    /// # Errors
    ///
    /// A failure of the worker: `slip_kiln::parse_observer_with`'s, or
    /// `with_input`'s.
    pub fn parse<R: Send>(
        &self,
        observer: &Observer,
        side: Side,
        phase: Phase,
        round: u64,
        f: impl FnOnce(&Input<'_>) -> R + Send,
    ) -> Result<(Shard, Option<R>), String> {
        let (shard, value) = self.parse_with(observer, side, phase, round, false, f)?;
        Ok((shard, value.map(|(value, _)| value)))
    }

    /// [`Image::parse`], with what header search looked at
    /// (`slip_kiln::Lookups`, for guard L1 of §5.3) beside `f`'s result.
    ///
    /// # Errors
    ///
    /// As [`Image::parse`].
    pub fn parse_looking<R: Send>(
        &self,
        observer: &Observer,
        side: Side,
        phase: Phase,
        round: u64,
        f: impl FnOnce(&Input<'_>) -> R + Send,
    ) -> Result<(Shard, Option<(R, Lookups)>), String> {
        let (shard, value) = self.parse_with(observer, side, phase, round, true, f)?;
        let value = match value {
            Some((value, Some(lookups))) => Some((value, lookups)),
            Some((_, None)) => return Err("the front end recorded no lookup log".to_owned()),
            None => None,
        };
        Ok((shard, value))
    }

    fn parse_with<R: Send>(
        &self,
        observer: &Observer,
        side: Side,
        phase: Phase,
        round: u64,
        lookups: bool,
        f: impl FnOnce(&Input<'_>) -> R + Send,
    ) -> Result<Parsed<R>, String> {
        if side == Side::Post && self.overlay.is_some() {
            // The post-image of the overlay form is the subject with the
            // overlay's bytes: parsed only through `parse_unit` with the
            // blobs given (`crate::worker::post::Loaded`), never from disk.
            return Err("the post-image of an overlay is parsed with its blobs".to_owned());
        }
        let mut options = ParseOptions::default();
        options.lookups = lookups;
        let (shard, walked) =
            self.parse_unit(observer, side, phase, round, options, |unit, subject| {
                (unit.errors() == 0)
                    .then(|| (with_input(unit, subject, f), unit.lookups().cloned()))
            })?;
        let value = match walked.flatten() {
            Some((Ok(value), looked)) => Some((value, looked)),
            Some((Err(e), _)) => return Err(e),
            None => None,
        };
        let value = value.filter(|_| shard.outcome.status == Status::Ok);
        Ok((shard, value))
    }

    /// One parse of an observer's image with `options`, `f` called on the
    /// finished unit (with errors or without) with the subject path of a
    /// real path for `side` (as [`Image::parse`]). The shard is the parse
    /// task's, and `f`'s result is returned whatever its status; `None`
    /// when no parse happened. The verify job of an overlay image parses
    /// its post-image here, with the overlay's blobs in `options`.
    ///
    /// # Errors
    ///
    /// `slip_kiln::parse_observer_with`'s.
    pub(crate) fn parse_unit<R: Send>(
        &self,
        observer: &Observer,
        side: Side,
        phase: Phase,
        round: u64,
        options: ParseOptions<'_>,
        f: impl FnOnce(&Unit<'_>, &dyn Fn(&[u8]) -> Vec<u8>) -> R + Send,
    ) -> Result<(Shard, Option<R>), String> {
        let post;
        let parsed = match side {
            Side::Pre => observer,
            Side::Post => {
                post = Observer { command: self.command(&observer.command), ..observer.clone() };
                &post
            }
        };
        let subject = |real: &[u8]| match side {
            Side::Pre => real.to_vec(),
            Side::Post => self.to_subject(real),
        };
        parse_observer_with(parsed, phase, round, options, |unit| f(unit, &subject))
    }
}

/// [`Image::from_post`] of `PostImage::Overlay(id)`.
fn read_overlay(store: &Store, id: &Digest) -> Result<Image, String> {
    let path = store.object_path(Object::Overlay, id);
    let bytes = store
        .read_object(Object::Overlay, id)
        .map_err(|e| format!("cannot read the overlay {id}: {e}"))?
        .ok_or_else(|| format!("the overlay {id} is not in the store ({})", path.display()))?;
    let overlay = Overlay::from_bytes(&bytes)
        .map_err(|e| format!("{} is not a valid OVLY file: {e}", path.display()))?;
    match content_id(&bytes) {
        Ok(actual) if actual == *id => Ok(Image::overlay(*id, Arc::new(overlay))),
        Ok(actual) => Err(format!("{} has the content id {actual}, not {id}", path.display())),
        Err(e) => Err(format!("{} is not a valid OVLY file: {e}", path.display())),
    }
}

/// A parse's shard, and `f`'s result with the lookups when they were asked
/// for.
type Parsed<R> = (Shard, Option<(R, Option<Lookups>)>);

/// A directory's device and inode, when it can be read.
type Identity = Option<(u64, u64)>;

/// How a path-bearing word's path is to be rewritten
/// ([`Resolver::resolve_part`]). `map` is the pair `(post prefix, prefix as
/// written)` that maps the spellings the front end makes under the new
/// path back to those it makes under the written one.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Part {
    /// To `new`.
    Rewrite { new: Vec<u8>, map: (Vec<u8>, Vec<u8>) },
    /// As written: it names the same place from the post command (`map`
    /// is the identity).
    Keep { map: (Vec<u8>, Vec<u8>) },
    /// By the textual rule.
    Textual,
    /// Kept as written, and refused (G5 (f)): a `..` follows the path
    /// given, which does not exist.
    Undecided(Vec<u8>),
}

/// The map pair of a path whose leading part `old` became `new` (§5.1):
/// with a `/` after each, unless the leading part is the whole path and
/// names a file (an empty leading part stays empty: the spellings under it
/// are relative).
fn map_of(new: &[u8], old: &[u8], whole: bool, kind: PathKind) -> (Vec<u8>, Vec<u8>) {
    let slash = |h: &[u8]| {
        if h.is_empty() || h.ends_with(b"/") { h.to_vec() } else { [h, b"/"].concat() }
    };
    if whole && kind == PathKind::File {
        (new.to_vec(), old.to_vec())
    } else {
        (slash(new), slash(old))
    }
}

/// The offset after each component of `path` (a run of bytes other than
/// `/`), in order.
fn component_ends(path: &[u8]) -> Vec<usize> {
    let mut ends = Vec::new();
    let mut i = 0;
    while i < path.len() {
        if path[i] == b'/' {
            i += 1;
            continue;
        }
        while i < path.len() && path[i] != b'/' {
            i += 1;
        }
        ends.push(i);
    }
    ends
}

/// The relative path from the directory `from` to `to`, both absolute and
/// with no `.`, `..` or symbolic link (`..` for each component of `from`
/// past the common ones); empty when they are equal.
fn relative_to(from: &Path, to: &Path) -> PathBuf {
    let a: Vec<_> = from.components().collect();
    let b: Vec<_> = to.components().collect();
    let common = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
    let mut out = PathBuf::new();
    for _ in common..a.len() {
        out.push("..");
    }
    for c in &b[common..] {
        out.push(c);
    }
    out
}

/// What a path-bearing word names (§5.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PathKind {
    Dir,
    File,
}

/// What the next word of the command is, after an option word.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Next {
    None,
    /// The value of a path option.
    Path(PathKind),
    /// The value of another option, which is not a path.
    Skip,
}

/// Options whose value is a path, joined (`-Ifoo`) or separate (`-I
/// foo`), longest first where one is a prefix of another (Kiln's
/// `JOINED_OR_SEPARATE`, `kiln_driver/src/args.rs:1290`, the path ones).
const JOINED_OR_SEPARATE_PATHS: &[(&[u8], PathKind)] = &[
    (b"-cxx-isystem", PathKind::Dir),
    (b"-idirafter", PathKind::Dir),
    (b"-isysroot", PathKind::Dir),
    (b"-isystem", PathKind::Dir),
    (b"-include", PathKind::File),
    (b"-iquote", PathKind::Dir),
    (b"-B", PathKind::Dir),
    (b"-F", PathKind::Dir),
    (b"-I", PathKind::Dir),
];

/// Path options with a separate value or an `=` (Kiln's `SEPARATE_OR_EQ`).
const EQ_OR_SEPARATE_PATHS: &[(&[u8], PathKind)] =
    &[(b"--sysroot", PathKind::Dir), (b"-resource-dir", PathKind::Dir)];

/// Path options joined by `=` only (Kiln `args.rs:2176`, `:2271`).
const EQ_PATHS: &[(&[u8], PathKind)] =
    &[(b"--embed-dir=", PathKind::Dir), (b"--gcc-toolchain=", PathKind::Dir)];

/// Options with a separate value that is not a path (Kiln's
/// `JOINED_OR_SEPARATE`, `SEPARATE_OR_EQ` and `SEPARATE`, the others, and
/// the pass-through options): the value is not an input.
const SEPARATE_OTHER: &[&[u8]] = &[
    b"-D",
    b"-U",
    b"-MF",
    b"-MJ",
    b"-MQ",
    b"-MT",
    b"-L",
    b"-l",
    b"-o",
    b"-x",
    b"-target-feature",
    b"-target",
    b"--target",
    b"-std",
    b"--std",
    b"-dumpdir",
    b"-ftrapv-handler",
    b"-Xclang",
    b"-mllvm",
    b"-Xlinker",
    b"-Xpreprocessor",
    b"-Xassembler",
    b"-arch",
];

/// The path in `word`, as the offset where it starts and what it names:
/// the value of a path option, or an input (a word that does not start
/// with `-` or `@` and is no option's value). `next` carries an option
/// whose value is the next word.
fn path_part(word: &[u8], next: &mut Next) -> Option<(usize, PathKind)> {
    match std::mem::replace(next, Next::None) {
        Next::Path(kind) => return Some((0, kind)),
        Next::Skip => return None,
        Next::None => {}
    }
    match word.first() {
        None | Some(b'@') => return None,
        Some(b'-') => {}
        Some(_) => return Some((0, PathKind::File)),
    }
    if word == b"-include-pch" {
        *next = Next::Path(PathKind::File);
        return None;
    }
    for &(opt, kind) in EQ_OR_SEPARATE_PATHS {
        if word == opt {
            *next = Next::Path(kind);
            return None;
        }
        if word.strip_prefix(opt).is_some_and(|rest| rest.first() == Some(&b'=')) {
            return Some((opt.len() + 1, kind));
        }
    }
    for &(opt, kind) in EQ_PATHS {
        if word.starts_with(opt) {
            return Some((opt.len(), kind));
        }
    }
    for &(opt, kind) in JOINED_OR_SEPARATE_PATHS {
        if word == opt {
            *next = Next::Path(kind);
            return None;
        }
        if word.starts_with(opt) {
            return Some((opt.len(), kind));
        }
    }
    if SEPARATE_OTHER.contains(&word) {
        *next = Next::Skip;
    }
    None
}

/// Where a directory is, by identity (§5.1): its nearest ancestor (itself
/// included) that is a root of the image, a post root or a subject root,
/// with the number of components of that root; or neither.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Region {
    Subject { map: usize, depth: usize },
    Post { map: usize, depth: usize },
    Neither,
}

/// What `lstat` says of a path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Missing,
    Dir,
    File,
    Link(PathBuf),
}

impl Kind {
    /// In a reason: "a directory", "a file", "missing", "a link to <t>".
    fn describe(&self) -> String {
        match self {
            Kind::Missing => "missing".to_owned(),
            Kind::Dir => "a directory".to_owned(),
            Kind::File => "a file".to_owned(),
            Kind::Link(t) => format!("a link to {}", t.display()),
        }
    }
}

/// Symbolic links a walk follows before it stops (Linux's `MAXSYMLINKS`).
const MAX_LINKS: u32 = 40;

/// What [`Resolver::walk`] saw.
#[derive(Debug, Default)]
struct Walked {
    /// The first directory of the subject the walk looked up a name in.
    subject: Option<PathBuf>,
    /// In rewrite mode: the place in the post tree, when the walk entered
    /// the subject with no `..` left.
    entered: Option<PathBuf>,
    /// A symbolic link was followed in a directory of the subject.
    link_in_subject: bool,
    /// Where the walk stopped, and the components it did not resolve.
    end: PathBuf,
    tail: Vec<std::ffi::OsString>,
}

/// What [`Resolver::route`] saw.
#[derive(Debug, Default)]
struct Route {
    /// The first directory under a post root the walk was in, its region
    /// `(map, depth)`, and the components still to walk from it.
    entry: Option<(PathBuf, usize, usize, Vec<std::ffi::OsString>)>,
    /// After `entry`, the walk was in a directory under no post root.
    left: bool,
    /// The walk stopped at more than [`MAX_LINKS`] symbolic links.
    looped: bool,
    /// The last directory reached, or the file the walk ended at; and the
    /// components it did not resolve (from a missing one, or one under a
    /// file).
    end: PathBuf,
    tail: Vec<std::ffi::OsString>,
}

/// "Under a root" by identity (§5.1), with caches: one per job's checks.
///
/// A directory is under a root when it or one of its ancestors is the
/// root's directory, compared by device and inode, the nearest root
/// winning (so a post root under a subject root is not under the
/// subject). A path is walked component by component, as the kernel
/// resolves it (symbolic links followed): every directory a name is looked
/// up in is visited, and a walk stops at the first component that does
/// not exist, since what does not exist cannot change where it leads.
/// Only directories are compared, never a file's own inode: a post tree
/// made of hard links is legitimate.
#[derive(Debug)]
pub struct Resolver<'i> {
    maps: &'i [RootMap],
    /// Each map's subject and post root identity; `None` when it cannot be
    /// read (the root then falls back to the textual rule and bytes).
    ids: Vec<(Identity, Identity)>,
    regions: HashMap<PathBuf, Region>,
    kinds: HashMap<PathBuf, Kind>,
    walks: HashMap<PathBuf, Option<PathBuf>>,
    canon: HashMap<PathBuf, Option<PathBuf>>,
}

impl Resolver<'_> {
    /// The region of the directory `dir` (absolute, no symbolic link,
    /// `.` or `..` in it).
    fn region(&mut self, dir: &Path) -> Region {
        if let Some(&r) = self.regions.get(dir) {
            return r;
        }
        let id = std::fs::metadata(dir).ok().filter(std::fs::Metadata::is_dir);
        let id = id.map(|m| (m.dev(), m.ino()));
        let depth = dir.components().count().saturating_sub(1);
        let r = if let Some(map) =
            id.and_then(|id| self.ids.iter().position(|ids| ids.1 == Some(id)))
        {
            Region::Post { map, depth }
        } else if let Some(map) =
            id.and_then(|id| self.ids.iter().position(|ids| ids.0 == Some(id)))
        {
            Region::Subject { map, depth }
        } else {
            match dir.parent() {
                Some(parent) => self.region(parent),
                None => Region::Neither,
            }
        };
        self.regions.insert(dir.to_path_buf(), r);
        r
    }

    fn kind(&mut self, path: &Path) -> Kind {
        if let Some(k) = self.kinds.get(path) {
            return k.clone();
        }
        let k = match std::fs::symlink_metadata(path) {
            Ok(m) if m.file_type().is_symlink() => {
                std::fs::read_link(path).map_or(Kind::Missing, Kind::Link)
            }
            Ok(m) if m.is_dir() => Kind::Dir,
            Ok(_) => Kind::File,
            Err(_) => Kind::Missing,
        };
        self.kinds.insert(path.to_path_buf(), k.clone());
        k
    }

    /// `dir` is a post root.
    fn is_post_root(&mut self, dir: &Path) -> bool {
        let depth = dir.components().count().saturating_sub(1);
        matches!(self.region(dir), Region::Post { depth: d, .. } if d == depth)
    }

    /// The walk of the absolute path `start` (see [`Resolver`]). Stops at
    /// the first directory of the subject it looks a name up in, unless
    /// `rewrite`; then it also stops where it enters the subject with no
    /// `..` left, giving the same place in the post tree.
    fn walk(&mut self, start: &Path, rewrite: bool) -> Walked {
        let mut todo: VecDeque<std::ffi::OsString> = VecDeque::new();
        components(start, &mut todo);
        let mut cur = PathBuf::from("/");
        let mut out = Walked::default();
        let mut links = 0;
        loop {
            let region = self.region(&cur);
            let in_subject = matches!(region, Region::Subject { .. });
            let Some(c) = todo.pop_front() else {
                if in_subject {
                    out.subject.get_or_insert_with(|| cur.clone());
                }
                break;
            };
            if c == "." {
                continue;
            }
            if c == ".." {
                if in_subject {
                    out.subject.get_or_insert_with(|| cur.clone());
                    if !rewrite {
                        break;
                    }
                }
                cur.pop();
                continue;
            }
            let next = cur.join(&c);
            let kind = self.kind(&next);
            // Looking up a post root's name in its subject is how the post
            // tree is reached, not a look at the subject.
            if in_subject && !(kind == Kind::Dir && self.is_post_root(&next)) {
                if let (true, None, Region::Subject { map, depth }) =
                    (rewrite, &out.subject, region)
                    && !todo.iter().any(|t| t == "..")
                {
                    let mut target = bytes_path(&self.maps[map].post);
                    target.extend(cur.components().skip(1 + depth));
                    target.push(&c);
                    target.extend(todo.iter());
                    out.entered = Some(target);
                    return out;
                }
                out.subject.get_or_insert_with(|| cur.clone());
                if !rewrite {
                    break;
                }
            }
            match kind {
                Kind::Dir => cur = next,
                Kind::Link(target) if links < MAX_LINKS => {
                    links += 1;
                    out.link_in_subject |= in_subject;
                    if target.is_absolute() {
                        cur = PathBuf::from("/");
                    }
                    let mut front = VecDeque::new();
                    components(&target, &mut front);
                    for t in front.into_iter().rev() {
                        todo.push_front(t);
                    }
                }
                _ => {
                    todo.push_front(c);
                    break;
                }
            }
        }
        out.end = cur;
        out.tail = todo.into();
        out
    }

    /// The first directory of the subject the walk of `start` looks a name
    /// up in, if any (cached).
    fn subject_dir(&mut self, start: &Path) -> Option<PathBuf> {
        if let Some(r) = self.walks.get(start) {
            return r.clone();
        }
        let r = self.walk(start, false).subject;
        self.walks.insert(start.to_path_buf(), r.clone());
        r
    }

    /// The real path of `path` (`slip_spelling::real_path`: spelled on
    /// disk; cached); `None` when it does not exist. A path that exists
    /// and has no spelling on disk (a directory that must be listed and
    /// cannot be) keeps `canonicalize`'s answer: the regions are decided
    /// by identity, and taking it for missing would end a walk of guard
    /// G5 early.
    fn canonical(&mut self, path: &Path) -> Option<PathBuf> {
        if let Some(r) = self.canon.get(path) {
            return r.clone();
        }
        let r = match slip_spelling::real_path(path) {
            Ok(r) => Some(r),
            Err(e) if e.is_not_found() => None,
            // canonicalize: fallback on unlistable dir; regions by identity (WP-151 decision 2)
            Err(_) => std::fs::canonicalize(path).ok(),
        };
        self.canon.insert(path.to_path_buf(), r.clone());
        r
    }

    /// Whether the post prefix `post` of a map pair (relative to `to`, the
    /// post command's directory, when relative) lies under a post root
    /// (§5.1, the invariant G5 checks), by real paths: walked from the real
    /// path of `to` (`/` when absolute), each component resolved before
    /// the next, a `..` taken from a real path only. Where a component
    /// does not exist, the deepest existing ancestor decides, and only if
    /// no `..` follows; anything else is not under a post root.
    fn in_post(&mut self, post: &[u8], to: &Path) -> bool {
        let mut cur = if post.starts_with(b"/") {
            PathBuf::from("/")
        } else {
            match self.canonical(to) {
                Some(real) => real,
                None => return false,
            }
        };
        let parts: Vec<&[u8]> = post.split(|&b| b == b'/').collect();
        for (i, c) in parts.iter().enumerate() {
            match *c {
                b"" | b"." => {}
                b".." => {
                    cur.pop();
                }
                name => match self.canonical(&cur.join(bytes_path(name))) {
                    Some(real) => cur = real,
                    None if parts[i..].contains(&&b".."[..]) => return false,
                    None => break,
                },
            }
        }
        matches!(self.place(&cur), Region::Post { .. })
    }

    /// Guard G5 (f) for the path `part` walked from the directory `from`
    /// (`/` for an absolute part): the first path whose kind differs
    /// between the subject and the post tree (with both kinds), among the
    /// names the walk looks up in a directory of the subject. The walk is
    /// the kernel's, by real paths. Before the last `..` every difference
    /// counts: the `..` of the real post-image is taken from where the post
    /// tree's entry leads. After it, only a link in the subject counts: the
    /// rewrite writes the place the subject's walk reached, with its links
    /// resolved, and the post parse walks the rest of a path as written as
    /// the real post-image does. A post root looked up in its subject is
    /// skipped.
    fn walk_differs(&mut self, from: &Path, part: &[u8]) -> Option<(PathBuf, Kind, Kind)> {
        let mut cur =
            if part.starts_with(b"/") { PathBuf::from("/") } else { self.canonical(from)? };
        let parts: Vec<&[u8]> = part.split(|&b| b == b'/').collect();
        let last_up = parts.iter().rposition(|c| *c == b"..").unwrap_or(0);
        for (i, c) in parts.into_iter().enumerate() {
            match c {
                b"" | b"." => continue,
                b".." => {
                    cur.pop();
                    continue;
                }
                _ => {}
            }
            let next = cur.join(bytes_path(c));
            if let Region::Subject { map, depth } = self.region(&cur) {
                let ours = self.kind(&next);
                let counts = i < last_up || matches!(ours, Kind::Link(_));
                if counts && !(ours == Kind::Dir && self.is_post_root(&next)) {
                    let theirs = self.kind(&self.counterpart(&cur, map, depth).join(bytes_path(c)));
                    if ours != theirs {
                        return Some((next, ours, theirs));
                    }
                }
            }
            cur = self.canonical(&next)?;
        }
        None
    }

    /// The region of the real path `real`: its own for a directory, its
    /// directory's for anything else.
    fn place(&mut self, real: &Path) -> Region {
        if self.kind(real) == Kind::Dir {
            self.region(real)
        } else {
            self.region(&file_dir(&path_bytes(real)))
        }
    }

    /// The same place as the real path `real`, of the subject's region
    /// `(map, depth)`, in the post tree.
    fn counterpart(&self, real: &Path, map: usize, depth: usize) -> PathBuf {
        let mut target = bytes_path(&self.maps[map].post);
        target.extend(real.components().skip(1 + depth));
        target
    }

    /// `target` as the post command writes it: relative to `to` (the post
    /// command's `directory`, by its real path) when `relative`, else
    /// absolute (and absolute when `to` cannot be resolved).
    fn written(&mut self, target: &Path, relative: bool, to: &Path) -> Vec<u8> {
        if relative && let Some(real_to) = self.canonical(to) {
            return path_bytes(&relative_to(&real_to, target));
        }
        path_bytes(target)
    }

    /// How the path `part` of a word of the post command is written
    /// (§5.1). Its leading parts are resolved against `from` (the
    /// command's `directory`; `/` for an absolute part), shortest first,
    /// by real path: the empty one first for a relative part, then each
    /// one ending at a component. The first that is in the subject (by
    /// identity, [`Resolver::place`]) is replaced by the same place in the
    /// post tree, written relative to `to` (the post command's
    /// `directory`) when `part` is relative, absolute when it is
    /// absolute; the rest of `part` is kept byte for byte, so that two
    /// spellings of one directory stay two spellings ([`Part::Keep`] when
    /// that gives `part`). A rest with a `..` component is resolved with
    /// the leading part instead ([`Resolver::leaving`]). A part none of
    /// whose leading parts is in the subject, or that cannot be resolved
    /// (`from` not absolute, a component missing first, a leading part in
    /// a post tree), takes the textual rule.
    fn resolve_part(&mut self, part: &[u8], kind: PathKind, from: &Path, to: &Path) -> Part {
        let relative = !part.starts_with(b"/");
        if part.is_empty() || (relative && !from.is_absolute()) {
            return Part::Textual;
        }
        let base = if relative { from.to_path_buf() } else { PathBuf::from("/") };
        let ends = component_ends(part);
        let heads = relative.then_some(0).into_iter().chain(ends.iter().copied());
        for end in heads {
            let head = &part[..end];
            let Some(real) = self.canonical(&base.join(bytes_path(head))) else {
                return Part::Textual;
            };
            let (map, depth) = match self.place(&real) {
                Region::Neither => continue,
                Region::Post { .. } => return Part::Textual,
                Region::Subject { map, depth } => (map, depth),
            };
            let rest = &part[end..];
            if rest.split(|&b| b == b'/').any(|c| c == b"..") {
                return self.leaving(part, kind, &base, relative, to);
            }
            let target = self.counterpart(&real, map, depth);
            let mut new_head = self.written(&target, relative, to);
            if new_head.is_empty() && end > 0 {
                new_head = b".".to_vec();
            }
            let new = if end == 0 && !new_head.is_empty() {
                [&new_head[..], b"/", rest].concat()
            } else {
                [&new_head[..], rest].concat()
            };
            if new == part {
                return Part::Keep { map: map_of(part, part, true, kind) };
            }
            return Part::Rewrite { map: map_of(&new_head, head, rest.is_empty(), kind), new };
        }
        Part::Textual
    }

    /// [`Resolver::resolve_part`] of a `part` that reaches the subject and
    /// has a `..` after: walked as a whole ([`Resolver::walk`]) to where it
    /// ends, in the post tree when that is in the subject, and written
    /// whole (relative to `to` when `part` is relative). A part that
    /// followed a symbolic link in the subject is written through the
    /// subject, which guard L1 then refuses if the front end looks there.
    fn leaving(
        &mut self,
        part: &[u8],
        kind: PathKind,
        base: &Path,
        relative: bool,
        to: &Path,
    ) -> Part {
        let start = base.join(bytes_path(part));
        let walked = self.walk(&start, true);
        let target = if let Some(target) = walked.entered {
            target
        } else if walked.link_in_subject {
            if relative {
                let new = path_bytes(&start);
                return Part::Rewrite { map: map_of(&new, part, true, kind), new };
            }
            return Part::Keep { map: map_of(part, part, true, kind) };
        } else {
            let mut end = walked.end;
            // A `..` after a component that does not exist (or is a file)
            // is not folded: where it ends cannot be decided.
            if walked.tail.iter().any(|c| c == "..") {
                return Part::Undecided(path_bytes(&end.join(&walked.tail[0])));
            }
            let tail: Vec<std::ffi::OsString> =
                walked.tail.into_iter().filter(|c| c != ".").collect();
            if let Region::Subject { map, depth } = self.region(&end) {
                end = self.counterpart(&end, map, depth);
            }
            end.extend(tail);
            end
        };
        let mut new = self.written(&target, relative, to);
        if new.is_empty() {
            new = b".".to_vec();
        }
        if new == part {
            return Part::Keep { map: map_of(part, part, true, kind) };
        }
        Part::Rewrite { map: map_of(&new, part, true, kind), new }
    }

    /// Guard G1's test (§5.3) for the real path of a file read: its
    /// directory is under a subject root and under no post root, by
    /// identity. A root whose identity cannot be read is compared as bytes.
    pub fn under_subject(&mut self, real: &[u8]) -> bool {
        match self.region(file_dir(real).as_path()) {
            Region::Subject { .. } => true,
            Region::Post { .. } => false,
            Region::Neither => {
                self.maps
                    .iter()
                    .zip(&self.ids)
                    .any(|(m, ids)| ids.0.is_none() && at_or_under(real, &m.subject))
                    && !self.maps.iter().any(|m| at_or_under(real, &m.post))
            }
        }
    }

    /// The post tree's counterpart of the real path `real` of a file whose
    /// directory is under a subject root (by identity, as
    /// [`Resolver::under_subject`] decides it): `p/x` for `s/x`. `None`
    /// for a file of no subject root's region.
    pub fn post_counterpart(&mut self, real: &[u8]) -> Option<Vec<u8>> {
        let dir = file_dir(real);
        let Region::Subject { map, depth } = self.region(&dir) else {
            return None;
        };
        let mut target = self.counterpart(&dir, map, depth);
        target.push(bytes_path(real).file_name()?);
        Some(path_bytes(&target))
    }

    /// The real path of a file read is under a post root, by identity
    /// (bytes for a root whose identity cannot be read).
    pub fn under_post(&mut self, real: &[u8]) -> bool {
        match self.region(file_dir(real).as_path()) {
            Region::Post { .. } => true,
            Region::Subject { .. } => false,
            Region::Neither => self
                .maps
                .iter()
                .zip(&self.ids)
                .any(|(m, ids)| ids.1.is_none() && at_or_under(real, &m.post)),
        }
    }

    /// Guard G6 (§5.3), its first part: the reason when the walk of a
    /// file the post-image parse read, as the front end spelled it
    /// (`lookups.read`, from `directory`, the post command's), follows a
    /// symbolic link at a place under a post root (`p/D/n`) that is not, at
    /// `s/D/n` in the subject, a symbolic link with the same target bytes;
    /// `None` when every such link is. A link with an absolute target
    /// differs as text between a tree and its copy and is refused (§5.3).
    pub fn link_differs(&mut self, directory: &[u8], lookups: &Lookups) -> Option<String> {
        let base = bytes_path(directory);
        for name in &lookups.read {
            let mut todo = VecDeque::new();
            components(&base.join(name), &mut todo);
            let mut cur = PathBuf::from("/");
            let mut links = 0;
            while let Some(c) = todo.pop_front() {
                if c == "." {
                    continue;
                }
                if c == ".." {
                    cur.pop();
                    continue;
                }
                let next = cur.join(&c);
                match self.kind(&next) {
                    Kind::Dir => cur = next,
                    Kind::Link(target) if links < MAX_LINKS => {
                        links += 1;
                        if let Region::Post { map, depth } = self.region(&cur) {
                            let there = self.subject_counterpart(&next, map, depth);
                            let theirs = self.kind(&there);
                            if theirs != Kind::Link(target.clone()) {
                                return Some(format!(
                                    "post-image: {name} follows the link {}, which is {} in the \
                                     subject",
                                    show_path(&path_bytes(&next)),
                                    theirs.describe()
                                ));
                            }
                        }
                        if target.is_absolute() {
                            cur = PathBuf::from("/");
                        }
                        let mut front = VecDeque::new();
                        components(&target, &mut front);
                        for t in front.into_iter().rev() {
                            todo.push_front(t);
                        }
                    }
                    _ => break,
                }
            }
        }
        None
    }

    /// Guard G6 (§5.3), its second part: for the real path `p/Y` of a file
    /// read under a post root, the reason when its subject path `s/Y`,
    /// resolved in the subject component by component up to the first
    /// component that does not exist, has a symbolic link among its
    /// existing components; `None` for a file under no post root.
    pub fn subject_route_link(&mut self, real: &[u8]) -> Option<String> {
        let Region::Post { map, depth } = self.region(&file_dir(real)) else {
            return None;
        };
        let real_path = bytes_path(real);
        let mut cur = bytes_path(&self.maps[map].subject);
        let subject_path = self.subject_counterpart(&real_path, map, depth);
        for c in real_path.components().skip(1 + depth) {
            let next = cur.join(c);
            match self.kind(&next) {
                Kind::Dir => cur = next,
                Kind::Link(_) => {
                    return Some(format!(
                        "post-image: {} stands for {}, which the subject reaches through the \
                         link {}",
                        show_path(real),
                        show_path(&path_bytes(&subject_path)),
                        show_path(&path_bytes(&next))
                    ));
                }
                Kind::File | Kind::Missing => break,
            }
        }
        None
    }

    /// The same place as the real path `real`, of the post region
    /// `(map, depth)`, in the subject.
    fn subject_counterpart(&self, real: &Path, map: usize, depth: usize) -> PathBuf {
        let mut target = bytes_path(&self.maps[map].subject);
        target.extend(real.components().skip(1 + depth));
        target
    }

    /// The walk of the absolute path `start` as the kernel resolves it,
    /// component by component, symbolic links followed (see [`Route`]).
    fn route(&mut self, start: &Path) -> Route {
        let mut todo: VecDeque<std::ffi::OsString> = VecDeque::new();
        components(start, &mut todo);
        let mut cur = PathBuf::from("/");
        let mut out = Route::default();
        let mut links = 0;
        loop {
            match self.region(&cur) {
                Region::Post { map, depth } => {
                    if out.entry.is_none() {
                        out.entry = Some((cur.clone(), map, depth, todo.iter().cloned().collect()));
                    }
                }
                Region::Subject { .. } | Region::Neither => out.left |= out.entry.is_some(),
            }
            let Some(c) = todo.pop_front() else {
                break;
            };
            if c == "." {
                continue;
            }
            if c == ".." {
                cur.pop();
                continue;
            }
            let next = cur.join(&c);
            match self.kind(&next) {
                Kind::Dir => cur = next,
                Kind::File if todo.iter().all(|t| t == ".") => {
                    todo.clear();
                    cur = next;
                    break;
                }
                Kind::Link(target) => {
                    if links == MAX_LINKS {
                        out.looped = true;
                        todo.push_front(c);
                        break;
                    }
                    links += 1;
                    if target.is_absolute() {
                        cur = PathBuf::from("/");
                    }
                    let mut front = VecDeque::new();
                    components(&target, &mut front);
                    for t in front.into_iter().rev() {
                        todo.push_front(t);
                    }
                }
                Kind::File | Kind::Missing => {
                    todo.push_front(c);
                    break;
                }
            }
        }
        out.end = cur;
        out.tail = todo.into();
        out
    }

    /// The place a walk ended at: its end with the components it did not
    /// resolve, the end taken to the subject first when `to_subject` and
    /// it is under a post root.
    fn ended_at(&mut self, end: &Path, tail: &[std::ffi::OsString], to_subject: bool) -> PathBuf {
        let mut place = end.to_path_buf();
        if to_subject && let Region::Post { map, depth } = self.place(end) {
            place = self.subject_counterpart(end, map, depth);
        }
        place.extend(tail);
        place
    }

    /// Guard G6 (§5.3), its third part: the reason when a place the
    /// post-image parse looked at (`lookups`, as spelled from `directory`,
    /// the post command's: each directory of the search list, each
    /// location tried, each file read, in this order), walked in the file
    /// system, is under a post root and then under none (it leaves the
    /// post root), and the same route walked from the subject (from the
    /// subject's counterpart of the first directory under a post root the
    /// walk was in, with the components left there) ends elsewhere. The
    /// two ends are compared as real paths with the components the walk
    /// could not resolve (a missing component, a file), the post end taken
    /// to the subject when it is under a post root. A walk that follows
    /// more than [`MAX_LINKS`] links cannot be decided and is refused.
    pub fn leaves_elsewhere(&mut self, directory: &[u8], lookups: &Lookups) -> Option<String> {
        let base = bytes_path(directory);
        let routes = lookups
            .list
            .iter()
            .map(|d| (base.join(d), format!("searches {d}")))
            .chain(
                lookups
                    .tried
                    .iter()
                    .map(|(d, n)| (base.join(d).join(n), format!("looks for {n} in {d}"))),
            )
            .chain(lookups.read.iter().map(|n| (base.join(n), format!("reads {n}"))));
        for (start, what) in routes {
            let here = self.route(&start);
            let Some((dir, map, depth, rest)) = &here.entry else {
                continue;
            };
            if !here.left && !here.looped {
                continue;
            }
            let mut from = self.subject_counterpart(dir, *map, *depth);
            from.extend(rest);
            let there = self.route(&from);
            if here.looped || there.looped {
                return Some(format!(
                    "post-image: {what}, which leaves the post root: more than {MAX_LINKS} \
                     symbolic links {}",
                    if here.looped { "here" } else { "from the subject" }
                ));
            }
            let ours = self.ended_at(&here.end, &here.tail, true);
            let theirs = self.ended_at(&there.end, &there.tail, false);
            if ours != theirs {
                let shown = self.ended_at(&here.end, &here.tail, false);
                return Some(format!(
                    "post-image: {what}, which leaves the post root: {} here, {} from the subject",
                    show_path(&path_bytes(&shown)),
                    show_path(&path_bytes(&theirs))
                ));
            }
        }
        None
    }

    /// Guard L1 (§5.3): the reason when a place `lookups` names, walked
    /// from `directory` (the post command's), looks a name up in a
    /// directory of the subject; `None` when none does. In this order:
    /// each directory of the search list; each location tried, by the
    /// directory that would hold it; each file read, by its directory as
    /// spelled.
    pub fn looked_in_subject(&mut self, directory: &[u8], lookups: &Lookups) -> Option<String> {
        let base = bytes_path(directory);
        let at = |p: &str| base.join(p);
        let show = |p: &Path| show_path(&path_bytes(p));
        for d in &lookups.list {
            if let Some(hit) = self.subject_dir(&at(d)) {
                return Some(format!(
                    "post-image: searches {d}: {} is a directory of the subject",
                    show(&hit)
                ));
            }
        }
        for (d, name) in &lookups.tried {
            let place = at(d).join(name);
            let holder = place.parent().map_or_else(|| PathBuf::from("/"), Path::to_path_buf);
            if let Some(hit) = self.subject_dir(&holder) {
                return Some(format!(
                    "post-image: looked for {name} in {d}: {} is a directory of the subject",
                    show(&hit)
                ));
            }
        }
        for name in &lookups.read {
            let place = at(name);
            let holder = place.parent().map_or_else(|| PathBuf::from("/"), Path::to_path_buf);
            if let Some(hit) = self.subject_dir(&holder) {
                return Some(format!(
                    "post-image: read {name}: {} is a directory of the subject",
                    show(&hit)
                ));
            }
        }
        None
    }
}

/// The components of `path` other than its root, as names (`.` and `..`
/// kept), appended to `out`.
fn components(path: &Path, out: &mut VecDeque<std::ffi::OsString>) {
    use std::path::Component;
    for c in path.components() {
        match c {
            Component::Normal(n) => out.push_back(n.to_owned()),
            Component::CurDir => out.push_back(".".into()),
            Component::ParentDir => out.push_back("..".into()),
            Component::RootDir | Component::Prefix(_) => {}
        }
    }
}

/// The directory of the file whose real path is `real`.
fn file_dir(real: &[u8]) -> PathBuf {
    let p = bytes_path(real);
    p.parent().map_or_else(|| PathBuf::from("/"), Path::to_path_buf)
}

/// A path in a reason: its bytes where they are UTF-8, each other byte as
/// `\xNN` (`spec/verify.md` §5.3).
pub fn show_path(path: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(path.len());
    for chunk in path.utf8_chunks() {
        out.push_str(chunk.valid());
        for b in chunk.invalid() {
            let _ = write!(out, "\\x{b:02x}");
        }
    }
    out
}
