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
    consistent: HashSet<u64>,
    answer: BTreeMap<u64, Answer>,
    unlistable: BTreeSet<Vec<u8>>,
    seen: RefCell<HashSet<Vec<u8>>>,
    folded_lookups: RefCell<u64>,
    names_calls: RefCell<Vec<Vec<u8>>>,
    /// Entries not there yet.
    hidden: RefCell<BTreeSet<Vec<u8>>>,
    next: u64,
}

/// What a folding directory compares: valid UTF-8 with U+200B dropped,
/// `é`, `ö` and U+037E decomposed (NFD, and U+037E's singleton `;`), then
/// lower-cased (so the Kelvin sign is `k`); a name that is not UTF-8 by
/// its bytes, as ext4's casefolding does.
fn fold(name: &[u8]) -> Vec<u8> {
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
    out.to_lowercase().into_bytes()
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
                    if *n % 2 == 0 {
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
        Ok(self.find(p).map(|s| self.entries[&s]))
    }

    fn names(&self, dir: &[u8]) -> io::Result<Option<Vec<Vec<u8>>>> {
        self.names_calls.borrow_mut().push(dir.to_vec());
        let Some(stored) = self.find(dir) else { return Ok(None) };
        if self.unlistable.contains(&stored) {
            return Err(io::Error::new(io::ErrorKind::PermissionDenied, "Permission denied"));
        }
        Ok(Some(self.children(&stored).into_iter().map(|k| leaf(k).to_vec()).collect()))
    }

    fn consistent(&self, dev: u64) -> bool {
        self.consistent.contains(&dev)
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
    let s = Spelling::new(Fake::new().file("/r/src/a.cc", 1).folds("/").folds("/r").folds("/r/src"));
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
    let mounts = Mounts::parse(
        "30 1 0:68 / / rw,relatime - overlay overlay rw,lowerdir=/a,upperdir=/b,workdir=/c\n",
    );
    let dev = makedev(0, 68);
    assert!(!mounts.consistent(dev));
    let mut fake = Fake::new().file("/r/foo.h", dev).folds("/r").answers(dev, Answer::EveryOtherMisses);
    fake.entries.insert(b"/".to_vec(), Id { dev, ino: 1 });
    if mounts.consistent(dev) {
        fake = fake.consistent(dev);
    }
    let s = Spelling::new(fake);
    assert_eq!(ok(&s, "/r/Foo.h"), "/r/foo.h");
}

/// A3: a directory with search but no read permission above the tree.
/// On a consistent case-sensitive device it is probed, not listed; where
/// it must be listed, `Unlistable` names it.
#[test]
fn a_directory_that_cannot_be_listed() {
    let s = Spelling::new(Fake::new().file("/home/u/a.cc", 1).unlistable("/home").consistent(1));
    assert_eq!(ok(&s, "/home/u/a.cc"), "/home/u/a.cc");
    assert_eq!(s.lookup().listings_of("/home"), 0);

    let s = Spelling::new(Fake::new().file("/home/u/a.cc", 1).unlistable("/home"));
    let got = on_disk(&s, "/home/u/a.cc");
    let Err(SpellingError::Unlistable { dir, .. }) = got else { panic!("{got:?}") };
    assert_eq!(dir, b"/home");
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

/// mountinfo: the device of a line is `makedev(major, minor)`; a device on
/// several lines is consistent only if every line's type is.
#[test]
fn mounts_by_device() {
    let text = "\
22 1 8:1 / / rw - ext4 /dev/sda1 rw
23 22 0:104 / /dev/shm rw,nosuid shared:5 - tmpfs shm rw,size=65536k
24 22 0:47 /x /v rw,nosuid - fakeowner /run/host_mark/Volumes rw
25 22 259:3 / /a rw - ext4 /dev/nvme0n1p3 rw
26 22 259:3 /b /c rw - overlay overlay rw
27 22 garbage
";
    let m = Mounts::parse(text);
    assert_eq!(makedev(8, 1), 0x801);
    assert_eq!(makedev(259, 3), 259 * 256 + 3);
    assert_eq!(makedev(0, 104), 104);
    assert!(m.consistent(makedev(8, 1)));
    assert_eq!(m.types(makedev(0, 104)), ["tmpfs"]);
    assert!(m.consistent(makedev(0, 104)));
    assert!(!m.consistent(makedev(0, 47)), "FUSE");
    assert!(!m.consistent(makedev(259, 3)), "ext4 and overlay on one device");
    assert!(!m.consistent(makedev(9, 9)), "a device no line names");
}
