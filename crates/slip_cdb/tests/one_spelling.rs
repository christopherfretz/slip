//! WP-147 item 1, the inventory (`spec/landed/edits/WP-147.md`): where a
//! path of the compile database becomes a real path (`spec/cdb.md` §5
//! step 5, §7 step 1), on a volume that finds a name in any case and
//! keeps the spelling it was asked with (this workspace's host mount).
//!
//! The trees are written under `CARGO_TARGET_TMPDIR` (the target
//! directory, on the workspace's volume; `std::env::temp_dir()` is the
//! container's own, case-sensitive one). Each test skips, saying so, on a
//! case-sensitive volume, where another case names no file. Each asserts
//! what WP-147 item 2 built; the inventory states what they gave before.

use std::path::{Path, PathBuf};

use slip_cdb::check::ByExtension;
use slip_cdb::normalise::normalise;
use slip_cdb::plan::{Source, Tool, build};

/// A directory of its own under the target directory, removed on drop by
/// its path (retrying the mount's occasional `ENOTEMPTY` on a directory
/// just emptied, as `slip_script`'s tests do).
struct Dir(PathBuf);

impl Dir {
    fn new(name: &str) -> Dir {
        let p = Path::new(env!("CARGO_TARGET_TMPDIR"))
            .join(format!("slip-cdb-one-spelling-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        Dir(std::fs::canonicalize(&p).unwrap())
    }

    fn write(&self, rel: &str, text: &str) -> PathBuf {
        let p = self.0.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, text).unwrap();
        p
    }

    fn abs(&self, rel: &str) -> String {
        self.0.join(rel).to_str().unwrap().to_owned()
    }

    /// The volume finds `rel` (which exists) under `other` (another case).
    fn folds(&self, other: &str, test: &str) -> bool {
        let found = self.0.join(other).exists();
        if !found {
            eprintln!(
                "{test}: skipped: {} is on a case-sensitive volume, where `{other}` names no file",
                self.0.display()
            );
        }
        found
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        for _ in 0..20 {
            match std::fs::remove_dir_all(&self.0) {
                Err(e) if e.kind() == std::io::ErrorKind::DirectoryNotEmpty => {
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
                _ => return,
            }
        }
    }
}

fn strings(words: &[&str]) -> Vec<String> {
    words.iter().map(|w| (*w).to_owned()).collect()
}

/// `spec/cdb.md` §5 step 5.3 (WP-147 inventory row C1): the main file's
/// real path is the spelling on disk, whatever case the entry's
/// `directory` and input are written in. Before WP-147 `main`
/// was `<root>/PROJ/src/A.cc`, the spelling asked.
#[test]
fn s5_main_file_has_the_spelling_on_disk() {
    let d = Dir::new("main");
    d.write("proj/src/a.cc", "int a;\n");
    if !d.folds("PROJ/src/A.cc", "s5_main_file_has_the_spelling_on_disk") {
        return;
    }
    let n = normalise(&d.abs("PROJ"), &strings(&["cc", "-c", "src/A.cc"]), "src/A.cc")
        .expect("an accepted entry");
    assert_eq!(n.main, Path::new(&d.abs("proj/src/a.cc")), "the spelling on disk");
}

/// `spec/cdb.md` §5 step 5.4 (row C2): an entry whose `file` names the
/// input in another case than the command line is one file, not
/// `malformed_entry`. Before WP-147 it was excluded, "file does not match the
/// command", because the two real paths keep the two spellings.
#[test]
fn s5_file_in_another_case_matches_the_input() {
    let d = Dir::new("match");
    d.write("proj/a.cc", "int a;\n");
    if !d.folds("proj/A.cc", "s5_file_in_another_case_matches_the_input") {
        return;
    }
    let got = normalise(&d.abs("proj"), &strings(&["cc", "-c", "a.cc"]), "A.cc");
    let n = got.unwrap_or_else(|x| panic!("excluded: {x:?}"));
    assert_eq!(n.main, Path::new(&d.abs("proj/a.cc")));
}

/// `spec/cdb.md` §7 steps 1 and 4 (rows C3, C4): the database's
/// `FileRef` and each observer's main file name the file by its
/// spelling on disk. Two entries whose `directory` differs only in case
/// are two commands (the command is replayed as written), and so two
/// observers, but of one main file under one name. Before WP-147 the
/// database was `<root>/DB.json` and the mains were `<root>/proj/a.cc` and
/// `<root>/PROJ/a.cc`: one file under two names, hashed twice.
#[test]
fn s7_plan_names_each_file_once() {
    let d = Dir::new("plan");
    d.write("proj/a.cc", "int a;\n");
    if !d.folds("PROJ/a.cc", "s7_plan_names_each_file_once") {
        return;
    }
    let entry = |dir: &str| {
        format!(
            "{{\"directory\": \"{}\", \"file\": \"a.cc\", \"arguments\": [\"cc\", \"-c\", \"a.cc\"]}}",
            d.abs(dir)
        )
    };
    d.write("db.json", &format!("[{}, {}]", entry("proj"), entry("PROJ")));
    let sources = [Source { name: "c".into(), path: PathBuf::from(d.abs("DB.json")) }];
    let tool = Tool { slip: "test".into(), kiln: "test".into() };
    let plan = build(&sources, &ByExtension, &tool, 1).expect("a plan");
    let text = |b: &[u8]| String::from_utf8_lossy(b).into_owned();
    assert_eq!(plan.observers.len(), 2, "two commands: {:?}", plan.excluded);
    let mains: Vec<String> = plan.observers.iter().map(|o| text(&o.main.path)).collect();
    assert_eq!(mains, [d.abs("proj/a.cc"), d.abs("proj/a.cc")], "one name for the main file");
    assert_eq!(text(&plan.configurations[0].database.path), d.abs("db.json"));
}
