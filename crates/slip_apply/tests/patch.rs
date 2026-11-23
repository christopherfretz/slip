//! `spec/edits.md` §7.2: the overlay as a unified diff. Its form (headers,
//! hunks, context, merging, `\ No newline at end of file`), what it
//! refuses, and the property: `patch(1)` and `git apply` applied to the
//! pre-image give the post-image, byte for byte (each skipped, and
//! counted, where the tool is missing).

mod common;

use std::collections::BTreeMap;
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use common::{Mem, Rng};
use slip_apply::{ApplyError, patch};
use slip_model::Digest;
use slip_model::hash::content_digest;
use slip_model::overlay::{Content, Overlay, OverlayFile};

fn content(b: &[u8]) -> Content {
    Content { size: b.len() as u64, content: content_digest(b) }
}

/// An overlay of `(path, pre, post)` files, the pre-image and the blobs.
fn overlay(files: &[(&[u8], &[u8], &[u8])]) -> (Overlay, Mem, BTreeMap<Digest, Vec<u8>>) {
    let mut files = files.to_vec();
    files.sort();
    let overlay = Overlay {
        edits: Digest([4; 32]),
        files: files
            .iter()
            .map(|(p, a, b)| OverlayFile { path: p.to_vec(), pre: content(a), post: content(b) })
            .collect(),
        spellings: vec![],
    };
    let mem = Mem(files.iter().map(|(p, a, _)| (p.to_vec(), a.to_vec())).collect());
    let blobs = files.iter().map(|(_, _, b)| (content_digest(b), b.to_vec())).collect();
    (overlay, mem, blobs)
}

fn diff(files: &[(&[u8], &[u8], &[u8])], rel: Option<&[u8]>) -> Result<Vec<u8>, ApplyError> {
    let (o, mem, blobs) = overlay(files);
    let get =
        |d: Digest| blobs.get(&d).cloned().ok_or_else(|| io::Error::from(io::ErrorKind::NotFound));
    let mut out = Vec::new();
    patch(&o, &mem, &get, rel, &mut out)?;
    Ok(out)
}

fn text(b: &[u8]) -> String {
    String::from_utf8_lossy(b).into_owned()
}

// §7.2: one section per changed file in path order, `---`/`+++` with the
// path, three lines of context.
#[test]
fn s7_2_form_of_a_section() {
    let pre = b"1\n2\n3\n4\n5\n6\n7\n8\n";
    let post = b"1\n2\n3\n4x\n5\n6\n7\n8\n";
    let out = diff(
        &[(b"/t/b", pre, post), (b"/t/a", b"x\n", b"y\n"), (b"/t/c", b"same\n", b"same\n")],
        None,
    )
    .unwrap();
    assert_eq!(
        text(&out),
        "--- /t/a\n+++ /t/a\n@@ -1 +1 @@\n-x\n+y\n\
         --- /t/b\n+++ /t/b\n@@ -1,7 +1,7 @@\n 1\n 2\n 3\n-4\n+4x\n 5\n 6\n 7\n"
    );
}

// §7.2: hunks are merged when their context touches or overlaps: six
// equal lines between two changes make one hunk, seven make two.
#[test]
fn s7_2_hunks_merge_when_context_touches() {
    let lines = |n: usize, change: &[usize]| -> Vec<u8> {
        (0..n)
            .flat_map(|i| {
                if change.contains(&i) { format!("{i}x\n") } else { format!("{i}\n") }.into_bytes()
            })
            .collect()
    };
    let pre = lines(20, &[]);
    let one = diff(&[(b"/t/a", &pre, &lines(20, &[3, 10]))], None).unwrap();
    assert_eq!(text(&one).matches("@@ -").count(), 1, "{}", text(&one));
    let two = diff(&[(b"/t/a", &pre, &lines(20, &[3, 11]))], None).unwrap();
    assert_eq!(text(&two).matches("@@ -").count(), 2, "{}", text(&two));
    assert!(
        text(&two).contains("@@ -1,7 +1,7 @@") && text(&two).contains("@@ -9,7 +9,7 @@"),
        "{}",
        text(&two)
    );
}

// §7.2: a last line without `\n` is followed by `\ No newline at end of
// file`, on the side that lacks it; a `\r` is part of its line.
#[test]
fn s7_2_no_final_newline_and_crlf() {
    let out = diff(&[(b"/t/a", b"a\nb", b"a\nb\n")], None).unwrap();
    assert_eq!(
        text(&out),
        "--- /t/a\n+++ /t/a\n@@ -1,2 +1,2 @@\n a\n-b\n\\ No newline at end of file\n+b\n"
    );
    let out = diff(&[(b"/t/a", b"a\nb", b"A\nb")], None).unwrap();
    assert_eq!(
        text(&out),
        "--- /t/a\n+++ /t/a\n@@ -1,2 +1,2 @@\n-a\n+A\n b\n\\ No newline at end of file\n"
    );
    let out = diff(&[(b"/t/a", b"a\r\nb\r\n", b"a\nb\r\n")], None).unwrap();
    assert_eq!(text(&out), "--- /t/a\n+++ /t/a\n@@ -1,2 +1,2 @@\n-a\r\n+a\n b\r\n");
    // Emptied and filled.
    let out = diff(&[(b"/t/a", b"a\n", b"")], None).unwrap();
    assert_eq!(text(&out), "--- /t/a\n+++ /t/a\n@@ -1 +0,0 @@\n-a\n");
    let out = diff(&[(b"/t/a", b"", b"a")], None).unwrap();
    assert_eq!(text(&out), "--- /t/a\n+++ /t/a\n@@ -0,0 +1 @@\n+a\n\\ No newline at end of file\n");
}

// §7.2: `--relative-to`; a file outside the directory, a name a header
// cannot carry and a directory that is not canonical are `Invalid`, and
// nothing is written.
#[test]
fn s7_2_relative_to_and_names() {
    let f: &[(&[u8], &[u8], &[u8])] = &[(b"/t/src/a.cc", b"x\n", b"y\n")];
    assert!(text(&diff(f, Some(b"/t")).unwrap()).starts_with("--- src/a.cc\n+++ src/a.cc\n"));
    assert!(text(&diff(f, Some(b"/")).unwrap()).starts_with("--- t/src/a.cc\n"));
    // Bytes that are not UTF-8 are written as they are.
    let raw: &[(&[u8], &[u8], &[u8])] = &[(b"/t/\xff\xfe.c", b"x\n", b"y\n")];
    assert!(diff(raw, None).unwrap().starts_with(b"--- /t/\xff\xfe.c\n+++ /t/\xff\xfe.c\n@@"));
    // A name with a space ends at a tab.
    let sp: &[(&[u8], &[u8], &[u8])] = &[(b"/t/a b", b"x\n", b"y\n")];
    assert!(text(&diff(sp, None).unwrap()).starts_with("--- /t/a b\t\n+++ /t/a b\t\n@@"));
    for bad in [&b"/t/sr"[..], b"/u", b"/t/src/a.cc", b"t", b"/t/"] {
        assert!(matches!(diff(f, Some(bad)), Err(ApplyError::Invalid(_))), "{}", text(bad));
    }
    for p in [&b"/t/a\nb"[..], b"/t/a\tb", b"/t/a\rb", b"/t/\"q"] {
        let (o, mem, blobs) = overlay(&[(b"/t/ok", b"1\n", b"2\n"), (p, b"x\n", b"y\n")]);
        let mut out = Vec::new();
        let get = |d: Digest| Ok(blobs[&d].clone());
        assert!(matches!(
            patch(&o, &mem, &get, Some(b"/t"), &mut out),
            Err(ApplyError::Invalid(_))
        ));
        assert!(out.is_empty());
    }
}

// §7.2: the pre-image must be the overlay's; a blob must hash to its name.
#[test]
fn s7_2_stale_pre_image_and_bad_blob() {
    let (o, mut mem, blobs) = overlay(&[(b"/t/a", b"x\n", b"y\n")]);
    let get = |d: Digest| Ok(blobs[&d].clone());
    mem.0.insert(b"/t/a".to_vec(), b"z\n".to_vec());
    assert_eq!(
        patch(&o, &mem, &get, None, &mut Vec::new()),
        Err(ApplyError::StaleFile(b"/t/a".to_vec()))
    );
    let (o, mem, _) = overlay(&[(b"/t/a", b"x\n", b"y\n")]);
    let wrong = |_: Digest| Ok(b"w\n".to_vec());
    assert!(matches!(patch(&o, &mem, &wrong, None, &mut Vec::new()), Err(ApplyError::Io(..))));
}

// ---------------------------------------------------------------------------
// The property, through the tools.

fn scratch(name: &str) -> PathBuf {
    let d = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("wp063-patch-{name}-{}", std::process::id()));
    if d.exists() {
        std::fs::remove_dir_all(&d).unwrap();
    }
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn have(tool: &str) -> bool {
    Command::new(tool)
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// Writes the pre-image under `dir`, applies `diff` with the tool, and
/// returns each file's bytes after, or the tool's complaint.
fn apply_with(
    tool: &str,
    dir: &Path,
    strip: &str,
    files: &[(Vec<u8>, Vec<u8>)],
    diff: &[u8],
) -> Result<Vec<Vec<u8>>, String> {
    for (p, pre) in files {
        let at = dir.join(std::ffi::OsStr::from_bytes(p));
        std::fs::create_dir_all(at.parent().unwrap()).unwrap();
        std::fs::write(&at, pre).unwrap();
    }
    let patch_file = dir.join("the.patch");
    std::fs::write(&patch_file, diff).unwrap();
    let mut cmd = if tool == "patch" {
        let mut c = Command::new("patch");
        c.args([strip, "--batch", "--forward", "--no-backup-if-mismatch", "-s", "-i"])
            .arg(&patch_file);
        c
    } else {
        let mut c = Command::new("git");
        // Not inside the repository the target directory lies in.
        c.env("GIT_CEILING_DIRECTORIES", dir.parent().unwrap())
            .args(["apply", strip])
            .arg(&patch_file);
        c
    };
    if diff.is_empty() {
        cmd = Command::new("true");
    }
    let out = cmd.current_dir(dir).output().unwrap();
    std::fs::remove_file(&patch_file).unwrap();
    if !out.status.success() {
        return Err(format!("{tool}: {}{}", text(&out.stdout), text(&out.stderr)));
    }
    Ok(files
        .iter()
        .map(|(p, _)| std::fs::read(dir.join(std::ffi::OsStr::from_bytes(p))).unwrap())
        .collect())
}

/// A random file: lines of a small alphabet, CRLF and LF endings, maybe
/// no final newline.
fn random_file(rng: &mut Rng, alphabet: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    for _ in 0..rng.below(30) {
        out.extend(rng.bytes_n(0, 4, alphabet));
        out.extend_from_slice(if rng.chance(1, 4) { b"\r\n" } else { b"\n" });
    }
    if rng.chance(1, 3) {
        out.extend(rng.bytes_n(1, 3, alphabet));
    }
    out
}

/// `pre` with random lines changed, removed, added, or the final newline flipped.
fn mutate(rng: &mut Rng, pre: &[u8], alphabet: &[u8]) -> Vec<u8> {
    let mut lines: Vec<Vec<u8>> =
        pre.split_inclusive(|&b| b == b'\n').map(<[u8]>::to_vec).collect();
    for _ in 0..=rng.below(4) {
        let at = rng.below(lines.len() as u64 + 1) as usize;
        match rng.below(4) {
            0 if at < lines.len() => {
                lines.remove(at);
            }
            1 if at < lines.len() => {
                lines[at] = [rng.bytes_n(1, 3, alphabet), b"\n".to_vec()].concat();
            }
            2 => lines.insert(at, [rng.bytes_n(0, 3, alphabet), b"\n".to_vec()].concat()),
            _ => {
                if let Some(last) = lines.last_mut() {
                    if last.last() == Some(&b'\n') {
                        last.pop();
                    } else {
                        last.push(b'\n');
                    }
                }
            }
        }
    }
    lines.concat()
}

fn round_trip(tool: &str, alphabet: &[u8], cases: usize, seed: u64) -> usize {
    let dir = scratch(&format!("{tool}-{seed}"));
    let mut rng = Rng(seed);
    let mut checked = 0;
    for case in 0..cases {
        let n = 1 + rng.below(3) as usize;
        let mut files: Vec<(Vec<u8>, Vec<u8>, Vec<u8>)> = Vec::new();
        for k in 0..n {
            let pre = random_file(&mut rng, alphabet);
            let post = mutate(&mut rng, &pre, alphabet);
            let mut name = format!("/w/c{case}/d{k}/f").into_bytes();
            // A space, and not: the volume the tests run on refuses names that
            // are not UTF-8, so those are checked in the header test only.
            name.extend_from_slice(&[[b' ', b'_'][k % 2], b'0' + k as u8]);
            name.extend_from_slice(b".cc");
            files.push((name, pre, post));
        }
        let view: Vec<(&[u8], &[u8], &[u8])> =
            files.iter().map(|(p, a, b)| (&p[..], &a[..], &b[..])).collect();
        // Relative to /w with -p0, and absolute with -p1.
        for (rel, strip) in [(Some(&b"/w"[..]), "-p0"), (None, "-p1")] {
            let d = diff(&view, rel).unwrap();
            if d.is_empty() {
                // No file changed: a tool refuses an empty patch.
                assert!(files.iter().all(|(_, a, b)| a == b));
            }
            let on_disk: Vec<(Vec<u8>, Vec<u8>)> = files
                .iter()
                .map(|(p, a, _)| (p[if rel.is_some() { 3 } else { 1 }..].to_vec(), a.clone()))
                .collect();
            let got = apply_with(tool, &dir, strip, &on_disk, &d)
                .unwrap_or_else(|e| panic!("{e}\n{}", text(&d)));
            for ((p, a, b), g) in files.iter().zip(&got) {
                assert_eq!(g, b, "{tool} {strip} on {}: pre {:?}\n{}", text(p), text(a), text(&d));
            }
            checked += 1;
        }
        std::fs::remove_dir_all(dir.join(format!("c{case}"))).unwrap();
        std::fs::remove_dir_all(dir.join("w")).unwrap();
    }
    std::fs::remove_dir_all(&dir).unwrap();
    checked
}

// §7.2, the property: `patch -p0` (relative) and `patch -p1` (absolute
// paths) applied to the pre-image give the post-image, for generated
// files with CRLF lines, missing final newlines and names with spaces.
#[test]
fn s7_2_patch_applies_with_patch_1() {
    if !have("patch") {
        eprintln!("skipped 1: no patch(1)");
        return;
    }
    assert_eq!(round_trip("patch", b"ab \t{}", 150, 0x63_0001), 300);
}

// §7.2, the same property through a second tool, `git apply`.
#[test]
fn s7_2_patch_applies_with_git_apply() {
    if !have("git") {
        eprintln!("skipped 1: no git");
        return;
    }
    assert_eq!(round_trip("git", b"ab \t{}", 150, 0x63_0002), 300);
}

// §7.2 and §7.4: a file with NUL bytes or invalid UTF-8 is written the
// same way, its bytes copied. What each tool does with it: GNU patch
// applies it; git apply applies it too (a traditional diff carries no
// binary marker). Recorded in spec/landed/edits/WP-063.md §3.
#[test]
fn s7_2_nul_and_invalid_utf8() {
    let pre: &[u8] = b"a\0b\n\xff\xfe\n\xc3\x28 ok\n";
    let post: &[u8] = b"a\0B\n\xff\xfe\n\xc3\x28 ok\nmore\0\n";
    let d = diff(&[(b"/w/bin", pre, post)], Some(b"/w")).unwrap();
    assert!(d.windows(4).any(|w| w == b"-a\0b"));
    for tool in ["patch", "git"] {
        if !have(tool) {
            eprintln!("skipped 1: no {tool}");
            continue;
        }
        let dir = scratch(&format!("bin-{tool}"));
        let got = apply_with(tool, &dir, "-p0", &[(b"bin".to_vec(), pre.to_vec())], &d);
        assert_eq!(got, Ok(vec![post.to_vec()]), "{tool}");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}

// §7.2: a large file with a few scattered changes gives small hunks, not
// one hunk of the whole file (the anchors), and a file of repeated lines
// still gives a correct, bounded diff (Myers).
#[test]
fn s7_2_large_files() {
    let pre: Vec<u8> = (0..20_000).flat_map(|i| format!("line {i}\n").into_bytes()).collect();
    let post = String::from_utf8(pre.clone())
        .unwrap()
        .replace("line 500\n", "LINE 500\n")
        .replace("line 19000\n", "");
    let d = diff(&[(b"/w/big", &pre, post.as_bytes())], None).unwrap();
    assert_eq!(text(&d).matches("@@ -").count(), 2);
    assert!(d.len() < 400, "{}", text(&d));
    let same: Vec<u8> = b"x\n".repeat(3000);
    let mut changed = same.clone();
    changed.splice(3000..3000, b"y\n".iter().copied());
    let d = diff(&[(b"/w/rep", &same, &changed)], None).unwrap();
    assert!(d.len() < 100, "{}", text(&d));
    // No line unique on either side, and the ends differ: only Myers
    // finds the five common lines (one line out, one in).
    let a: &[u8] = b"p\nq\np\nq\np\nq\n";
    let b: &[u8] = b"q\np\nq\np\nq\np\n";
    let d = text(&diff(&[(b"/w/pq", a, b)], None).unwrap());
    let body: Vec<&str> = d.lines().skip(3).collect();
    assert_eq!(body.iter().filter(|l| l.starts_with('-')).count(), 1, "{d}");
    assert_eq!(body.iter().filter(|l| l.starts_with('+')).count(), 1, "{d}");
}
