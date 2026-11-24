//! `Spelling::on_disk` over fake file systems (`spec/edits.md` §2.1, "One
//! spelling"; `spec/landed/edits/WP-147.md` §3–§5): directories that
//! fold case and normalisation the way the volumes Slip meets do, devices
//! whose lookups are answered consistently or not, directories that
//! cannot be listed, hard links, and a tree across two devices.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::io;

use slip_spelling::{Id, Lookup, Mounts, Spelling, SpellingError, makedev};

/// How a device answers a lookup that finds a name by folding (not by its
/// exact bytes).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Answer {
    Always,
    /// The first lookup of each such spelling finds nothing.
    FirstMisses,
    /// The second, fourth, … such lookup on the device finds nothing.
    EveryOtherMisses,
}

/// A file system. Every entry (directories too) has a stored spelling and
/// an identity; a folding directory finds a child by [`fold`].
#[derive(Debug, Default)]
struct Fake {
    entries: BTreeMap<Vec<u8>, Id>,
    folding: BTreeSet<Vec<u8>>,
    /// Directories that fold normalisation but not case (ZFS with
    /// `normalization=formD` and `casesensitivity=sensitive`).
    normalising: BTreeSet<Vec<u8>>,
    /// Trust by a mount table (`Mounts::consistent`) instead of by device.
    mounts: Option<Mounts>,
    /// Directories whose listing fails while named here (`EMFILE`).
    flaky: RefCell<BTreeSet<Vec<u8>>>,
    consistent: HashSet<u64>,
    answer: BTreeMap<u64, Answer>,
    unlistable: BTreeSet<Vec<u8>>,
    seen: RefCell<HashSet<Vec<u8>>>,
    folded_lookups: RefCell<u64>,
    names_calls: RefCell<Vec<Vec<u8>>>,
    lstats: RefCell<Vec<Vec<u8>>>,
    /// Entries not there yet.
    hidden: RefCell<BTreeSet<Vec<u8>>>,
    next: u64,
}

/// What a folding directory compares: valid UTF-8 with U+200B dropped,
/// `é`, `ö` and U+037E decomposed (NFD, and U+037E's singleton `;`), then
/// lower-cased (so the Kelvin sign is `k`); a name that is not UTF-8 by
/// its bytes, as ext4's casefolding does.
fn fold(name: &[u8]) -> Vec<u8> {
    let s = normalise(name);
    String::from_utf8_lossy(&s).to_lowercase().into_bytes()
}

/// [`fold`] without the lower-casing.
fn normalise(name: &[u8]) -> Vec<u8> {
    let Ok(s) = std::str::from_utf8(name) else { return name.to_vec() };
    let mut out = String::new();
    for ch in s.chars() {
        match ch {
            '\u{200B}' => {}
            '\u{e9}' => out.push_str("e\u{301}"),
            '\u{f6}' => out.push_str("o\u{308}"),
            '\u{37e}' => out.push(';'),
            c => out.push(c),
        }
    }
    out.into_bytes()
}

fn parent(p: &[u8]) -> &[u8] {
    match p.iter().rposition(|&b| b == b'/') {
        Some(0) => b"/",
        Some(i) => &p[..i],
        None => b"",
    }
}

fn leaf(p: &[u8]) -> &[u8] {
    &p[p.iter().rposition(|&b| b == b'/').map_or(0, |i| i + 1)..]
}

impl Fake {
    fn new() -> Fake {
        let mut f = Fake::default();
        f.add(b"/", 1);
        f
    }

    fn add(&mut self, p: &[u8], dev: u64) -> Id {
        if let Some(&id) = self.entries.get(p) {
            return id;
        }
        if p != b"/" {
            self.add(parent(p), dev);
        }
        self.next += 1;
        let id = Id { dev, ino: self.next };
        self.entries.insert(p.to_vec(), id);
        id
    }

    fn file(mut self, p: &str, dev: u64) -> Fake {
        self.add(p.as_bytes(), dev);
        self
    }

    fn bytes(mut self, p: &[u8], dev: u64) -> Fake {
        self.add(p, dev);
        self
    }

    /// `p` a hard link of `to`.
    fn link(mut self, p: &str, to: &str) -> Fake {
        let id = self.entries[to.as_bytes()];
        self.entries.insert(p.as_bytes().to_vec(), id);
        self
    }

    fn folds(mut self, dir: &str) -> Fake {
        self.folding.insert(dir.as_bytes().to_vec());
        self
    }

    fn normalises(mut self, dir: &str) -> Fake {
        self.normalising.insert(dir.as_bytes().to_vec());
        self
    }

    fn mounted(mut self, text: &str) -> Fake {
        self.mounts = Some(Mounts::parse(text));
        self
    }

    fn consistent(mut self, dev: u64) -> Fake {
        self.consistent.insert(dev);
        self
    }

    fn answers(mut self, dev: u64, a: Answer) -> Fake {
        self.answer.insert(dev, a);
        self
    }

    fn unlistable(mut self, dir: &str) -> Fake {
        self.unlistable.insert(dir.as_bytes().to_vec());
        self
    }

    fn hide(self, p: &str) -> Fake {
        self.hidden.borrow_mut().insert(p.as_bytes().to_vec());
        self
    }

    fn children(&self, dir: &[u8]) -> Vec<&Vec<u8>> {
        let hidden = self.hidden.borrow();
        self.entries
            .keys()
            .filter(|k| k.as_slice() != b"/" && parent(k) == dir && !hidden.contains(*k))
            .collect()
    }

    /// The stored spelling `p` reaches, if any.
    fn find(&self, p: &[u8]) -> Option<Vec<u8>> {
        if p == b"/" {
            return Some(b"/".to_vec());
        }
        let mut at = b"/".to_vec();
        let mut by_folding = false;
        for name in p[1..].split(|&b| b == b'/') {
            let kids = self.children(&at);
            let exact = kids.iter().find(|k| leaf(k) == name);
            let child = match exact {
                Some(k) => (*k).clone(),
                None if self.folding.contains(&at) => {
                    by_folding = true;
                    (*kids.iter().find(|k| fold(leaf(k)) == fold(name))?).clone()
                }
                None if self.normalising.contains(&at) => {
                    by_folding = true;
                    (*kids.iter().find(|k| normalise(leaf(k)) == normalise(name))?).clone()
                }
                None => return None,
            };
            at = child;
        }
        if by_folding {
            let dev = self.entries[&at].dev;
            match self.answer.get(&dev).copied().unwrap_or(Answer::Always) {
                Answer::Always => {}
                Answer::FirstMisses => {
                    if self.seen.borrow_mut().insert(p.to_vec()) {
                        return None;
                    }
                }
                Answer::EveryOtherMisses => {
                    let mut n = self.folded_lookups.borrow_mut();
                    *n += 1;
                    if n.is_multiple_of(2) {
                        return None;
                    }
                }
            }
        }
        Some(at)
    }

    fn listings_of(&self, dir: &str) -> usize {
        self.names_calls.borrow().iter().filter(|d| d.eq_ignore_ascii_case(dir.as_bytes())).count()
    }

    fn listings(&self) -> usize {
        self.names_calls.borrow().len()
    }
}

impl Lookup for Fake {
    fn lstat(&self, p: &[u8]) -> io::Result<Option<Id>> {
        self.lstats.borrow_mut().push(p.to_vec());
        Ok(self.find(p).map(|s| self.entries[&s]))
    }

    fn names(&self, dir: &[u8]) -> io::Result<Option<Vec<Vec<u8>>>> {
        self.names_calls.borrow_mut().push(dir.to_vec());
        let Some(stored) = self.find(dir) else { return Ok(None) };
        if self.unlistable.contains(&stored) {
            return Err(io::Error::new(io::ErrorKind::PermissionDenied, "Permission denied"));
        }
        if self.flaky.borrow().contains(&stored) {
            return Err(io::Error::other("Too many open files"));
        }
        Ok(Some(self.children(&stored).into_iter().map(|k| leaf(k).to_vec()).collect()))
    }

    fn consistent(&self, dir: &[u8], dev: u64) -> bool {
        match &self.mounts {
            Some(m) => m.consistent(dir, dev),
            None => self.consistent.contains(&dev),
        }
    }
}

fn on_disk(s: &Spelling<Fake>, p: &str) -> Result<String, SpellingError> {
    s.on_disk(p.as_bytes()).map(|b| String::from_utf8_lossy(&b).into_owned())
}

fn ok(s: &Spelling<Fake>, p: &str) -> String {
    on_disk(s, p).unwrap_or_else(|e| panic!("{p}: {e}"))
}

/// A folding volume, not known to be consistent (this workspace's): the
/// path is given its directories' spellings, the last component too, and
/// the answer is its own spelling.
#[test]
fn folding_volume_gives_the_listed_spelling() {
    let s =
        Spelling::new(Fake::new().file("/r/src/a.cc", 1).folds("/").folds("/r").folds("/r/src"));
    assert_eq!(ok(&s, "/R/Src/A.cc"), "/r/src/a.cc");
    assert_eq!(ok(&s, "/r/src/A.cc"), "/r/src/a.cc", "the last component");
    assert_eq!(ok(&s, "/r/src/a.cc"), "/r/src/a.cc", "idempotent");
    assert_eq!(ok(&s, "/"), "/");
}

/// A case-sensitive volume known to be consistent: no directory is listed
/// (one probe per directory decides it), and the answer is the path.
#[test]
fn case_sensitive_consistent_volume_lists_nothing() {
    let s = Spelling::new(
        Fake::new().file("/r/src/a.cc", 1).file("/r/src/b.cc", 1).file("/r/13", 1).consistent(1),
    );
    assert_eq!(ok(&s, "/r/src/a.cc"), "/r/src/a.cc");
    assert_eq!(ok(&s, "/r/src/b.cc"), "/r/src/b.cc");
    // ruling (a): a `NotFolding` directory serves a letterless component.
    assert_eq!(ok(&s, "/r/13"), "/r/13");
    assert_eq!(s.lookup().listings(), 0, "{:?}", s.lookup().names_calls);
}

/// The cache is keyed by the directory, not by the spelling asked:
/// `/r/Src/a` then `/r/src/b` list `/r/src` once.
#[test]
fn one_listing_per_directory_whatever_the_spelling_asked() {
    let s = Spelling::new(
        Fake::new().file("/r/src/a", 1).file("/r/src/b", 1).folds("/").folds("/r").folds("/r/src"),
    );
    assert_eq!(ok(&s, "/r/Src/a"), "/r/src/a");
    assert_eq!(ok(&s, "/r/src/b"), "/r/src/b");
    assert_eq!(ok(&s, "/r/SRC/b"), "/r/src/b");
    assert_eq!(s.lookup().listings_of("/r/src"), 1, "{:?}", s.lookup().names_calls);
}

/// WP-075's hole (A2): another normalisation on a volume whose lookups are
/// answered inconsistently, the first lookup missing: `Missing`, never the
/// spelling asked.
#[test]
fn a_lookup_that_misses_is_missing_not_a_pass() {
    let s = Spelling::new(
        Fake::new()
            .file("/w/vend\u{f6}r/x.h", 1)
            .folds("/w")
            .folds("/w/vend\u{f6}r")
            .answers(1, Answer::FirstMisses),
    );
    let got = on_disk(&s, "/w/vendo\u{308}r/x.h");
    assert!(matches!(got, Err(SpellingError::Missing { .. })), "{got:?}");
    // Asked again, the volume finds it: the listed spelling.
    assert_eq!(ok(&s, "/w/vendo\u{308}r/x.h"), "/w/vend\u{f6}r/x.h");
}

/// A device whose lookups are not consistent is never probed: there, a
/// probe in another case can miss in a folding directory. This one is
/// `overlay` by its mountinfo (the ruling's Q3: `overlay` is not trusted),
/// answering every other folded lookup with nothing.
#[test]
fn an_overlay_device_is_listed() {
    let text =
        "30 1 0:68 / / rw,relatime - overlay overlay rw,lowerdir=/a,upperdir=/b,workdir=/c\n";
    let dev = makedev(0, 68);
    assert!(!Mounts::parse(text).consistent(b"/r", dev));
    let mut fake = Fake::new()
        .file("/r/foo.h", dev)
        .folds("/r")
        .answers(dev, Answer::EveryOtherMisses)
        .mounted(text);
    fake.entries.insert(b"/".to_vec(), Id { dev, ino: 1 });
    let s = Spelling::new(fake);
    assert_eq!(ok(&s, "/r/Foo.h"), "/r/foo.h");
}

/// A3: a directory with search but no read permission above the tree.
/// On a consistent case-sensitive device it is probed, not listed; where
/// it must be listed, `Unlistable` names it.
#[test]
fn a_directory_that_cannot_be_listed() {
    let s = Spelling::new(Fake::new().file("/srv/u/a.cc", 1).unlistable("/srv").consistent(1));
    assert_eq!(ok(&s, "/srv/u/a.cc"), "/srv/u/a.cc");
    assert_eq!(s.lookup().listings_of("/srv"), 0);

    let s = Spelling::new(Fake::new().file("/srv/u/a.cc", 1).unlistable("/srv"));
    let got = on_disk(&s, "/srv/u/a.cc");
    let Err(SpellingError::Unlistable { dir, .. }) = got else { panic!("{got:?}") };
    assert_eq!(dir, b"/srv");
}

/// A hard link in another case in a case-sensitive directory: the probe
/// finds the same file, so the directory is listed (once); both names are
/// their own spelling.
#[test]
fn hard_links_list_the_directory() {
    let s = Spelling::new(Fake::new().file("/d/foo", 1).link("/d/FOO", "/d/foo").consistent(1));
    assert_eq!(ok(&s, "/d/foo"), "/d/foo");
    assert_eq!(ok(&s, "/d/FOO"), "/d/FOO");
    assert_eq!(s.lookup().listings_of("/d"), 1);
}

/// Two listed names of one file that both could be the component:
/// `NoName`, naming both.
#[test]
fn two_names_of_one_file_are_no_name() {
    let s = Spelling::new(Fake::new().file("/d/Foo", 1).link("/d/fOO", "/d/Foo").folds("/d"));
    let got = on_disk(&s, "/d/FOO");
    let Err(SpellingError::NoName { names, .. }) = got else { panic!("{got:?}") };
    assert_eq!(names, [b"Foo".to_vec(), b"fOO".to_vec()]);
}

/// The Kelvin sign (U+212A) for `k`: a non-ASCII component, listed, the
/// stored name found by identity.
#[test]
fn the_kelvin_sign() {
    let s = Spelling::new(Fake::new().file("/r/k.h", 1).folds("/r").consistent(1));
    assert_eq!(ok(&s, "/r/\u{212A}.h"), "/r/k.h");
}

/// Ruling (a): in a casefolding directory a letterless ASCII component
/// reaches a stored non-ASCII name (`1\u{200B}3` as `13`, U+037E as `;`):
/// such a component lists an undecided directory, even on a consistent
/// device.
#[test]
fn a_letterless_component_lists_an_undecided_directory() {
    let s = Spelling::new(
        Fake::new().file("/r/1\u{200B}3", 1).file("/r/\u{37e}", 1).folds("/r").consistent(1),
    );
    assert_eq!(ok(&s, "/r/13"), "/r/1\u{200B}3");
    assert_eq!(ok(&s, "/r/;"), "/r/\u{37e}");
}

/// Ruling (b): a component with a non-ASCII byte never decides its
/// directory's state. Here the directory folds every valid name and
/// compares `fo\xFFo` by its bytes, so a probe of `FO\xFFO` would miss; a
/// later `Foo.h` still gets the listed `foo.h`.
#[test]
fn a_non_ascii_component_does_not_decide_the_state() {
    let s = Spelling::new(
        Fake::new().bytes(b"/d/fo\xFFo", 1).file("/d/foo.h", 1).folds("/d").consistent(1),
    );
    assert_eq!(s.on_disk(b"/d/fo\xFFo").unwrap(), b"/d/fo\xFFo");
    assert_eq!(ok(&s, "/d/Foo.h"), "/d/foo.h");
    // A directory that folds normalisation and not case: a probe by the
    // non-ASCII name would miss its other case and call the directory
    // not folding, passing the NFD spelling as asked.
    let s = Spelling::new(Fake::new().file("/e/caf\u{e9}.h", 1).normalises("/e").consistent(1));
    assert_eq!(ok(&s, "/e/cafe\u{301}.h"), "/e/caf\u{e9}.h");
}

/// Folding is decided per directory, not per device: a folding directory
/// under a case-sensitive one on one consistent device.
#[test]
fn per_directory_not_per_mount() {
    let s = Spelling::new(
        Fake::new().file("/r/cs/a", 1).file("/r/fold/foo.h", 1).folds("/r/fold").consistent(1),
    );
    assert_eq!(ok(&s, "/r/cs/a"), "/r/cs/a");
    assert_eq!(ok(&s, "/r/fold/Foo.h"), "/r/fold/foo.h");
}

/// A tree across two devices: each directory is decided on its own
/// device. `/r` is on a consistent case-sensitive device and is not
/// listed; `/r/m`, a mount of a folding volume not known consistent, is.
#[test]
fn a_tree_across_two_devices() {
    let mut f = Fake::new().file("/r/a", 1).consistent(1).folds("/r/m");
    f.add(b"/r/m", 2);
    f.add(b"/r/m/foo.h", 2);
    let s = Spelling::new(f);
    assert_eq!(ok(&s, "/r/a"), "/r/a");
    assert_eq!(ok(&s, "/r/m/Foo.h"), "/r/m/foo.h");
    assert_eq!(s.lookup().listings_of("/r"), 0, "{:?}", s.lookup().names_calls);
    assert_eq!(s.lookup().listings_of("/r/m"), 1);
}

/// A name made after its directory was listed: the walk lists it again,
/// once, and finds it.
#[test]
fn a_name_made_since_the_listing() {
    let s = Spelling::new(Fake::new().file("/r/a", 1).file("/r/new", 1).hide("/r/new").folds("/r"));
    assert_eq!(ok(&s, "/r/A"), "/r/a");
    s.lookup().hidden.borrow_mut().clear();
    assert_eq!(ok(&s, "/r/New"), "/r/new");
    assert_eq!(s.lookup().listings_of("/r"), 2);
}

/// Cost: a file made since its directory was listed costs one listing
/// and no `lstat` of the directory's other names; a respelled name in a
/// cached listing costs no listing.
#[test]
fn a_new_file_in_a_big_listed_directory() {
    let mut fake = Fake::new().folds("/r");
    for i in 0..1000 {
        fake = fake.file(&format!("/r/f{i:04}"), 1);
    }
    let s = Spelling::new(fake.file("/r/new", 1).hide("/r/new"));
    assert_eq!(ok(&s, "/r/f0000"), "/r/f0000");
    assert_eq!(s.lookup().listings_of("/r"), 1);
    s.lookup().hidden.borrow_mut().clear();
    s.lookup().lstats.borrow_mut().clear();
    assert_eq!(ok(&s, "/r/new"), "/r/new");
    assert_eq!(s.lookup().listings_of("/r"), 2, "one names call");
    let others = |s: &Spelling<Fake>| {
        s.lookup().lstats.borrow().iter().filter(|p| p.starts_with(b"/r/f")).count()
    };
    assert_eq!(others(&s), 0, "no lstat of the other names");
    assert_eq!(ok(&s, "/r/F0001"), "/r/f0001");
    assert_eq!(s.lookup().listings_of("/r"), 2, "a respelled name: no names call");
    assert_eq!(others(&s), 1, "the one name equal but for case");
}

/// Not there, and not a path.
#[test]
fn missing_and_not_a_path() {
    let s = Spelling::new(Fake::new().file("/r/a", 1).consistent(1));
    let got = on_disk(&s, "/r/b");
    let Err(SpellingError::Missing { at, .. }) = got else { panic!("{got:?}") };
    assert_eq!(at, b"/r/b");
    for p in ["r/a", "/r/../a", "/r/", "/r//a", ""] {
        assert!(matches!(on_disk(&s, p), Err(SpellingError::NotAPath(_))), "{p}");
    }
}

/// mountinfo: the device of a line is `makedev(major, minor)`; a
/// directory is trusted when the mount that holds it (the longest mount
/// point at or above it) names its device and a trusted type, and every
/// line of that device is of a trusted type.
#[test]
fn mounts_by_device() {
    let text = "\
22 1 8:1 / / rw - ext4 /dev/sda1 rw
23 22 0:104 / /dev/shm rw,nosuid shared:5 - tmpfs shm rw,size=65536k
24 22 0:47 /x /v rw,nosuid - fakeowner /run/host_mark/Volumes rw
25 22 259:3 / /a rw - ext4 /dev/nvme0n1p3 rw
26 22 259:3 /b /c rw - overlay overlay rw
28 22 8:300 / /big rw - xfs /dev/sdz rw
27 22 garbage
";
    let m = Mounts::parse(text);
    assert_eq!(makedev(8, 1), 0x801);
    assert_eq!(makedev(259, 3), 259 * 256 + 3);
    assert_eq!(makedev(0, 104), 104);
    assert_eq!(makedev(8, 300), 1_050_668, "minor > 255: 44 | 8 << 8 | 256 << 12");
    assert!(m.consistent(b"/usr", makedev(8, 1)));
    assert_eq!(m.types(makedev(0, 104)), ["tmpfs"]);
    assert!(m.consistent(b"/dev/shm/x", makedev(0, 104)));
    assert!(m.consistent(b"/big/d", makedev(8, 300)), "a minor above 255");
    assert!(!m.consistent(b"/v/w", makedev(0, 47)), "FUSE");
    assert!(!m.consistent(b"/a/d", makedev(259, 3)), "ext4 and overlay on one device");
    assert!(!m.consistent(b"/q", makedev(9, 9)), "a device no line names");
    assert_eq!(m.holder(b"/dev/shmx").map(|h| h.at.as_slice()), Some(b"/".as_slice()));
    assert_eq!(m.holder(b"/dev/shm").map(|h| h.at.as_slice()), Some(b"/dev/shm".as_slice()));
}

/// Review S2: a FUSE mount reports a device of its own choosing. Here the
/// mount at `/v` (`fakeowner`, 0:47) reports `st_dev` 0:45, which another
/// line, a tmpfs at `/t`, names: trusting by the device alone would trust
/// it. The mount that holds `/v/w` is `/v`, of another device: not
/// trusted, so its folding directory is listed, never probed (a probe on
/// this mount can miss).
#[test]
fn a_mount_that_reports_another_mounts_device() {
    let text = "\
22 1 8:1 / / rw - ext4 /dev/sda1 rw
23 22 0:45 / /t rw - tmpfs tmpfs rw
24 22 0:47 /x /v rw,nosuid - fakeowner /run/host_mark/Volumes rw
25 22 0:46 / /u rw - tmpfs tmpfs rw
";
    let dev = makedev(0, 45);
    let m = Mounts::parse(text);
    assert!(m.consistent(b"/t/a", dev), "the tmpfs itself");
    assert!(!m.consistent(b"/v/w", dev), "the FUSE mount reporting the tmpfs's device");
    assert!(!m.consistent(b"/u/w", dev), "a trusted holder of another device");
    let fake = Fake::new()
        .file("/v/w/foo.h", dev)
        .folds("/v/w")
        .answers(dev, Answer::EveryOtherMisses)
        .mounted(text);
    let s = Spelling::new(fake);
    assert_eq!(ok(&s, "/v/w/Foo.h"), "/v/w/foo.h");
    assert_eq!(s.lookup().listings_of("/v/w"), 1);
}

/// A mount point with a space, which mountinfo writes `\040`.
#[test]
fn an_escaped_mount_point() {
    let m =
        Mounts::parse("22 1 8:1 / / rw - ext4 a rw\n23 22 0:104 / /my\\040disk rw - tmpfs b rw\n");
    assert!(m.consistent(b"/my disk/x", makedev(0, 104)));
    assert_eq!(m.holder(b"/my disk").map(|h| h.at.clone()), Some(b"/my disk".to_vec()));
}

/// Review S4: a listing that failed (`EMFILE`) is not kept: the next
/// question lists again, and finds the name.
#[test]
fn a_listing_that_failed_is_listed_again() {
    let s = Spelling::new(Fake::new().file("/r/a", 1).file("/r/b", 1).folds("/r"));
    s.lookup().flaky.borrow_mut().insert(b"/r".to_vec());
    let got = on_disk(&s, "/r/a");
    assert!(matches!(got, Err(SpellingError::Unlistable { .. })), "{got:?}");
    s.lookup().flaky.borrow_mut().clear();
    assert_eq!(ok(&s, "/r/B"), "/r/b");
    assert_eq!(s.lookup().listings_of("/r"), 2);
}

/// Review S3, a stated limit (`spec/edits.md` §2.1): the tree does not
/// change during a run. A case-only rename after the directory was listed
/// (`foo.h` becomes `Foo.h`) keeps the old spelling for the walk's life;
/// a new walk (apply's, per call) gives the new one.
#[test]
fn a_case_only_rename_after_the_listing_is_a_stated_limit() {
    let fake = || {
        Fake::new().file("/r/foo.h", 1).link("/r/Foo.h", "/r/foo.h").hide("/r/Foo.h").folds("/r")
    };
    let s = Spelling::new(fake());
    assert_eq!(ok(&s, "/r/FOO.H"), "/r/foo.h");
    let rename = |f: &Fake| {
        f.hidden.borrow_mut().clear();
        f.hidden.borrow_mut().insert(b"/r/foo.h".to_vec());
    };
    rename(s.lookup());
    assert_eq!(ok(&s, "/r/Foo.h"), "/r/foo.h", "the stale answer, for the walk's life");
    let fresh = Spelling::new(fake());
    rename(fresh.lookup());
    assert_eq!(ok(&fresh, "/r/foo.h"), "/r/Foo.h");
}

/// Review note, a stated limit (`spec/landed/edits/WP-147.md` "As built"):
/// a hard link in a nearer tier than the name the volume matched wins.
/// The volume finds U+037E as the stored `;`; the same file is also
/// listed as `é`, a non-ASCII name, tried first. A path given that name
/// is a file with two links, which apply refuses (`hard_linked`); a
/// file-level ownership entry naming either name is its own spelling and
/// covers that name only, and the write is refused all the same.
#[test]
fn a_hard_link_in_a_nearer_tier_wins() {
    let s = Spelling::new(Fake::new().file("/r/;", 1).link("/r/\u{e9}", "/r/;").folds("/r"));
    assert_eq!(ok(&s, "/r/\u{37e}"), "/r/\u{e9}");
}

/// Two directories with one identity (a file system that reuses or fakes
/// inode numbers): the listing of one serves the other, and a name of the
/// first asked in the second still needs a lookup that finds it: missing.
#[test]
fn two_directories_with_one_identity() {
    let mut f = Fake::new().folds("/a").folds("/b");
    f.add(b"/a/x.h", 1);
    f.add(b"/b/y.h", 1);
    let ida = f.entries[b"/a".as_slice()];
    f.entries.insert(b"/b".to_vec(), ida);
    let s = Spelling::new(f);
    assert_eq!(ok(&s, "/a/X.h"), "/a/x.h");
    let got = on_disk(&s, "/b/x.h");
    assert!(got.is_err(), "{got:?}");
}

/// A path through a file: missing.
#[test]
fn a_path_through_a_file_is_missing() {
    let s = Spelling::new(Fake::new().file("/r/a", 1).consistent(1));
    let got = on_disk(&s, "/r/a/b");
    assert!(matches!(got, Err(SpellingError::Missing { .. })), "{got:?}");
}

/// Review S5 (S2): ruling (b), a non-ASCII component lists its directory
/// whatever the directory's state, `NotFolding` too: a directory that
/// folds normalisation and not case is seen not to fold by an ASCII
/// probe, and `café.h` asked in NFD is the listed NFC name.
#[test]
fn a_non_ascii_component_lists_a_not_folding_directory() {
    let s = Spelling::new(
        Fake::new().file("/d/foo.h", 1).file("/d/caf\u{e9}.h", 1).normalises("/d").consistent(1),
    );
    assert_eq!(ok(&s, "/d/foo.h"), "/d/foo.h");
    assert_eq!(s.lookup().listings_of("/d"), 0, "decided by the probe");
    assert_eq!(ok(&s, "/d/cafe\u{301}.h"), "/d/caf\u{e9}.h");
    assert_eq!(s.lookup().listings_of("/d"), 1);
}

/// Review S5 (S6): a listed name that the lookup no longer finds (removed
/// since the listing) is missing, never a pass.
#[test]
fn a_listed_name_the_lookup_misses_is_missing() {
    let s = Spelling::new(Fake::new().file("/r/a", 1).file("/r/b", 1).folds("/r"));
    assert_eq!(ok(&s, "/r/a"), "/r/a");
    s.lookup().hidden.borrow_mut().insert(b"/r/b".to_vec());
    let got = on_disk(&s, "/r/b");
    assert!(matches!(&got, Err(SpellingError::Missing { at, .. }) if at == b"/r/b"), "{got:?}");
}

/// Review S5 (S11): the last tier. A stored ASCII name reached by a
/// non-ASCII spelling (U+037E for a stored `;`, which this workspace's
/// volume finds): neither equal but for ASCII case nor non-ASCII, so only
/// every name, over a fresh listing, finds it.
#[test]
fn a_stored_ascii_name_reached_by_a_non_ascii_spelling() {
    let s = Spelling::new(Fake::new().file("/r/;", 1).file("/r/a", 1).folds("/r"));
    assert_eq!(ok(&s, "/r/\u{37e}"), "/r/;");
}
