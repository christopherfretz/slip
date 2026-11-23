//! `ParseOptions::lookups` and `Unit::lookups` (work package WP-052): what
//! Kiln's lookup log holds, each form through a real parse, as guard L1 of
//! `spec/verify.md` §5.3 reads it. Each test names a form and the place
//! it must show; a form the log does not hold is pinned as such, so that
//! the day Kiln records it the test says so.
//!
//! The parse changes the process's current directory: every test holds
//! [`LOCK`] and uses absolute paths only.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};

use slip_kiln::{Lookups, ParseOptions, parse_observer_with};
use slip_model::{Command, Digest, FileRef, Language, Observer, Origin, Phase, Status};

static LOCK: Mutex<()> = Mutex::new(());

struct Dir {
    root: PathBuf,
    _lock: MutexGuard<'static, ()>,
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

impl Dir {
    fn new(name: &str) -> Dir {
        static N: AtomicUsize = AtomicUsize::new(0);
        let lock = LOCK.lock().unwrap_or_else(PoisonError::into_inner);
        let n = N.fetch_add(1, Ordering::SeqCst);
        let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
            .join(format!("wp052-lookups-{name}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let root = std::fs::canonicalize(&root).unwrap();
        Dir { root, _lock: lock }
    }

    fn write(&self, rel: &str, text: &str) {
        let p = self.root.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, text).unwrap();
    }

    fn at(&self, rel: &str) -> String {
        self.root.join(rel).to_str().unwrap().to_owned()
    }

    /// The lookups of `c++ -std=c++20 <flags> <root>/a.cpp` run from
    /// `<root>/<dir>`, and the shard's status.
    fn lookups(&self, dir: &str, flags: &[&str]) -> (Status, Lookups) {
        let (status, looked) = self.parse(dir, flags);
        (status, looked.expect("a lookup log"))
    }

    /// As [`Dir::lookups`]; `None` when no parse happened.
    fn parse(&self, dir: &str, flags: &[&str]) -> (Status, Option<Lookups>) {
        let mut argv: Vec<Vec<u8>> = vec![b"c++".to_vec(), b"-std=c++20".to_vec()];
        argv.extend(flags.iter().map(|f| f.as_bytes().to_vec()));
        argv.push(self.at("a.cpp").into_bytes());
        let directory = self.root.join(dir);
        std::fs::create_dir_all(&directory).unwrap();
        let command = Command {
            directory: directory.to_str().unwrap().as_bytes().to_vec(),
            argv,
            env: vec![],
        };
        let observer = Observer {
            id: command.observer_id(),
            command,
            language: Language::Cxx,
            main: FileRef {
                path: self.at("a.cpp").into_bytes(),
                size: 0,
                content: Digest([0; 32]),
            },
            cost_hint: 1,
            origins: vec![Origin { configuration: 0, entry: 0 }],
        };
        let mut options = ParseOptions::default();
        options.lookups = true;
        let (shard, looked) =
            parse_observer_with(&observer, Phase::Map, 0, options, |u| u.lookups().cloned())
                .unwrap();
        (shard.outcome.status, looked.flatten())
    }
}

fn tried(l: &Lookups, dir: &str, name: &str) -> bool {
    l.tried.iter().any(|(d, n)| d == dir && n == name)
}

// Off by default: no log unless asked for.
#[test]
fn lookups_off_by_default() {
    let d = Dir::new("off");
    d.write("a.cpp", "int x;\n");
    let o = {
        let command = Command {
            directory: d.root.to_str().unwrap().as_bytes().to_vec(),
            argv: vec![b"c++".to_vec(), b"-std=c++20".to_vec(), d.at("a.cpp").into_bytes()],
            env: vec![],
        };
        Observer {
            id: command.observer_id(),
            command,
            language: Language::Cxx,
            main: FileRef { path: d.at("a.cpp").into_bytes(), size: 0, content: Digest([0; 32]) },
            cost_hint: 1,
            origins: vec![Origin { configuration: 0, entry: 0 }],
        }
    };
    let (_, looked) =
        parse_observer_with(&o, Phase::Map, 0, ParseOptions::default(), |u| u.lookups().is_some())
            .unwrap();
    assert_eq!(looked, Some(false));
}

// `#include "..."` and `#include <...>`, found and not found: the includer's
// directory for `"..."`, then the list from its start (`"..."`) or from
// the angled start; through the hit, or all of it.
#[test]
fn include_both_forms() {
    let d = Dir::new("include");
    d.write("a.cpp", "#include \"q.h\"\n#include <found.h>\n#include <gone.h>\n");
    d.write("inc2/found.h", "\n");
    let (inc1, inc2, inc3) = (d.at("inc1"), d.at("inc2"), d.at("inc3"));
    for i in [&inc1, &inc3] {
        std::fs::create_dir_all(i).unwrap();
    }
    let flags = [format!("-iquote{inc1}"), format!("-I{inc2}"), format!("-I{inc3}")];
    let flags: Vec<&str> = flags.iter().map(String::as_str).collect();
    let (status, l) = d.lookups("", &flags);
    assert_eq!(status, Status::Diagnosed, "q.h and gone.h are missing");
    let root = d.root.to_str().unwrap();
    assert!(tried(&l, root, "q.h"), "{l:?}");
    assert!(tried(&l, &inc1, "q.h"), "{l:?}");
    assert!(tried(&l, &inc2, "q.h") && tried(&l, &inc3, "q.h"), "{l:?}");
    assert!(!tried(&l, &inc1, "found.h"), "<...> starts after the -iquote dirs: {l:?}");
    assert!(tried(&l, &inc2, "found.h") && !tried(&l, &inc3, "found.h"), "{l:?}");
    assert!(tried(&l, &inc2, "gone.h") && tried(&l, &inc3, "gone.h"), "{l:?}");
    assert!(l.list.contains(&inc1) && l.list.contains(&inc2) && l.list.contains(&inc3));
}

// `__has_include` and `__has_include_next` (in the main file: from the
// start), failed: recorded as `#include` is.
#[test]
fn has_include_and_next_failed() {
    let d = Dir::new("has");
    d.write(
        "a.cpp",
        "#if __has_include(\"p.h\") || __has_include(<r.h>) || __has_include_next(<n.h>)\n#endif\n",
    );
    let inc = d.at("inc");
    std::fs::create_dir_all(&inc).unwrap();
    let flag = format!("-I{inc}");
    let (status, l) = d.lookups("", &[&flag]);
    assert_eq!(status, Status::Ok);
    let root = d.root.to_str().unwrap();
    assert!(tried(&l, root, "p.h") && tried(&l, &inc, "p.h"), "{l:?}");
    assert!(tried(&l, &inc, "r.h") && !tried(&l, root, "r.h"), "{l:?}");
    assert!(tried(&l, &inc, "n.h"), "{l:?}");
}

// `#include_next` and `__has_include_next` from a header found in the
// list: the directories after the one it was found in.
#[test]
fn include_next_continues_after() {
    let d = Dir::new("next");
    d.write("a.cpp", "#include <h.h>\n");
    d.write("one/h.h", "#if __has_include_next(<m.h>)\n#endif\n#include_next <h.h>\n");
    d.write("two/h.h", "\n");
    let (one, two, three) = (d.at("one"), d.at("two"), d.at("three"));
    std::fs::create_dir_all(&three).unwrap();
    let flags = [format!("-I{one}"), format!("-I{two}"), format!("-I{three}")];
    let flags: Vec<&str> = flags.iter().map(String::as_str).collect();
    let (status, l) = d.lookups("", &flags);
    assert_eq!(status, Status::Ok);
    assert!(tried(&l, &two, "m.h") && tried(&l, &three, "m.h"), "{l:?}");
    assert!(!tried(&l, &one, "m.h"), "{l:?}");
    assert!(tried(&l, &two, "h.h") && !tried(&l, &three, "h.h"), "found in two: {l:?}");
}

// `#embed` and `__has_embed`: `"..."` tries the includer's directory and
// `.`; both forms then every `--embed-dir` (the log does not name them;
// Kiln's driver does, `Args::embed_dirs`).
#[test]
fn embed_and_has_embed() {
    let d = Dir::new("embed");
    d.write(
        "a.cpp",
        "#if __has_embed(\"e.bin\") || __has_embed(<f.bin>)\n#endif\nconst char k[] = {\n#embed \"g.bin\"\n};\n",
    );
    d.write("data/g.bin", "x");
    let data = d.at("data");
    let flag = format!("--embed-dir={data}");
    let (status, l) = d.lookups("build", &[&flag]);
    assert_eq!(status, Status::Ok, "{l:?}");
    let root = d.root.to_str().unwrap();
    for name in ["e.bin", "g.bin"] {
        assert!(tried(&l, root, name) && tried(&l, ".", name), "{name}: {l:?}");
    }
    assert!(tried(&l, &data, "e.bin") && tried(&l, &data, "f.bin"), "{l:?}");
    assert!(!tried(&l, root, "f.bin"), "{l:?}");
    assert!(l.read.iter().any(|r| r.ends_with("data/g.bin")), "{l:?}");
}

// `-include`: a `"..."` lookup from the predefines buffer, whose
// includer's directory is `.`.
#[test]
fn dash_include() {
    let d = Dir::new("dash-include");
    d.write("a.cpp", "int x;\n");
    let inc = d.at("inc");
    std::fs::create_dir_all(&inc).unwrap();
    let flag = format!("-I{inc}");
    let (status, l) = d.lookups("build", &[&flag, "-include", "pre.h"]);
    assert_eq!(status, Status::Diagnosed, "pre.h is missing");
    assert!(tried(&l, ".", "pre.h") && tried(&l, &inc, "pre.h"), "{l:?}");
}

// `-imacros`: Kiln's driver does not accept it, so the unit is not parsed
// and nothing is looked up (no `verified` can come of it: the post-image
// is `not_parsed`).
#[test]
fn dash_imacros_is_not_accepted() {
    let d = Dir::new("imacros");
    d.write("a.cpp", "int x;\n");
    d.write("m.h", "#define M 1\n");
    let m = d.at("m.h");
    let (status, l) = d.parse("", &["-imacros", &m]);
    assert_eq!((status, l), (Status::Rejected, None));
}

// An absolute name is not recorded (Kiln `kiln_pp/src/resolver.rs:132`):
// a failed `__has_include` of one is invisible to guard L1. Pinned so
// that the day Kiln records it, this says so (`spec/verify.md` §5.3).
#[test]
fn absolute_name_not_recorded() {
    let d = Dir::new("absolute");
    let gone = d.at("gone.h");
    d.write("a.cpp", &format!("#if __has_include(\"{gone}\")\n#endif\n"));
    let (status, l) = d.lookups("", &[]);
    assert_eq!(status, Status::Ok);
    assert!(!l.tried.iter().any(|(_, n)| n.ends_with("gone.h")), "{l:?}");
}

// A lookup after the first few thousand is recorded (the log is not cut).
#[test]
fn lookup_after_thousands() {
    let d = Dir::new("many");
    let mut text = String::new();
    for i in 0..5000 {
        writeln!(text, "#if __has_include(<n{i}.h>)\n#endif").unwrap();
    }
    d.write("a.cpp", &text);
    let inc = d.at("inc");
    std::fs::create_dir_all(&inc).unwrap();
    let flag = format!("-I{inc}");
    let (status, l) = d.lookups("", &[&flag]);
    assert_eq!(status, Status::Ok);
    assert!(tried(&l, &inc, "n4999.h"), "{} tried", l.tried.len());
}

// A lookup from a header outside the main file's directory: its own
// directory is the includer's.
#[test]
fn lookup_from_another_directory() {
    let d = Dir::new("other");
    d.write("a.cpp", "#include <e.h>\n");
    d.write("ext/e.h", "#if __has_include(\"z.h\")\n#endif\n");
    let ext = d.at("ext");
    let flag = format!("-I{ext}");
    let (status, l) = d.lookups("", &[&flag]);
    assert_eq!(status, Status::Ok);
    assert!(tried(&l, &ext, "z.h"), "{l:?}");
    assert!(l.read.iter().any(|r| Path::new(r).ends_with("ext/e.h")), "{l:?}");
}

// The search list shows the toolchain's directories (the builtin headers
// found from the front end's own location, not from `argv[0]`): they are
// in `list`, where guard L1 checks them.
#[test]
fn toolchain_directories_in_the_list() {
    let d = Dir::new("toolchain");
    d.write("a.cpp", "#include <stddef.h>\n");
    let (status, l) = d.lookups("", &[]);
    assert_eq!(status, Status::Ok, "{l:?}");
    assert!(!l.list.is_empty(), "{l:?}");
    assert!(l.read.iter().any(|r| r.ends_with("stddef.h")), "{l:?}");
}

// ---------------------------------------------------------------------------
// `Lookups::missed`, the negative inputs (work package WP-117,
// `spec/model.md` §8.6): one test per form of `spec/landed/verify/WP-117.md`
// item 1's table.

fn missed(l: &Lookups, dir: &str, name: &str) -> bool {
    l.missed.iter().any(|(d, n)| d == dir && n == name)
}

// N-a. `#include "..."` found on the list: the includer's directory, then
// the list before the directory it was found in; not that one, not after.
#[test]
fn missed_quoted_include_before_the_hit() {
    let d = Dir::new("missed-quoted");
    d.write("a.cpp", "#include \"h.h\"\n");
    d.write("inc2/h.h", "\n");
    let (inc1, inc2, inc3) = (d.at("inc1"), d.at("inc2"), d.at("inc3"));
    for i in [&inc1, &inc3] {
        std::fs::create_dir_all(i).unwrap();
    }
    let flags = [format!("-I{inc1}"), format!("-I{inc2}"), format!("-I{inc3}")];
    let flags: Vec<&str> = flags.iter().map(String::as_str).collect();
    let (status, l) = d.lookups("", &flags);
    assert_eq!(status, Status::Ok, "{l:?}");
    let root = d.root.to_str().unwrap();
    assert!(missed(&l, root, "h.h") && missed(&l, &inc1, "h.h"), "{l:?}");
    assert!(!missed(&l, &inc2, "h.h") && !missed(&l, &inc3, "h.h"), "{l:?}");
}

// N-a. `#include "..."` found in the includer's directory: no negative.
#[test]
fn missed_none_when_found_beside_the_includer() {
    let d = Dir::new("missed-beside");
    d.write("a.cpp", "#include \"h.h\"\n");
    d.write("h.h", "\n");
    let inc = d.at("inc");
    std::fs::create_dir_all(&inc).unwrap();
    let flag = format!("-I{inc}");
    let (status, l) = d.lookups("", &[&flag]);
    assert_eq!(status, Status::Ok, "{l:?}");
    assert!(!l.missed.iter().any(|(_, n)| n == "h.h"), "{l:?}");
}

// N-b. `#include <...>`: the list from the angled start (not the
// `-iquote` directories, not the includer's) to the hit.
#[test]
fn missed_angled_include_from_the_angled_start() {
    let d = Dir::new("missed-angled");
    d.write("a.cpp", "#include <h.h>\n");
    d.write("inc2/h.h", "\n");
    let (q, inc1, inc2) = (d.at("q"), d.at("inc1"), d.at("inc2"));
    for i in [&q, &inc1] {
        std::fs::create_dir_all(i).unwrap();
    }
    let flags = [format!("-iquote{q}"), format!("-I{inc1}"), format!("-I{inc2}")];
    let flags: Vec<&str> = flags.iter().map(String::as_str).collect();
    let (status, l) = d.lookups("", &flags);
    assert_eq!(status, Status::Ok, "{l:?}");
    let root = d.root.to_str().unwrap();
    assert!(missed(&l, &inc1, "h.h"), "{l:?}");
    assert!(!missed(&l, &q, "h.h") && !missed(&l, root, "h.h"), "{l:?}");
    assert!(!missed(&l, &inc2, "h.h"), "{l:?}");
}

// N-c. `__has_include` and `__has_include_next` answering false: every
// place they tried is a negative.
#[test]
fn missed_has_include_false_is_all_negatives() {
    let d = Dir::new("missed-has");
    d.write("a.cpp", "#if __has_include(\"p.h\") || __has_include(<r.h>)\n#endif\n");
    let (inc1, inc2) = (d.at("inc1"), d.at("inc2"));
    for i in [&inc1, &inc2] {
        std::fs::create_dir_all(i).unwrap();
    }
    let flags = [format!("-I{inc1}"), format!("-I{inc2}")];
    let flags: Vec<&str> = flags.iter().map(String::as_str).collect();
    let (status, l) = d.lookups("", &flags);
    assert_eq!(status, Status::Ok);
    let root = d.root.to_str().unwrap();
    for (dir, name) in
        [(root, "p.h"), (&inc1, "p.h"), (&inc2, "p.h"), (&inc1, "r.h"), (&inc2, "r.h")]
    {
        assert!(missed(&l, dir, name), "{dir}/{name}: {l:?}");
    }
    assert!(!missed(&l, root, "r.h"), "{l:?}");
}

// N-d. `#include_next` and `__has_include_next`: the directories after the
// includer's, up to the hit (all of them for a false answer).
#[test]
fn missed_include_next_between_the_two() {
    let d = Dir::new("missed-next");
    d.write("a.cpp", "#include <h.h>\n");
    d.write("one/h.h", "#if __has_include_next(<m.h>)\n#endif\n#include_next <h.h>\n");
    d.write("three/h.h", "\n");
    let (one, two, three) = (d.at("one"), d.at("two"), d.at("three"));
    std::fs::create_dir_all(&two).unwrap();
    let flags = [format!("-I{one}"), format!("-I{two}"), format!("-I{three}")];
    let flags: Vec<&str> = flags.iter().map(String::as_str).collect();
    let (status, l) = d.lookups("", &flags);
    assert_eq!(status, Status::Ok);
    assert!(missed(&l, &two, "h.h") && !missed(&l, &three, "h.h"), "{l:?}");
    assert!(missed(&l, &two, "m.h") && missed(&l, &three, "m.h"), "{l:?}");
    assert!(!missed(&l, &one, "m.h"), "{l:?}");
}

// N-e. `#embed` and `__has_embed`: the tries before the file read (the
// includer's directory, `.` for `"..."`, then each `--embed-dir`); every
// try for a false answer.
#[test]
fn missed_embed_before_the_file_read() {
    let d = Dir::new("missed-embed");
    d.write(
        "a.cpp",
        "#if __has_embed(<f.bin>)\n#endif\nconst char k[] = {\n#embed \"g.bin\"\n};\n",
    );
    d.write("d2/g.bin", "x");
    let (d1, d2, d3) = (d.at("d1"), d.at("d2"), d.at("d3"));
    for i in [&d1, &d3] {
        std::fs::create_dir_all(i).unwrap();
    }
    let flags =
        [format!("--embed-dir={d1}"), format!("--embed-dir={d2}"), format!("--embed-dir={d3}")];
    let flags: Vec<&str> = flags.iter().map(String::as_str).collect();
    let (status, l) = d.lookups("build", &flags);
    assert_eq!(status, Status::Ok, "{l:?}");
    let root = d.root.to_str().unwrap();
    assert!(missed(&l, root, "g.bin") && missed(&l, ".", "g.bin") && missed(&l, &d1, "g.bin"));
    assert!(!missed(&l, &d2, "g.bin") && !missed(&l, &d3, "g.bin"), "{l:?}");
    for dir in [&d1, &d2, &d3] {
        assert!(missed(&l, dir, "f.bin"), "{dir}: {l:?}");
    }
}

// N-f. `-include`: a `"..."` lookup from the predefines buffer, whose
// includer's directory is `.`; found on the list.
#[test]
fn missed_dash_include() {
    let d = Dir::new("missed-dash-include");
    d.write("a.cpp", "int x;\n");
    d.write("inc/pre.h", "\n");
    let inc = d.at("inc");
    let flag = format!("-I{inc}");
    let (status, l) = d.lookups("build", &[&flag, "-include", "pre.h"]);
    assert_eq!(status, Status::Ok, "{l:?}");
    assert!(missed(&l, ".", "pre.h") && !missed(&l, &inc, "pre.h"), "{l:?}");
}

// N-g. A search directory of the command that does not exist (Kiln's
// driver leaves it out of the list): a negative of each name the list was
// searched for where it would be searched were it created, by its place
// in the list. After the hit: none; through the end for a name not found;
// an `-iquote` one only for `"..."`. Spelled as the command spells it
// (here relative).
#[test]
fn missed_absent_search_directories() {
    let d = Dir::new("missed-absent");
    d.write("a.cpp", "#include <h.h>\n#include \"k.h\"\n#if __has_include(<z.h>)\n#endif\n");
    d.write("inc/h.h", "\n");
    d.write("inc/k.h", "\n");
    let inc = d.at("inc");
    let flags = [format!("-I{inc}"), "-Ilater".to_owned(), "-iquotequote".to_owned()];
    let flags: Vec<&str> = flags.iter().map(String::as_str).collect();
    let (status, l) = d.lookups("", &flags);
    assert_eq!(status, Status::Ok, "{l:?}");
    assert!(!l.list.iter().any(|x| x.ends_with("later")), "{l:?}");
    assert!(!missed(&l, "later", "h.h") && !missed(&l, "later", "k.h"), "{l:?}");
    assert!(missed(&l, "later", "z.h"), "{l:?}");
    assert!(missed(&l, "quote", "k.h") && !missed(&l, "quote", "h.h"), "{l:?}");
    assert!(!missed(&l, "quote", "z.h"), "{l:?}");
}

// N-g. Before the hit: a negative.
#[test]
fn missed_absent_before_the_hit() {
    let d = Dir::new("missed-absent-before");
    d.write("a.cpp", "#include <h.h>\n");
    d.write("inc/h.h", "\n");
    let (status, l) = d.lookups("", &["-Igen", "-Iinc"]);
    assert_eq!(status, Status::Ok, "{l:?}");
    assert!(missed(&l, "gen", "h.h"), "{l:?}");
}

// N-g. `-iquote`: by its place among the `-iquote` directories, and never
// for `<...>`.
#[test]
fn missed_absent_iquote_position() {
    let d = Dir::new("missed-absent-iquote");
    d.write("a.cpp", "#include \"k.h\"\n#include \"m.h\"\n");
    d.write("q1/k.h", "\n");
    d.write("q3/m.h", "\n");
    let (status, l) = d.lookups("build", &["-iquote../q1", "-iquote../q2", "-iquote../q3"]);
    assert_eq!(status, Status::Ok, "{l:?}");
    assert!(!missed(&l, "../q2", "k.h"), "{l:?}");
    assert!(missed(&l, "../q2", "m.h"), "{l:?}");
}

// N-g. `-isystem`: by its place among the system directories (after
// every `-I`, the command's before the toolchain's).
#[test]
fn missed_absent_isystem_position() {
    let d = Dir::new("missed-absent-isystem");
    d.write("a.cpp", "#include <h.h>\n#include <g.h>\n");
    d.write("sb/h.h", "\n");
    d.write("sd/g.h", "\n");
    let (status, l) = d.lookups("", &["-isystemsa", "-isystemsb", "-isystemsc", "-isystemsd"]);
    assert_eq!(status, Status::Ok, "{l:?}");
    assert!(missed(&l, "sa", "h.h") && !missed(&l, "sc", "h.h"), "{l:?}");
    assert!(missed(&l, "sa", "g.h") && missed(&l, "sc", "g.h"), "{l:?}");
}

// N-g. `-isystem` and a hit in an `-I` directory: created as that
// directory, the system one would take its place (clang
// `RemoveDuplicates`), so it is a negative although it comes after.
#[test]
fn missed_absent_isystem_after_a_user_hit() {
    let d = Dir::new("missed-absent-displace");
    d.write("a.cpp", "#include <h.h>\n");
    d.write("inc/h.h", "\n");
    let (status, l) = d.lookups("", &["-Iinc", "-isystemsys", "-Ilater"]);
    assert_eq!(status, Status::Ok, "{l:?}");
    assert!(missed(&l, "sys", "h.h") && !missed(&l, "later", "h.h"), "{l:?}");
}

// N-g. `-idirafter`: after the toolchain's directories; before a later
// `-idirafter` one.
#[test]
fn missed_absent_idirafter_position() {
    let d = Dir::new("missed-absent-after");
    d.write(
        "a.cpp",
        "#include <h.h>\n#include <g.h>\n#if __has_include(<cstddef>)\n#endif\n#if __has_include(<z.h>)\n#endif\n",
    );
    d.write("sb/h.h", "\n");
    d.write("a2/g.h", "\n");
    let (status, l) = d.lookups("", &["-isystemsb", "-idirafteraft", "-idirafter", "a2"]);
    assert_eq!(status, Status::Ok, "{l:?}");
    assert!(!missed(&l, "aft", "h.h"), "{l:?}");
    // Found in a toolchain directory (libstdc++'s), which comes before
    // every `-idirafter` one.
    assert!(!missed(&l, "aft", "cstddef"), "{l:?}");
    assert!(missed(&l, "aft", "g.h") && missed(&l, "aft", "z.h"), "{l:?}");
}

// N-g. `-cxx-isystem` in C++: a system directory, in the order of the
// words (Kiln's C++ list).
#[test]
fn missed_absent_cxx_isystem_placed() {
    let d = Dir::new("missed-absent-cxx");
    d.write("a.cpp", "#include <h.h>\n#if __has_include(<z.h>)\n#endif\n");
    d.write("sb/h.h", "\n");
    let (status, l) = d.lookups("", &["-isystemsb", "-cxx-isystem", "cx"]);
    assert_eq!(status, Status::Ok, "{l:?}");
    assert!(!missed(&l, "cx", "h.h") && missed(&l, "cx", "z.h"), "{l:?}");
}

// N-g. Kiln's driver accepts no `-c-isystem` word: the unit is
// `rejected`, so no such directory reaches the placement.
#[test]
fn missed_absent_c_isystem_is_not_accepted() {
    let d = Dir::new("missed-absent-c");
    d.write("a.cpp", "#include <h.h>\n");
    d.write("sb/h.h", "\n");
    let (status, l) = d.parse("", &["-isystemsb", "-c-isystem", "cs"]);
    assert_eq!((status, l), (Status::Rejected, None));
}

// N-g. An `-iquote` directory that is also a toolchain one is listed
// twice (Kiln deduplicates the `-iquote` part apart): the log's list is
// then not the command's own with one toolchain block, and no directory
// is placed (`later`, after the hit, is still a negative).
#[test]
fn missed_absent_iquote_toolchain_directory_unplaced() {
    let d = Dir::new("missed-absent-iquote-tc");
    d.write("a.cpp", "#include <h.h>\n");
    d.write("inc/h.h", "\n");
    let (status, l) = d.lookups("", &["-iquote/usr/include", "-Iinc", "-Ilater"]);
    assert_eq!(status, Status::Ok, "{l:?}");
    let twice = l.list.iter().filter(|x| *x == "/usr/include").count();
    assert_eq!(twice, 2, "the host's toolchain lists /usr/include: {l:?}");
    assert!(missed(&l, "later", "h.h"), "{l:?}");
}

// N-g. `#include_next`: an absent directory is placed in the window after
// the includer's directory (`gen`, between `one` and `three`), not before
// it (`before`).
#[test]
fn missed_absent_in_the_include_next_window() {
    let d = Dir::new("missed-absent-next");
    d.write("a.cpp", "#include <h.h>\n");
    d.write("one/h.h", "#if __has_include_next(<m.h>)\n#endif\n#include_next <h.h>\n");
    d.write("three/h.h", "\n");
    d.write("three/m.h", "\n");
    let (one, three) = (d.at("one"), d.at("three"));
    let flags =
        ["-Ibefore".to_owned(), format!("-I{one}"), "-Igen".to_owned(), format!("-I{three}")];
    let flags: Vec<&str> = flags.iter().map(String::as_str).collect();
    let (status, l) = d.lookups("", &flags);
    assert_eq!(status, Status::Ok, "{l:?}");
    assert!(missed(&l, "gen", "h.h") && missed(&l, "gen", "m.h"), "{l:?}");
    assert!(!missed(&l, "before", "m.h"), "{l:?}");
}

// N-g. One directory named by `-isystem` and `-idirafter`: the list cannot
// tell which part it would be in (the system part's end and the
// `-idirafter` part's start are one place in the command's list): both
// are kept, so a name found in a toolchain directory (between them) is a
// negative there.
#[test]
fn missed_absent_isystem_and_idirafter_one_directory() {
    let d = Dir::new("missed-absent-both");
    d.write("a.cpp", "#if __has_include(<cstddef>)\n#endif\n");
    let (status, l) = d.lookups("", &["-isystemx", "-idirafterx"]);
    assert_eq!(status, Status::Ok, "{l:?}");
    assert!(missed(&l, "x", "cstddef"), "{l:?}");
}

// N-g. A search directory Kiln dropped as a duplicate of a listed one
// (`lnk -> a`, after `a`): a place as an absent one is, in its window.
#[test]
fn missed_duplicate_directory() {
    let d = Dir::new("missed-duplicate");
    d.write("a.cpp", "#include <h.h>\n");
    d.write("ext/h.h", "\n");
    std::fs::create_dir_all(d.root.join("a")).unwrap();
    std::os::unix::fs::symlink("a", d.root.join("lnk")).unwrap();
    let (status, l) = d.lookups("", &["-Ia", "-Ilnk", "-Iext"]);
    assert_eq!(status, Status::Ok, "{l:?}");
    assert!(!l.list.iter().any(|x| x == "lnk"), "{l:?}");
    assert!(missed(&l, "lnk", "h.h"), "{l:?}");
}

// N-g. A dropped duplicate before its twin (`-Ilnk -isystem b`, `lnk ->
// b`: clang drops the user one): its window reaches the twin, where the
// lookup found the name; that name is a file in it now, so no negative.
#[test]
fn missed_duplicate_directory_holding_the_name() {
    let d = Dir::new("missed-duplicate-holds");
    d.write("a.cpp", "#include <k.h>\n#if __has_include(<z.h>)\n#endif\n");
    d.write("b/k.h", "\n");
    std::os::unix::fs::symlink("b", d.root.join("lnk")).unwrap();
    let (status, l) = d.lookups("", &["-Ilnk", "-isystemb"]);
    assert_eq!(status, Status::Ok, "{l:?}");
    assert!(!l.list.iter().any(|x| x == "lnk"), "{l:?}");
    assert!(!missed(&l, "lnk", "k.h"), "{l:?}");
    assert!(missed(&l, "lnk", "z.h"), "{l:?}");
}

// N-h. An absolute name: Kiln's log does not record it yet (Kiln
// WP-2620): when it does, its own path is the negative of a false answer.
#[test]
#[ignore = "needs: kiln (WP-2620: absolute names in the lookup log)"]
fn missed_absolute_name() {
    let d = Dir::new("missed-absolute");
    let gone = d.at("gone.h");
    d.write("a.cpp", &format!("#if __has_include(\"{gone}\")\n#endif\n"));
    let (status, l) = d.lookups("", &[]);
    assert_eq!(status, Status::Ok);
    assert!(missed(&l, "/", &gone), "{l:?}");
}
