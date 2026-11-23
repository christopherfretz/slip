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

use std::fmt;
use std::path::{Path, PathBuf};

use slip_digest::Input;
use slip_digest::map::with_input;
use slip_exec::store::Store;
use slip_exec::worker::PostImage;
use slip_kiln::{ParseOptions, parse_observer_with};
use slip_model::{Command, Observer, Phase, Shard, Status};

use crate::args::ImageArgs;

/// One root map: the files under `subject` are, in the post-image, the
/// files at the same place under `post` (§5.1). Both are real paths of
/// directories, as bytes.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RootMap {
    pub subject: Vec<u8>,
    pub post: Vec<u8>,
}

/// A post-image (§5.1), of the tree form: root maps sorted by `subject`,
/// none of whose roots equals or lies under another. No root map is the
/// identity. The overlay form is added by `docs/wp/WP-074.md`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Image {
    maps: Vec<RootMap>,
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

/// The real path of `given`, which must be a directory.
fn real_dir(given: &Path) -> Result<Vec<u8>, String> {
    let real = std::fs::canonicalize(given).map_err(|e| e.to_string())?;
    let meta = std::fs::metadata(&real).map_err(|e| e.to_string())?;
    if !meta.is_dir() {
        return Err("not a directory".to_owned());
    }
    Ok(path_bytes(&real))
}

impl Image {
    /// The identity: the post-image is the subject as it is on disk.
    pub fn identity() -> Image {
        Image { maps: Vec::new() }
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
        let roots: Vec<&[u8]> =
            maps.iter().flat_map(|m| [m.subject.as_slice(), m.post.as_slice()]).collect();
        for (i, a) in roots.iter().enumerate() {
            for b in &roots[i + 1..] {
                if a == b {
                    return Err(ImageError::Duplicate(a.to_vec()));
                }
                if at_or_under(a, b) {
                    return Err(ImageError::Nested { inner: a.to_vec(), outer: b.to_vec() });
                }
                if at_or_under(b, a) {
                    return Err(ImageError::Nested { inner: b.to_vec(), outer: a.to_vec() });
                }
            }
        }
        let mut maps = maps;
        maps.sort();
        Ok(Image { maps })
    }

    /// The image of `--root`/`--post` (`spec/exec.md` §8.7): each
    /// directory made a real path (`std::fs::canonicalize`; it must be a
    /// directory), then [`Image::new`]. `Err` is `image: <path as given>:
    /// <error>` or `image: <ImageError>`.
    pub fn from_args(args: &ImageArgs) -> Result<Image, String> {
        if args.roots.len() != args.posts.len() {
            return Err(format!(
                "image: {} --root and {} --post: they come in pairs",
                args.roots.len(),
                args.posts.len()
            ));
        }
        let real =
            |p: &PathBuf| real_dir(p).map_err(|e| format!("image: {}: {e}", p.display()));
        let mut maps = Vec::with_capacity(args.roots.len());
        for (subject, post) in args.roots.iter().zip(&args.posts) {
            maps.push(RootMap { subject: real(subject)?, post: real(post)? });
        }
        Image::new(maps).map_err(|e| format!("image: {e}"))
    }

    /// The image of a job (`spec/exec.md` §8.4). A tree's roots must each
    /// be the real path of a directory (the coordinator made them so;
    /// one that is not, now, is refused: guard G1 compares real paths
    /// with them), then [`Image::new`]. The overlay form is `Err` until
    /// `docs/wp/WP-074.md`.
    pub fn from_post(_store: &Store, post: &PostImage) -> Result<Image, String> {
        let maps = match post {
            PostImage::Tree(maps) => maps,
            PostImage::Overlay(_) => return Err("the overlay image needs WP-074".to_owned()),
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
        PostImage::Tree(self.maps.iter().map(|m| (m.subject.clone(), m.post.clone())).collect())
    }

    /// The root maps, sorted by `subject`.
    pub fn maps(&self) -> &[RootMap] {
        &self.maps
    }

    /// No root map.
    pub fn is_identity(&self) -> bool {
        self.maps.is_empty()
    }

    /// The post command of §5.1: `directory` rewritten if it is a subject
    /// root or lies under one; `argv[0]` kept; in every other word each
    /// occurrence of a subject root followed by `/` or the end of the word
    /// replaced by its post root, from the left, not overlapping; one
    /// `-fmacro-prefix-map=<post>/=<subject>/` per map after the last
    /// word; `env` kept. The identity's is the command itself.
    pub fn command(&self, command: &Command) -> Command {
        let directory = match self.maps.iter().find(|m| at_or_under(&command.directory, &m.subject))
        {
            Some(m) => [&m.post[..], &command.directory[m.subject.len()..]].concat(),
            None => command.directory.clone(),
        };
        let mut argv: Vec<Vec<u8>> = Vec::with_capacity(command.argv.len() + self.maps.len());
        for (i, word) in command.argv.iter().enumerate() {
            argv.push(if i == 0 { word.clone() } else { self.rewrite(word) });
        }
        for m in &self.maps {
            argv.push([b"-fmacro-prefix-map=", &m.post[..], b"/=", &m.subject[..], b"/"].concat());
        }
        Command { directory, argv, env: command.env.clone() }
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
            match hit {
                Some(m) => {
                    out.extend_from_slice(&m.post);
                    at += m.subject.len();
                }
                None => {
                    out.push(word[at]);
                    at += 1;
                }
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

    /// Guard G1's test (§5.3): `real` is a subject root or lies under one.
    pub fn under_subject(&self, real: &[u8]) -> bool {
        self.maps.iter().any(|m| at_or_under(real, &m.subject))
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
        let (shard, walked) =
            parse_observer_with(parsed, phase, round, ParseOptions::default(), |unit| {
                (unit.errors() == 0).then(|| with_input(unit, &subject, f))
            })?;
        let value = match walked.flatten() {
            Some(Ok(value)) => Some(value),
            Some(Err(e)) => return Err(e),
            None => None,
        };
        let value = value.filter(|_| shard.outcome.status == Status::Ok);
        Ok((shard, value))
    }
}
