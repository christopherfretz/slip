//! `spec/exec.md` §5.1, §5.2: the worker protocol and `serve`, plus the
//! rules of §5.5. Messages are checked against
//! hand-written bytes; `serve` runs in process against in-memory streams and
//! as the `slip-fake-worker` child process. Every child is killed and reaped
//! on every path ([`Worker`]'s `Drop`), and every wait is bounded.
#![cfg(target_os = "linux")]

mod common;

use std::fs;
use std::io::{self, Cursor};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, channel};
use std::time::Duration;

use slip_exec::store::{Access, Store};
use slip_exec::worker::{
    FRAME_MAX, Job, PROTOCOL, Post, PostImage, ServeOptions, Task, ToCoordinator, ToWorker,
    cpu_ns_from_stat, read_frame, serve, write_frame,
};
use slip_model::wire::ErrorKind;
use slip_model::{
    Command as Cmd, Digest, FileRef, Language, Observer, Origin, Outcome, Phase, Shard, Status,
};

// ---------------------------------------------------------------------------
// Helpers

struct Scratch(PathBuf);

impl std::ops::Deref for Scratch {
    type Target = Path;
    fn deref(&self) -> &Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn scratch(tag: &str) -> Scratch {
    static N: AtomicUsize = AtomicUsize::new(0);
    let n = N.fetch_add(1, Ordering::SeqCst);
    let d = common::work_root().join(format!("slip-exec-worker-{}-{tag}-{n}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    Scratch(d)
}

fn d(n: u8) -> Digest {
    Digest([n; 32])
}

fn hex(parts: &[&str]) -> Vec<u8> {
    let text: String = parts.concat().chars().filter(|c| !c.is_whitespace()).collect();
    (0..text.len()).step_by(2).map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap()).collect()
}

fn rep(byte: &str, n: usize) -> String {
    byte.repeat(n)
}

/// An observer whose command is `argv` in "/".
fn observer(argv: &[&str]) -> Observer {
    let command = Cmd {
        directory: b"/".to_vec(),
        argv: argv.iter().map(|a| a.as_bytes().to_vec()).collect(),
        env: vec![],
    };
    Observer {
        id: command.observer_id(),
        command,
        language: Language::C,
        main: FileRef { path: b"/m".to_vec(), size: 2, content: d(0x22) },
        cost_hint: 3,
        origins: vec![Origin { configuration: 0, entry: 0 }],
    }
}

/// A job for the fake worker with behaviour `behaviour`.
fn job(store: &Path, behaviour: &str) -> Job {
    Job {
        store: store.as_os_str().as_encoded_bytes().to_vec(),
        plan: d(0x44),
        phase: Phase::Map,
        round: 0,
        observer: observer(&["fake", behaviour]),
        transform: None,
        post: None,
    }
}

// ---------------------------------------------------------------------------
// Messages (§5.2 table)

/// `cc` in "/": the observer of `spec/model.md`'s small plan.
const SMALL_OBSERVER_ID: &str = "112055dffe433489c8f0464593c633506997b3e837d21c61ee80710b9ce3ce45";

#[test]
fn s5_2_messages_hand_written_bytes() {
    let job = Job {
        store: b"/s".to_vec(),
        plan: d(0x44),
        phase: Phase::Map,
        round: 0,
        observer: observer(&["cc"]),
        transform: None,
        post: None,
    };
    assert_eq!(job.observer.id.to_string(), SMALL_OBSERVER_ID);
    let cases: Vec<(Vec<u8>, Vec<u8>)> = vec![
        (
            ToWorker::Job(Box::new(job.clone())).encode(),
            hex(&[
                "01",           // tag Job
                "02 2f 73",     // store "/s"
                &rep("44", 32), // plan
                "01 00",        // map, round 0
                SMALL_OBSERVER_ID,
                "01 2f  01 02 63 63  00", // directory "/", argv ["cc"], env []
                "01",                     // language c
                "02 2f 6d  02",           // main "/m", size 2
                &rep("22", 32),           // content
                "03  01 00 00",           // cost 3, origins [(0, 0)]
                "00 00",                  // transform, post: absent
            ]),
        ),
        (ToWorker::Exit.encode(), hex(&["02"])),
        (ToCoordinator::Ready { protocol: 2, tool: "t".into() }.encode(), hex(&["01 02 01 74"])),
        (
            ToCoordinator::Done { observer: d(1), shard: d(2), cpu_ns: 3, peak_rss: 300 }.encode(),
            hex(&["02", &rep("01", 32), &rep("02", 32), "03 ac 02"]),
        ),
        (
            ToCoordinator::Failed { observer: d(3), message: "m".into() }.encode(),
            hex(&["03", &rep("03", 32), "01 6d"]),
        ),
    ];
    for (got, want) in &cases {
        assert_eq!(got, want);
    }
    assert_eq!(ToWorker::decode(&cases[0].1).unwrap(), ToWorker::Job(Box::new(job)));
    assert_eq!(ToWorker::decode(&cases[1].1).unwrap(), ToWorker::Exit);
}

#[test]
fn s5_2_messages_round_trip() {
    let s = scratch("rt");
    let to_worker = [
        ToWorker::Job(Box::new(job(&s, "ok"))),
        ToWorker::Job(Box::new(Job {
            store: b"/\xff/odd".to_vec(),
            plan: d(9),
            phase: Phase::Verify,
            round: u64::MAX,
            observer: observer(&["x", "y", "z"]),
            transform: None,
            post: None,
        })),
        ToWorker::Exit,
    ];
    for m in &to_worker {
        assert_eq!(&ToWorker::decode(&m.encode()).unwrap(), m);
    }
    let to_coordinator = [
        ToCoordinator::Ready { protocol: PROTOCOL, tool: "slip 0.0.0".into() },
        ToCoordinator::Done { observer: d(1), shard: d(2), cpu_ns: u64::MAX, peak_rss: 0 },
        ToCoordinator::Failed { observer: d(3), message: "ünïcode\nline".into() },
        ToCoordinator::Failed { observer: d(3), message: String::new() },
    ];
    for m in &to_coordinator {
        assert_eq!(&ToCoordinator::decode(&m.encode()).unwrap(), m);
    }
}

/// §5.2 "Protocol 2": the two last fields of `Job`, by hand-written bytes.
#[test]
fn s5_2_protocol_2_fields_hand_written_bytes() {
    assert_eq!(PROTOCOL, 2);
    let base = Job {
        store: b"/s".to_vec(),
        plan: d(0x44),
        phase: Phase::Verify,
        round: 1,
        observer: observer(&["cc"]),
        transform: None,
        post: None,
    };
    let head = |phase_round: &str| {
        hex(&[
            "01 02 2f 73",
            &rep("44", 32),
            phase_round,
            SMALL_OBSERVER_ID,
            "01 2f  01 02 63 63  00  01  02 2f 6d  02",
            &rep("22", 32),
            "03  01 00 00",
        ])
    };
    let cases: Vec<(Job, Vec<u8>)> = vec![
        (
            Job { phase: Phase::Map, round: 0, transform: Some(d(0x31)), ..base.clone() },
            [head("01 00"), hex(&["01", &rep("31", 32), "00"])].concat(),
        ),
        (
            Job {
                post: Some(Post {
                    pre: 0,
                    declared: d(0x32),
                    image: PostImage::Tree(vec![
                        (b"/a".to_vec(), b"/b".to_vec()),
                        (b"/c".to_vec(), b"/d".to_vec()),
                    ]),
                }),
                ..base.clone()
            },
            [
                head("02 01"),
                hex(&["00 01  00", &rep("32", 32), "00 02  02 2f 61 02 2f 62  02 2f 63 02 2f 64"]),
            ]
            .concat(),
        ),
        (
            Job {
                post: Some(Post { pre: 3, declared: d(0x32), image: PostImage::Overlay(d(0x33)) }),
                ..base.clone()
            },
            [head("02 01"), hex(&["00 01  03", &rep("32", 32), "01", &rep("33", 32)])].concat(),
        ),
    ];
    for (job, want) in cases {
        let msg = ToWorker::Job(Box::new(job));
        assert_eq!(msg.encode(), want);
        assert_eq!(ToWorker::decode(&want).unwrap(), msg);
    }
}

/// §5.2 "Protocol 2": a `Job` with each combination of the two fields, and
/// each form of image, round trips (including the empty tree, the identity).
#[test]
fn s5_2_protocol_2_fields_round_trip() {
    let posts = [
        None,
        Some(Post { pre: 0, declared: d(1), image: PostImage::Tree(vec![]) }),
        Some(Post {
            pre: u64::MAX,
            declared: d(2),
            image: PostImage::Tree(vec![
                (b"/a".to_vec(), b"/x/\xff".to_vec()),
                (b"/a/b".to_vec(), b"/y".to_vec()),
                (b"/b".to_vec(), Vec::new()),
            ]),
        }),
        Some(Post { pre: 7, declared: d(3), image: PostImage::Overlay(d(4)) }),
    ];
    let mut n = 0;
    for transform in [None, Some(d(9))] {
        for post in &posts {
            for phase in [Phase::Map, Phase::Verify] {
                let job = Job {
                    store: b"/s".to_vec(),
                    plan: d(5),
                    phase,
                    round: 2,
                    observer: observer(&["x"]),
                    transform,
                    post: post.clone(),
                };
                let msg = ToWorker::Job(Box::new(job));
                let bytes = msg.encode();
                assert_eq!(ToWorker::decode(&bytes).unwrap(), msg);
                // Every proper prefix is refused.
                for k in 1..bytes.len() {
                    assert!(ToWorker::decode(&bytes[..k]).is_err(), "prefix {k} decoded");
                }
                n += 1;
            }
        }
    }
    assert_eq!(n, 16);
}

/// §5.5: root maps not strictly ascending by `subject` (equal, or
/// descending) are `Malformed` at the first byte of the first out of order; a `PostImage` tag
/// other than 0 and 1 is refused at the tag; an `opt` tag other than 0 and
/// 1 is refused at that byte.
#[test]
fn s5_5_post_image_rejected() {
    let base = Job {
        store: b"/s".to_vec(),
        plan: d(0x44),
        phase: Phase::Verify,
        round: 1,
        observer: observer(&["cc"]),
        transform: None,
        post: None,
    };
    let none = ToWorker::Job(Box::new(base.clone())).encode();
    // Offset of the opt tag of `post`: the last byte of a job with neither.
    let post_at = none.len() - 1;
    let with = |maps: Vec<(&[u8], &[u8])>| {
        let maps = maps.into_iter().map(|(a, b)| (a.to_vec(), b.to_vec())).collect();
        let post = Post { pre: 0, declared: d(1), image: PostImage::Tree(maps) };
        ToWorker::Job(Box::new(Job { post: Some(post), ..base.clone() })).encode()
    };
    // opt tag, pre, declared, image tag, count: the first map starts here.
    let first_map = post_at + 1 + 1 + 32 + 1 + 1;
    for maps in
        [vec![(&b"/b"[..], &b"/x"[..]), (b"/a", b"/y")], vec![(b"/a", b"/x"), (b"/a", b"/y")]]
    {
        let bytes = with(maps);
        let e = ToWorker::decode(&bytes).unwrap_err();
        assert_eq!((e.kind, e.offset), (ErrorKind::Malformed, first_map + 6), "{e}");
    }
    // In order: accepted, a proper prefix of a subject first.
    assert!(ToWorker::decode(&with(vec![(b"/a", b"/x"), (b"/a/", b"/y")])).is_ok());
    // An image tag of 2.
    let mut bytes = with(vec![]);
    let tag_at = post_at + 1 + 1 + 32;
    assert_eq!(bytes[tag_at], 0);
    bytes[tag_at] = 2;
    let e = ToWorker::decode(&bytes).unwrap_err();
    assert_eq!((e.kind, e.offset), (ErrorKind::Malformed, tag_at));
    // An opt tag of 2, in `transform` and in `post`.
    for at in [post_at - 1, post_at] {
        let mut bytes = none.clone();
        bytes[at] = 2;
        let e = ToWorker::decode(&bytes).unwrap_err();
        assert_eq!((e.kind, e.offset), (ErrorKind::Malformed, at));
    }
}

/// §5.5, `spec/model.md` §2: the encodings of the protocol 2 fields. A
/// byte after the last field is `Malformed` at that byte; the `PostImage`
/// tag is a `uv`, so a non-minimal one (`80 00`) is `Malformed` at its
/// redundant byte; a root-map count larger than the bytes left is
/// `TooLarge` at the count.
#[test]
fn s5_5_protocol_2_encodings_rejected() {
    let post = Post { pre: 0, declared: d(1), image: PostImage::Tree(vec![]) };
    let job = Job {
        store: b"/s".to_vec(),
        plan: d(0x44),
        phase: Phase::Verify,
        round: 1,
        observer: observer(&["cc"]),
        transform: None,
        post: Some(post),
    };
    let bytes = ToWorker::Job(Box::new(job)).encode();
    // The job ends with: image tag 00, count 00.
    let (tag_at, count_at) = (bytes.len() - 2, bytes.len() - 1);
    assert_eq!(&bytes[tag_at..], &[0, 0]);
    let mut extra = bytes.clone();
    extra.push(0);
    let e = ToWorker::decode(&extra).unwrap_err();
    assert_eq!((e.kind, e.offset), (ErrorKind::Malformed, bytes.len()));
    // Tag 0 written as 80 00.
    let mut long = bytes[..tag_at].to_vec();
    long.extend_from_slice(&[0x80, 0x00, 0x00]);
    let e = ToWorker::decode(&long).unwrap_err();
    assert_eq!((e.kind, e.offset), (ErrorKind::Malformed, tag_at + 1));
    // A count of 5 root maps and nothing after it.
    let mut many = bytes.clone();
    many[count_at] = 5;
    let e = ToWorker::decode(&many).unwrap_err();
    assert_eq!((e.kind, e.offset), (ErrorKind::TooLarge, count_at));
}

#[test]
fn s5_2_messages_rejected() {
    // Unknown tags (and tag 0), at offset 0.
    for bytes in [hex(&["00"]), hex(&["03"]), hex(&["7f"])] {
        let e = ToWorker::decode(&bytes).unwrap_err();
        assert_eq!((e.kind, e.offset), (ErrorKind::Malformed, 0));
    }
    for bytes in [hex(&["00"]), hex(&["04"])] {
        let e = ToCoordinator::decode(&bytes).unwrap_err();
        assert_eq!((e.kind, e.offset), (ErrorKind::Malformed, 0));
    }
    // Empty message: truncated at its tag.
    assert_eq!(ToWorker::decode(&[]).unwrap_err().kind, ErrorKind::Truncated);
    assert_eq!(ToCoordinator::decode(&[]).unwrap_err().kind, ErrorKind::Truncated);
    // Every proper prefix of each message is rejected.
    let s = scratch("rej");
    let w = ToWorker::Job(Box::new(job(&s, "ok"))).encode();
    for n in 1..w.len() {
        assert!(ToWorker::decode(&w[..n]).is_err(), "prefix {n} of a Job decoded");
    }
    let c = ToCoordinator::Done { observer: d(1), shard: d(2), cpu_ns: 3, peak_rss: 4 }.encode();
    for n in 1..c.len() {
        assert_eq!(ToCoordinator::decode(&c[..n]).unwrap_err().kind, ErrorKind::Truncated);
    }
    // Trailing bytes: Malformed at the first one.
    let mut extra = ToWorker::Exit.encode();
    extra.push(0);
    let e = ToWorker::decode(&extra).unwrap_err();
    assert_eq!((e.kind, e.offset), (ErrorKind::Malformed, 1));
    let mut extra = c.clone();
    extra.push(0);
    let e = ToCoordinator::decode(&extra).unwrap_err();
    assert_eq!((e.kind, e.offset), (ErrorKind::Malformed, c.len()));
    // Not UTF-8 in a str.
    let e = ToCoordinator::decode(&hex(&["01 01 01 ff"])).unwrap_err();
    assert_eq!((e.kind, e.offset), (ErrorKind::Malformed, 3));
    // An observer that does not validate on its own (id ≠ ObserverId).
    let mut bad = w.clone();
    // The id follows store, plan, phase and round.
    let pos = bad.windows(32).position(|win| win == job(&s, "ok").observer.id.0).unwrap();
    bad[pos] ^= 1;
    assert_eq!(ToWorker::decode(&bad).unwrap_err().kind, ErrorKind::Malformed);
    // An unlisted phase.
    let mut ph = ToWorker::Job(Box::new(job(&s, "ok"))).encode();
    let phase_at = pos - 2;
    assert_eq!(ph[phase_at], 1);
    ph[phase_at] = 3;
    assert_eq!(ToWorker::decode(&ph).unwrap_err().kind, ErrorKind::Malformed);
}

// ---------------------------------------------------------------------------
// Frames

#[test]
fn s5_2_frames() {
    let mut buf = Vec::new();
    write_frame(&mut buf, b"abc").unwrap();
    write_frame(&mut buf, b"").unwrap();
    assert_eq!(buf, hex(&["03 00 00 00 61 62 63  00 00 00 00"]));
    let mut r = Cursor::new(buf);
    assert_eq!(read_frame(&mut r, 3).unwrap(), Some(b"abc".to_vec()));
    assert_eq!(read_frame(&mut r, 3).unwrap(), Some(Vec::new()));
    // A clean end of input.
    assert_eq!(read_frame(&mut r, 3).unwrap(), None);
    // Oversized: refused before reading (or allocating) the payload.
    let mut r = Cursor::new(hex(&["04 00 00 00 61 62 63 64"]));
    assert_eq!(read_frame(&mut r, 3).unwrap_err().kind(), io::ErrorKind::InvalidData);
    assert_eq!(r.position(), 4);
    let mut r = Cursor::new(hex(&["ff ff ff ff"]));
    assert_eq!(read_frame(&mut r, FRAME_MAX).unwrap_err().kind(), io::ErrorKind::InvalidData);
    // Ends inside the length, inside the payload.
    for bytes in [hex(&["03"]), hex(&["03 00 00"]), hex(&["03 00 00 00 61"])] {
        let e = read_frame(&mut Cursor::new(bytes), 16).unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::UnexpectedEof);
    }
}

#[test]
fn s5_2_cpu_from_proc_stat() {
    // pid (comm with ") (" in it) state ppid … utime=fields 14, stime=15.
    let line = "42 (a) (b) S 1 42 42 0 -1 4194560 100 0 0 0 250 50 0 0 20 0 1 0 1 1 1";
    assert_eq!(cpu_ns_from_stat(line), Some(300 * 10_000_000));
    assert_eq!(cpu_ns_from_stat("42 (a) S 1"), None);
    let own = fs::read_to_string("/proc/self/stat").unwrap();
    assert!(cpu_ns_from_stat(&own).is_some());
}

// ---------------------------------------------------------------------------
// serve in process (§5.2)

/// A task that returns the shard a closure makes.
struct Make<F>(F);

impl<F: FnMut(&Job) -> Result<Shard, String> + Send> Task for Make<F> {
    fn run(&mut self, job: &Job) -> Result<Shard, String> {
        (self.0)(job)
    }
}

fn ok_shard(job: &Job) -> Shard {
    Shard {
        observer: job.observer.id,
        phase: job.phase,
        round: job.round,
        outcome: Outcome { status: Status::Ok, errors: 0, warnings: 0, message: String::new() },
        inputs: None,
        counters: vec![],
        digests: None,
        verdict: None,
        edits: None,
        binding: None,
        rule_use: None,
        spellings: None,
        names: None,
        negatives: None,
        found: None,
        rebuilt_use: None,
    }
}

fn frames(msgs: &[ToWorker]) -> Vec<u8> {
    let mut out = Vec::new();
    for m in msgs {
        write_frame(&mut out, &m.encode()).unwrap();
    }
    out
}

fn answers(output: &[u8]) -> Vec<ToCoordinator> {
    let mut r = Cursor::new(output);
    let mut out = Vec::new();
    while let Some(f) = read_frame(&mut r, FRAME_MAX).unwrap() {
        out.push(ToCoordinator::decode(&f).unwrap());
    }
    out
}

fn options(max_jobs: u64) -> ServeOptions {
    ServeOptions { tool: "test".into(), stack_bytes: 1 << 20, max_jobs }
}

fn ready() -> ToCoordinator {
    ToCoordinator::Ready { protocol: PROTOCOL, tool: "test".into() }
}

#[test]
fn s5_2_serve_in_process() {
    let s = scratch("inproc");
    let a = job(&s, "a");
    let b = job(&s, "b");
    let input = frames(&[ToWorker::Job(Box::new(a.clone())), ToWorker::Job(Box::new(b.clone()))]);
    let mut out = Vec::new();
    serve(Make(|j: &Job| Ok(ok_shard(j))), &options(5), Cursor::new(input), &mut out).unwrap();
    let got = answers(&out);
    assert_eq!(got.len(), 3, "Ready and two answers, then the end of input: {got:?}");
    assert_eq!(got[0], ready());
    let store = Store::open(&s, Access::Shared).unwrap();
    for (msg, j) in got[1..].iter().zip([&a, &b]) {
        let ToCoordinator::Done { observer, shard, .. } = msg else { panic!("{msg:?}") };
        assert_eq!(*observer, j.observer.id);
        let bytes = store.read_shard(&j.plan, j.phase, j.round, observer).unwrap().unwrap();
        assert_eq!(slip_model::formats::content_id(&bytes).unwrap(), *shard);
        assert_eq!(Shard::from_bytes(&bytes).unwrap(), ok_shard(j));
    }
    assert!(fs::read_dir(s.join("tmp")).unwrap().next().is_none());
}

#[test]
fn s5_2_serve_rejects_a_shard_for_another_job() {
    type MakeShard = Box<dyn Fn(&Job) -> Shard + Send>;
    let s = scratch("wrongshard");
    let j = job(&s, "a");
    let cases: Vec<MakeShard> = vec![
        Box::new(|j| Shard { observer: d(0), ..ok_shard(j) }),
        Box::new(|j| Shard { phase: Phase::Verify, ..ok_shard(j) }),
        Box::new(|j| Shard { round: 1, ..ok_shard(j) }),
        // Invalid: `ok` with a message.
        Box::new(|j| {
            let mut s = ok_shard(j);
            s.outcome.message = "x".into();
            s
        }),
    ];
    for make in cases {
        let mut out = Vec::new();
        let input = frames(&[ToWorker::Job(Box::new(j.clone()))]);
        serve(Make(move |j: &Job| Ok(make(j))), &options(1), Cursor::new(input), &mut out).unwrap();
        let got = answers(&out);
        let ToCoordinator::Failed { observer, .. } = &got[1] else { panic!("{got:?}") };
        assert_eq!(*observer, j.observer.id);
    }
    assert!(!s.join("shards").join(d(0x44).to_string()).exists());
}

#[test]
fn s5_2_serve_failed_message_is_clipped() {
    let s = scratch("clip");
    let input = frames(&[ToWorker::Job(Box::new(job(&s, "a")))]);
    let mut out = Vec::new();
    let long = "é".repeat(5000);
    serve(Make(move |_: &Job| Err(long.clone())), &options(1), Cursor::new(input), &mut out)
        .unwrap();
    let got = answers(&out);
    let ToCoordinator::Failed { message, .. } = &got[1] else { panic!("{got:?}") };
    assert_eq!(message.len(), 4096);
    assert!(message.chars().all(|c| c == 'é'));
}

#[test]
fn s5_2_serve_panic_ends_the_worker() {
    let s = scratch("panic");
    let input =
        frames(&[ToWorker::Job(Box::new(job(&s, "a"))), ToWorker::Job(Box::new(job(&s, "b")))]);
    let mut out = Vec::new();
    serve(
        Make(|_: &Job| -> Result<Shard, String> { panic!("boom {}", 7) }),
        &options(5),
        Cursor::new(input),
        &mut out,
    )
    .unwrap();
    let got = answers(&out);
    assert_eq!(got.len(), 2, "{got:?}");
    let ToCoordinator::Failed { message, .. } = &got[1] else { panic!("{got:?}") };
    assert_eq!(message, "panicked: boom 7");
}

#[test]
fn s5_2_serve_limits_and_bad_input() {
    let s = scratch("limits");
    // max_jobs 0: Ready only, nothing read.
    let mut out = Vec::new();
    let input = frames(&[ToWorker::Job(Box::new(job(&s, "a")))]);
    serve(Make(|j: &Job| Ok(ok_shard(j))), &options(0), Cursor::new(input), &mut out).unwrap();
    assert_eq!(answers(&out), [ready()]);
    // Exit before a job.
    let mut out = Vec::new();
    let input = frames(&[ToWorker::Exit, ToWorker::Job(Box::new(job(&s, "a")))]);
    serve(Make(|j: &Job| Ok(ok_shard(j))), &options(5), Cursor::new(input), &mut out).unwrap();
    assert_eq!(answers(&out), [ready()]);
    // A frame that does not decode, an oversized frame, a truncated frame.
    for input in [
        frames(&[]).into_iter().chain(hex(&["01 00 00 00 7f"])).collect::<Vec<u8>>(),
        hex(&["ff ff ff ff"]),
        hex(&["05 00"]),
    ] {
        let mut out = Vec::new();
        let e = serve(Make(|j: &Job| Ok(ok_shard(j))), &options(5), Cursor::new(input), &mut out)
            .unwrap_err();
        assert!(
            matches!(e.kind(), io::ErrorKind::InvalidData | io::ErrorKind::UnexpectedEof),
            "{e}"
        );
        assert_eq!(answers(&out), [ready()]);
    }
}

// ---------------------------------------------------------------------------
// The fake worker as a child process (§5.2)

const TIMEOUT: Duration = Duration::from_mins(1);

/// A running `slip-fake-worker`; killed and reaped on drop.
struct Worker {
    child: Child,
    stdin: Option<ChildStdin>,
    frames: Receiver<io::Result<Option<Vec<u8>>>>,
}

impl Worker {
    fn start(args: &[&str]) -> Worker {
        let mut child = Command::new(env!("CARGO_BIN_EXE_slip-fake-worker"))
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let stdin = child.stdin.take();
        let mut stdout = child.stdout.take().unwrap();
        let (tx, frames) = channel();
        std::thread::spawn(move || {
            loop {
                let f = read_frame(&mut stdout, FRAME_MAX);
                let end = !matches!(f, Ok(Some(_)));
                if tx.send(f).is_err() || end {
                    return;
                }
            }
        });
        Worker { child, stdin, frames }
    }

    fn send(&mut self, msg: &ToWorker) {
        write_frame(self.stdin.as_mut().unwrap(), &msg.encode()).unwrap();
    }

    /// The next message; `None` at the end of the worker's output.
    fn recv(&mut self) -> Option<ToCoordinator> {
        match self.frames.recv_timeout(TIMEOUT) {
            Ok(Ok(Some(f))) => Some(ToCoordinator::decode(&f).unwrap()),
            Ok(Ok(None)) | Err(RecvTimeoutError::Disconnected) => None,
            Ok(Err(e)) => panic!("reading the worker: {e}"),
            Err(RecvTimeoutError::Timeout) => {
                panic!("the worker did not answer within {TIMEOUT:?}")
            }
        }
    }

    fn close_stdin(&mut self) {
        self.stdin = None;
    }

    /// Waits (bounded) for the exit status.
    fn exit_code(&mut self) -> Option<i32> {
        let start = std::time::Instant::now();
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                return status.code();
            }
            assert!(start.elapsed() < TIMEOUT, "the worker did not exit");
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn fake_ready() -> ToCoordinator {
    ToCoordinator::Ready { protocol: PROTOCOL, tool: "slip-fake-worker".into() }
}

#[test]
fn s5_2_fake_worker_ready_done_and_end_of_input() {
    let s = scratch("fake-ok");
    drop(Store::open(&s, Access::Shared).unwrap());
    let mut w = Worker::start(&[]);
    assert_eq!(w.recv(), Some(fake_ready()));
    let j = job(&s, "ok");
    w.send(&ToWorker::Job(Box::new(j.clone())));
    let Some(ToCoordinator::Done { observer, shard, .. }) = w.recv() else { panic!() };
    assert_eq!(observer, j.observer.id);
    let store = Store::open(&s, Access::Shared).unwrap();
    let bytes = store.read_shard(&j.plan, j.phase, j.round, &observer).unwrap().unwrap();
    assert_eq!(slip_model::formats::content_id(&bytes).unwrap(), shard);
    let parsed = Shard::from_bytes(&bytes).unwrap();
    assert_eq!(parsed.outcome.status, Status::Ok);
    assert_eq!(parsed.inputs, Some(vec![j.observer.main.clone()]));
    // Default --jobs 1: the worker ends after one job.
    assert_eq!(w.recv(), None);
    assert_eq!(w.exit_code(), Some(0));
    assert!(fs::read_dir(s.join("tmp")).unwrap().next().is_none());
}

#[test]
fn s5_2_fake_worker_diagnosed() {
    let s = scratch("fake-diag");
    let mut w = Worker::start(&[]);
    assert_eq!(w.recv(), Some(fake_ready()));
    let j = job(&s, "diagnosed");
    w.send(&ToWorker::Job(Box::new(j.clone())));
    let Some(ToCoordinator::Done { observer, .. }) = w.recv() else { panic!() };
    let store = Store::open(&s, Access::Shared).unwrap();
    let bytes = store.read_shard(&j.plan, j.phase, j.round, &observer).unwrap().unwrap();
    assert_eq!(Shard::from_bytes(&bytes).unwrap().outcome.status, Status::Diagnosed);
    assert_eq!(w.exit_code(), Some(0));
}

#[test]
fn s5_2_fake_worker_fail_and_panic_answer_failed() {
    let s = scratch("fake-fail");
    for (behaviour, message) in
        [("fail:it broke", "it broke"), ("panic", "panicked: fake worker panic")]
    {
        let mut w = Worker::start(&["--jobs", "3"]);
        assert_eq!(w.recv(), Some(fake_ready()));
        let j = job(&s, behaviour);
        w.send(&ToWorker::Job(Box::new(j.clone())));
        assert_eq!(
            w.recv(),
            Some(ToCoordinator::Failed { observer: j.observer.id, message: message.into() })
        );
        if behaviour == "panic" {
            // A panic ends the worker even with jobs left.
            assert_eq!(w.recv(), None);
        } else {
            // A failure does not: the next job is served.
            let k = job(&s, "ok");
            w.send(&ToWorker::Job(Box::new(k)));
            assert!(matches!(w.recv(), Some(ToCoordinator::Done { .. })));
            w.close_stdin();
            assert_eq!(w.recv(), None);
        }
        assert_eq!(w.exit_code(), Some(0));
    }
    assert!(
        !s.join("shards")
            .join(d(0x44).to_string())
            .join("map.0")
            .join(format!("{}.shard", job(&s, "panic").observer.id))
            .exists()
    );
}

#[test]
fn s5_2_fake_worker_max_jobs() {
    let s = scratch("fake-max");
    let mut w = Worker::start(&["--jobs", "2"]);
    assert_eq!(w.recv(), Some(fake_ready()));
    for b in ["ok", "sleep:1"] {
        w.send(&ToWorker::Job(Box::new(job(&s, b))));
        assert!(matches!(w.recv(), Some(ToCoordinator::Done { .. })));
    }
    // After two jobs it ends without being told to.
    assert_eq!(w.recv(), None);
    assert_eq!(w.exit_code(), Some(0));
}

#[test]
fn s5_2_fake_worker_exit() {
    let mut w = Worker::start(&["--jobs", "5"]);
    assert_eq!(w.recv(), Some(fake_ready()));
    w.send(&ToWorker::Exit);
    assert_eq!(w.recv(), None);
    assert_eq!(w.exit_code(), Some(0));
}

#[test]
fn s5_2_fake_worker_end_of_input_before_a_job() {
    let mut w = Worker::start(&["--jobs", "5"]);
    assert_eq!(w.recv(), Some(fake_ready()));
    w.close_stdin();
    assert_eq!(w.recv(), None);
    assert_eq!(w.exit_code(), Some(0));
}

#[test]
fn s5_2_fake_worker_exit_status() {
    let s = scratch("fake-exit");
    let mut w = Worker::start(&[]);
    assert_eq!(w.recv(), Some(fake_ready()));
    w.send(&ToWorker::Job(Box::new(job(&s, "exit:7"))));
    assert_eq!(w.recv(), None);
    assert_eq!(w.exit_code(), Some(7));
}

// The rest of the fake worker's table, which tests/pool.rs relies on.
#[test]
fn fixture_fake_worker_misbehaviours() {
    use std::os::unix::process::ExitStatusExt as _;
    let s = scratch("fake-misc");
    drop(Store::open(&s, Access::Shared).unwrap());

    // garbage: a frame length above any limit.
    let mut w = Worker::start(&[]);
    assert_eq!(w.recv(), Some(fake_ready()));
    w.send(&ToWorker::Job(Box::new(job(&s, "garbage"))));
    match w.frames.recv_timeout(TIMEOUT).unwrap() {
        Err(e) => assert_eq!(e.kind(), io::ErrorKind::InvalidData),
        other => panic!("{other:?}"),
    }
    drop(w);

    // wrong-observer: Done for another observer, with a real shard.
    let mut w = Worker::start(&[]);
    assert_eq!(w.recv(), Some(fake_ready()));
    let j = job(&s, "wrong-observer");
    w.send(&ToWorker::Job(Box::new(j.clone())));
    let Some(ToCoordinator::Done { observer, shard, .. }) = w.recv() else { panic!() };
    assert_ne!(observer, j.observer.id);
    let store = Store::open(&s, Access::Shared).unwrap();
    let bytes = store.read_shard(&j.plan, j.phase, j.round, &j.observer.id).unwrap().unwrap();
    assert_eq!(slip_model::formats::content_id(&bytes).unwrap(), shard);
    assert_eq!(w.exit_code(), Some(0));

    // no-shard: Done naming a shard that was not published.
    let mut w = Worker::start(&[]);
    assert_eq!(w.recv(), Some(fake_ready()));
    let j = job(&s, "no-shard");
    w.send(&ToWorker::Job(Box::new(j.clone())));
    let Some(ToCoordinator::Done { observer, .. }) = w.recv() else { panic!() };
    assert_eq!(observer, j.observer.id);
    assert_eq!(store.read_shard(&j.plan, j.phase, j.round, &j.observer.id).unwrap(), None);
    assert_eq!(w.exit_code(), Some(0));

    // late-output: Done, then bytes after Exit.
    let mut w = Worker::start(&["--jobs", "2"]);
    assert_eq!(w.recv(), Some(fake_ready()));
    w.send(&ToWorker::Job(Box::new(job(&s, "late-output"))));
    assert!(matches!(w.recv(), Some(ToCoordinator::Done { .. })));
    w.send(&ToWorker::Exit);
    // "late output\n" read as a frame: its first four bytes are a length.
    assert!(matches!(w.frames.recv_timeout(TIMEOUT).unwrap(), Err(_) | Ok(Some(_))));
    assert_eq!(w.exit_code(), Some(0));

    // abort: killed by a signal, no answer.
    let mut w = Worker::start(&[]);
    assert_eq!(w.recv(), Some(fake_ready()));
    w.send(&ToWorker::Job(Box::new(job(&s, "abort"))));
    assert_eq!(w.recv(), None);
    assert_eq!(w.exit_code(), None);
    assert_eq!(w.child.wait().unwrap().signal(), Some(9));

    // alloc (small) and sleep: then as ok.
    let mut w = Worker::start(&["--jobs", "2"]);
    assert_eq!(w.recv(), Some(fake_ready()));
    let mut j = job(&s, "alloc:4");
    j.observer = self::observer(&["fake", "alloc:4", "1"]);
    w.send(&ToWorker::Job(Box::new(j)));
    let Some(ToCoordinator::Done { peak_rss, .. }) = w.recv() else { panic!() };
    assert!(peak_rss >= 4 << 20, "peak {peak_rss}");
    w.send(&ToWorker::Job(Box::new(job(&s, "sleep:1"))));
    assert!(matches!(w.recv(), Some(ToCoordinator::Done { .. })));
    assert_eq!(w.exit_code(), Some(0));

    // hang: no answer; the Drop kills it.
    let mut w = Worker::start(&[]);
    assert_eq!(w.recv(), Some(fake_ready()));
    w.send(&ToWorker::Job(Box::new(job(&s, "hang"))));
    assert!(matches!(
        w.frames.recv_timeout(Duration::from_millis(200)),
        Err(RecvTimeoutError::Timeout)
    ));
    drop(w);

    // An unknown behaviour is a failure.
    let mut w = Worker::start(&[]);
    assert_eq!(w.recv(), Some(fake_ready()));
    w.send(&ToWorker::Job(Box::new(job(&s, "no-such"))));
    assert!(matches!(w.recv(), Some(ToCoordinator::Failed { .. })));
}
