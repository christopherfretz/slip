//! `spec/verify.md` §5.1: the post-image of the tree form. The roots,
//! the post command, subject paths, guard G1's test, and `__FILE__` in a
//! real parse of a post tree.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, PoisonError};

use slip_cli::args::ImageArgs;
use slip_cli::image::{Image, ImageError, RootMap, Side};
use slip_cli::worker::MapTask;
use slip_exec::store::{Access, Store};
use slip_exec::worker::{Job, PostImage, Task};
use slip_model::{Command, Digest, EnvVar, FileRef, Language, Observer, Origin, Phase, Status};

/// A parse changes the process's current directory.
static LOCK: Mutex<()> = Mutex::new(());

fn map(subject: &str, post: &str) -> RootMap {
    RootMap { subject: subject.as_bytes().to_vec(), post: post.as_bytes().to_vec() }
}

fn err(maps: Vec<RootMap>) -> (ImageError, String) {
    let e = Image::new(maps).unwrap_err();
    let shown = e.to_string();
    (e, shown)
}

fn words(list: &[&str]) -> Vec<Vec<u8>> {
    list.iter().map(|w| w.as_bytes().to_vec()).collect()
}

/// A directory of its own under the target directory, removed on drop.
struct Dir(PathBuf);

impl Dir {
    fn new(name: &str) -> Dir {
        static N: AtomicUsize = AtomicUsize::new(0);
        let n = N.fetch_add(1, Ordering::SeqCst);
        let base = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
            .join(format!("wp052-image-{name}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        Dir(std::fs::canonicalize(&base).unwrap())
    }

    fn mkdir(&self, rel: &str) -> PathBuf {
        let p = self.0.join(rel);
        std::fs::create_dir_all(&p).unwrap();
        p
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn bytes(p: &Path) -> Vec<u8> {
    p.to_str().unwrap().as_bytes().to_vec()
}

// §5.1: every refusal of a root, with its message.
#[test]
fn s5_1_refused_roots() {
    let cases: Vec<(Vec<RootMap>, ImageError, &str)> = vec![
        (vec![map("s", "/p")], ImageError::NotAbsolute(b"s".to_vec()), "s is not an absolute path"),
        (vec![map("/s", "p")], ImageError::NotAbsolute(b"p".to_vec()), "p is not an absolute path"),
        (vec![map("/", "/p")], ImageError::IsRoot, "/ cannot be a root of an image"),
        (vec![map("/s/", "/p")], ImageError::NotNormal(b"/s/".to_vec()), "/s/ is not a real path"),
        (
            vec![map("/s//t", "/p")],
            ImageError::NotNormal(b"/s//t".to_vec()),
            "/s//t is not a real path",
        ),
        (
            vec![map("/s/./t", "/p")],
            ImageError::NotNormal(b"/s/./t".to_vec()),
            "/s/./t is not a real path",
        ),
        (
            vec![map("/s", "/p/../q")],
            ImageError::NotNormal(b"/p/../q".to_vec()),
            "/p/../q is not a real path",
        ),
        (
            vec![map("/s", "/p=q")],
            ImageError::EqualsInPost(b"/p=q".to_vec()),
            "the post root /p=q contains `=`",
        ),
        (
            vec![map("/s", "/s")],
            ImageError::Duplicate(b"/s".to_vec()),
            "/s is a root of the image twice",
        ),
        (
            vec![map("/s", "/p"), map("/t", "/p")],
            ImageError::Duplicate(b"/p".to_vec()),
            "/p is a root of the image twice",
        ),
        (
            vec![map("/s", "/s/p")],
            ImageError::Nested { inner: b"/s/p".to_vec(), outer: b"/s".to_vec() },
            "/s/p lies under /s",
        ),
        (
            vec![map("/a/b", "/p"), map("/a", "/q")],
            ImageError::Nested { inner: b"/a/b".to_vec(), outer: b"/a".to_vec() },
            "/a/b lies under /a",
        ),
    ];
    for (maps, want, shown) in cases {
        assert_eq!(err(maps), (want, shown.to_owned()));
    }
    let e = Image::new(vec![RootMap { subject: b"/s\xff".to_vec(), post: b"/p".to_vec() }]);
    assert_eq!(e.unwrap_err(), ImageError::NotUtf8(b"/s\xff".to_vec()));
    // `=` is refused in a post root only; `/a/bc` does not lie under `/a`.
    assert!(Image::new(vec![map("/s=t", "/p")]).is_ok());
    assert!(Image::new(vec![map("/a", "/p"), map("/ab", "/q")]).is_ok());
    assert!(Image::new(vec![map("/a/bc", "/a/b")]).is_ok());
}

// §5.1: the maps sorted by `subject`; no map is the identity.
#[test]
fn s5_1_sorted_and_identity() {
    let image = Image::new(vec![map("/t", "/q"), map("/s", "/p")]).unwrap();
    assert_eq!(image.maps(), [map("/s", "/p"), map("/t", "/q")]);
    assert!(!image.is_identity());
    assert!(Image::identity().is_identity());
    assert_eq!(Image::new(Vec::new()).unwrap(), Image::identity());
}

// §5.1 "The post command". `-DROOT="/s"` is kept: the root is followed
// by `"`, neither `/` nor the end of the word (the rule of the bullet; the
// example of the paragraph on the textual rule said otherwise and was
// corrected).
#[test]
fn s5_1_post_command() {
    let image = Image::new(vec![map("/t", "/q"), map("/s", "/p")]).unwrap();
    let command = Command {
        directory: b"/s/build".to_vec(),
        argv: words(&[
            "/s/bin/c++",
            "-I/s/include",
            "--sysroot=/s/x",
            "-DROOT=\"/s\"",
            "-DROOT=\"/s/\"",
            "/mnt/s/x",
            "/sx/y",
            "/s",
            "/s/a.cpp:/t/b.cpp",
            "/s:/s/b",
            "-fmacro-prefix-map=/s/=/x/",
        ]),
        env: vec![EnvVar { name: b"A".to_vec(), value: b"/s/env".to_vec() }],
    };
    let post = image.command(&command);
    assert_eq!(post.directory, b"/p/build".to_vec());
    assert_eq!(
        post.argv,
        words(&[
            "/s/bin/c++",
            "-I/p/include",
            "--sysroot=/p/x",
            "-DROOT=\"/s\"",
            "-DROOT=\"/p/\"",
            "/mnt/p/x",
            "/sx/y",
            "/p",
            "/p/a.cpp:/q/b.cpp",
            "/s:/p/b",
            "-fmacro-prefix-map=/p/=/x/",
            "-fmacro-prefix-map=/p/=/s/",
            "-fmacro-prefix-map=/q/=/t/",
        ])
    );
    assert_eq!(post.env, command.env);
    // The directory: the root itself is rewritten, `/sx` is not.
    for (dir, want) in [("/s", "/p"), ("/sx", "/sx"), ("/u/s", "/u/s")] {
        let c = Command { directory: dir.as_bytes().to_vec(), ..command.clone() };
        assert_eq!(image.command(&c).directory, want.as_bytes());
    }
    // The identity's post command is the command.
    assert_eq!(Image::identity().command(&command), command);
}

// §5.1: subject paths, and guard G1's test.
#[test]
fn s5_1_subject_paths_and_g1() {
    let image = Image::new(vec![map("/s", "/p")]).unwrap();
    assert_eq!(image.to_subject(b"/p/x/y.h"), b"/s/x/y.h");
    assert_eq!(image.to_subject(b"/p"), b"/s");
    assert_eq!(image.to_subject(b"/pq/y.h"), b"/pq/y.h");
    assert_eq!(image.to_subject(b"/usr/include/x.h"), b"/usr/include/x.h");
    assert!(image.under_subject(b"/s"));
    assert!(image.under_subject(b"/s/x.h"));
    assert!(!image.under_subject(b"/sx/x.h"));
    assert!(!image.under_subject(b"/p/x.h"));
    assert!(!Image::identity().under_subject(b"/s/x.h"));
    assert_eq!(Image::identity().to_subject(b"/p/x.h"), b"/p/x.h");
}

// `spec/exec.md` §8.7: `--root`/`--post` made real paths of directories.
#[test]
fn s5_1_from_args() {
    let dir = Dir::new("args");
    let s = dir.mkdir("s");
    let p = dir.mkdir("p");
    std::os::unix::fs::symlink(&p, dir.0.join("link")).unwrap();
    std::fs::write(dir.0.join("file"), "").unwrap();
    let args = |roots: Vec<PathBuf>, posts: Vec<PathBuf>| ImageArgs { roots, posts };

    let image = Image::from_args(&args(vec![s.clone()], vec![dir.0.join("link")])).unwrap();
    assert_eq!(image.maps(), [RootMap { subject: bytes(&s), post: bytes(&p) }]);
    assert_eq!(Image::from_args(&ImageArgs::default()).unwrap(), Image::identity());

    let e = Image::from_args(&args(vec![s.clone()], Vec::new())).unwrap_err();
    assert_eq!(e, "image: 1 --root and 0 --post: they come in pairs");
    let file = dir.0.join("file");
    let e = Image::from_args(&args(vec![s.clone()], vec![file.clone()])).unwrap_err();
    assert_eq!(e, format!("image: {}: not a directory", file.display()));
    let missing = dir.0.join("missing");
    let e = Image::from_args(&args(vec![missing.clone()], vec![p.clone()])).unwrap_err();
    assert!(e.starts_with(&format!("image: {}: ", missing.display())), "{e}");
    let e = Image::from_args(&args(vec![s.clone()], vec![s.clone()])).unwrap_err();
    assert_eq!(e, format!("image: {} is a root of the image twice", s.display()));
}

// `spec/exec.md` §8.4: the image of a job. `from_post(to_post)` gives the
// image back; a root that is not now the real path of a directory is
// refused; the overlay form needs WP-074.
#[test]
fn s5_1_from_post() {
    let dir = Dir::new("post");
    let store = Store::open(&dir.0.join("store"), Access::Shared).unwrap();
    let (sub, post, sub2, post2) = (dir.mkdir("s"), dir.mkdir("p"), dir.mkdir("t"), dir.mkdir("q"));
    let image = Image::new(vec![
        RootMap { subject: bytes(&sub2), post: bytes(&post2) },
        RootMap { subject: bytes(&sub), post: bytes(&post) },
    ])
    .unwrap();
    assert_eq!(Image::from_post(&store, &image.to_post()).unwrap(), image);
    assert_eq!(Image::from_post(&store, &Image::identity().to_post()).unwrap(), Image::identity());
    assert_eq!(
        Image::from_post(&store, &PostImage::Overlay(Digest([1; 32]))).unwrap_err(),
        "the overlay image needs WP-074"
    );
    std::os::unix::fs::symlink(&post, dir.0.join("link")).unwrap();
    let link = bytes(&dir.0.join("link"));
    let e =
        Image::from_post(&store, &PostImage::Tree(vec![(bytes(&sub), link.clone())])).unwrap_err();
    assert_eq!(
        e,
        format!("image: {} is not the real path of a directory", String::from_utf8(link).unwrap())
    );
    let gone = bytes(&dir.0.join("gone"));
    let e = Image::from_post(&store, &PostImage::Tree(vec![(bytes(&sub), gone)])).unwrap_err();
    assert!(e.contains("gone: "), "{e}");
    let e = Image::from_post(&store, &PostImage::Tree(vec![(b"rel".to_vec(), bytes(&post))]))
        .unwrap_err();
    assert_eq!(e, "image: rel is not an absolute path");
}

fn observer(directory: &Path, main: &Path) -> Observer {
    let command = Command {
        directory: bytes(directory),
        argv: vec![b"c++".to_vec(), b"-std=c++20".to_vec(), bytes(main)],
        env: Vec::new(),
    };
    Observer {
        id: command.observer_id(),
        command,
        language: Language::Cxx,
        main: FileRef { path: bytes(main), size: 0, content: Digest([0; 32]) },
        cost_hint: 1,
        origins: vec![Origin { configuration: 0, entry: 0 }],
    }
}

fn job(dir: &Dir, observer: &Observer) -> Job {
    Job {
        store: bytes(&dir.0.join("store")),
        plan: Digest([7; 32]),
        phase: Phase::Map,
        round: 0,
        observer: observer.clone(),
        transform: None,
        post: None,
    }
}

// §5.1: the added `-fmacro-prefix-map=` words make `__FILE__` in a header
// of the post root spell the subject path, checked by a `static_assert`
// on `sizeof(__FILE__)` in that header. The same tree parsed by its own
// command (no added words) fails it; and `Image::parse` gives the walk's
// value only for an `ok` shard.
#[test]
fn s5_1_file_spelling_in_a_post_root() {
    let _lock = LOCK.lock().unwrap_or_else(PoisonError::into_inner);
    let dir = Dir::new("file");
    let s = dir.mkdir("s");
    let p = dir.mkdir("post-tree");
    let _store = Store::open(&dir.0.join("store"), Access::Shared).unwrap();
    let header = s.join("h.h");
    let check =
        format!("static_assert(sizeof(__FILE__) == {});\n", header.to_str().unwrap().len() + 1);
    for root in [&s, &p] {
        std::fs::write(root.join("a.cpp"), "#include \"h.h\"\nint a = 1;\n").unwrap();
        std::fs::write(root.join("h.h"), &check).unwrap();
    }
    let o = observer(&s, &s.join("a.cpp"));
    let image = Image::new(vec![RootMap { subject: bytes(&s), post: bytes(&p) }]).unwrap();

    let (shard, value) = image.parse(&o, Side::Post, Phase::Verify, 1, |_| 42).unwrap();
    assert_eq!(shard.outcome.status, Status::Ok, "{:?}", shard.outcome);
    assert_eq!(value, Some(42));
    assert_eq!(shard.observer, o.id);
    let inputs = shard.inputs.unwrap();
    assert!(inputs.iter().any(|f| f.path == bytes(&p.join("h.h"))));

    let (shard, value) = image.parse(&o, Side::Pre, Phase::Map, 0, |_| 7).unwrap();
    assert_eq!((shard.outcome.status, value), (Status::Ok, Some(7)));
    assert!(shard.inputs.unwrap().iter().any(|f| f.path == bytes(&header)));

    // The post tree by its own command: `__FILE__` spells the post path.
    let own = observer(&p, &p.join("a.cpp"));
    let shard = MapTask::new().run(&job(&dir, &own)).unwrap();
    assert_eq!(shard.outcome.status, Status::Diagnosed);
    let (shard, value) =
        Image::identity().parse(&own, Side::Post, Phase::Verify, 1, |_| 1).unwrap();
    assert_eq!((shard.outcome.status, value), (Status::Diagnosed, None));
}
