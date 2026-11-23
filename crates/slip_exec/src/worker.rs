//! The worker side and the protocol (`spec/exec.md` §5). Work package WP-008.
//!
//! Coordinator and worker exchange frames over the worker's standard input
//! and output: a `u32le` length, then one message in the encoding of
//! `spec/model.md` §2 ([`write_frame`], [`read_frame`]). [`ToWorker`] and
//! [`ToCoordinator`] are the messages of the §5.2 table; [`serve`] is the
//! worker's main loop around a [`Task`]. The worker's rules in detail are
//! `spec/exec.md` §5.5, the interface §5.7.

use std::io::{self, Read, Write};
use std::os::unix::ffi::OsStrExt as _;
use std::path::Path;

use slip_model::formats::{
    clip_message, decode_observer, decode_phase, encode_observer, encode_phase,
};
use slip_model::wire::{self, Decoder, Encoder, ErrorKind};
use slip_model::{Digest, Observer, Phase, Shard};

use crate::store::{Access, Store};

/// The protocol version sent in `Ready`: 2 since M2, which added
/// [`Job::transform`] and [`Job::post`] (§5.2 "Protocol 2").
pub const PROTOCOL: u64 = 2;

/// The largest frame either side accepts (64 MiB).
pub const FRAME_MAX: usize = 64 << 20;

/// One observer's work for one phase and round.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Job {
    /// The store's root directory, as bytes of the path (absolute).
    pub store: Vec<u8>,
    /// Content id of the plan.
    pub plan: Digest,
    pub phase: Phase,
    pub round: u64,
    pub observer: Observer,
    /// A map job that runs a transform: the content id of a `TRFM`
    /// container in the job's store (`spec/edits.md` §8.1).
    pub transform: Option<Digest>,
    /// Every verify job: the round's parameters (§5.2).
    pub post: Option<Post>,
}

/// A verify job's parameters (§5.2, `spec/verify.md` §5.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Post {
    /// The round of the map shards the job compares against.
    pub pre: u64,
    /// Content id of a `DECL` container in the job's store.
    pub declared: Digest,
    pub image: PostImage,
}

/// The post-image of a verify job (`spec/verify.md` §5.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PostImage {
    /// Tag 0: root maps `(subject, post)`, strictly ascending by `subject`;
    /// empty is the identity.
    Tree(Vec<(Vec<u8>, Vec<u8>)>),
    /// Tag 1: the content id of an `OVLY` container in the job's store
    /// (`spec/edits.md` §7.1).
    Overlay(Digest),
}

impl Job {
    /// [`Job::store`] as a path.
    pub fn store_path(&self) -> &Path {
        Path::new(std::ffi::OsStr::from_bytes(&self.store))
    }
}

/// What a worker does with one observer (§5.1).
pub trait Task {
    /// Do one observer's work and return the shard to publish.
    /// An `Err` is an internal failure of the worker, not a diagnostic about
    /// the subject: diagnostics are a shard with status `diagnosed`.
    fn run(&mut self, job: &Job) -> Result<Shard, String>;
}

/// A message to the worker (§5.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToWorker {
    /// Tag 1.
    Job(Box<Job>),
    /// Tag 2.
    Exit,
}

/// A message to the coordinator (§5.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToCoordinator {
    /// Tag 1: sent once, first.
    Ready { protocol: u64, tool: String },
    /// Tag 2: the shard, by content id, is in the store.
    Done { observer: Digest, shard: Digest, cpu_ns: u64, peak_rss: u64 },
    /// Tag 3.
    Failed { observer: Digest, message: String },
}

fn unknown_tag(at: usize) -> wire::Error {
    wire::Error::new(ErrorKind::Malformed, at, "unknown message tag")
}

impl ToWorker {
    pub fn encode(&self) -> Vec<u8> {
        let mut e = Encoder::new();
        match self {
            ToWorker::Job(job) => {
                e.uv(1).bytes(&job.store).digest(&job.plan);
                encode_phase(job.phase, &mut e);
                e.uv(job.round);
                encode_observer(&job.observer, &mut e);
                match &job.transform {
                    None => {
                        e.u8(0);
                    }
                    Some(t) => {
                        e.u8(1).digest(t);
                    }
                }
                match &job.post {
                    None => {
                        e.u8(0);
                    }
                    Some(post) => {
                        e.u8(1);
                        encode_post(post, &mut e);
                    }
                }
            }
            ToWorker::Exit => {
                e.uv(2);
            }
        }
        e.into_bytes()
    }

    /// Decodes one message; bytes after its last field are `Malformed`.
    pub fn decode(bytes: &[u8]) -> Result<Self, wire::Error> {
        let mut d = Decoder::new(bytes);
        let at = d.offset();
        let msg = match d.uv()? {
            1 => {
                let store = d.bytes()?.to_vec();
                let plan = d.digest()?;
                let phase = decode_phase(&mut d)?;
                let round = d.uv()?;
                let observer = decode_observer(&mut d)?;
                let transform = decode_opt(&mut d, Decoder::digest)?;
                let post = decode_opt(&mut d, decode_post)?;
                ToWorker::Job(Box::new(Job {
                    store,
                    plan,
                    phase,
                    round,
                    observer,
                    transform,
                    post,
                }))
            }
            2 => ToWorker::Exit,
            _ => return Err(unknown_tag(at)),
        };
        d.finish()?;
        Ok(msg)
    }
}

/// `Post` (§5.2).
fn encode_post(post: &Post, e: &mut Encoder) {
    e.uv(post.pre).digest(&post.declared);
    match &post.image {
        PostImage::Tree(maps) => {
            e.uv(0).count(maps.len());
            for (subject, post) in maps {
                e.bytes(subject).bytes(post);
            }
        }
        PostImage::Overlay(id) => {
            e.uv(1).digest(id);
        }
    }
}

/// `Post` (§5.2, §5.5): a `PostImage` tag other than 0 and 1 is
/// `Malformed` at the tag; root maps not strictly ascending by `subject`
/// are `Malformed` at the first out of order (`spec/model.md` §2: a sorted
/// list in another order).
fn decode_post(d: &mut Decoder<'_>) -> Result<Post, wire::Error> {
    let pre = d.uv()?;
    let declared = d.digest()?;
    let at = d.offset();
    let image = match d.uv()? {
        0 => {
            let n = d.count()?;
            let mut maps: Vec<(Vec<u8>, Vec<u8>)> = Vec::with_capacity(n.min(1 << 10));
            for _ in 0..n {
                let map_at = d.offset();
                let subject = d.bytes()?.to_vec();
                let post = d.bytes()?.to_vec();
                if maps.last().is_some_and(|(last, _)| *last >= subject) {
                    return Err(wire::Error::new(
                        ErrorKind::Malformed,
                        map_at,
                        "root maps not strictly ascending by subject",
                    ));
                }
                maps.push((subject, post));
            }
            PostImage::Tree(maps)
        }
        1 => PostImage::Overlay(d.digest()?),
        _ => return Err(wire::Error::new(ErrorKind::Malformed, at, "unknown post image tag")),
    };
    Ok(Post { pre, declared, image })
}

/// `opt<T>` (`spec/model.md` §2): `Malformed` at the tag byte unless it is
/// 0 or 1.
fn decode_opt<'a, T>(
    d: &mut Decoder<'a>,
    value: impl FnOnce(&mut Decoder<'a>) -> Result<T, wire::Error>,
) -> Result<Option<T>, wire::Error> {
    let at = d.offset();
    match d.u8()? {
        0 => Ok(None),
        1 => Ok(Some(value(d)?)),
        _ => Err(wire::Error::new(ErrorKind::Malformed, at, "opt tag is not 0 or 1")),
    }
}

impl ToCoordinator {
    pub fn encode(&self) -> Vec<u8> {
        let mut e = Encoder::new();
        match self {
            ToCoordinator::Ready { protocol, tool } => {
                e.uv(1).uv(*protocol).str(tool);
            }
            ToCoordinator::Done { observer, shard, cpu_ns, peak_rss } => {
                e.uv(2).digest(observer).digest(shard).uv(*cpu_ns).uv(*peak_rss);
            }
            ToCoordinator::Failed { observer, message } => {
                e.uv(3).digest(observer).str(message);
            }
        }
        e.into_bytes()
    }

    /// Decodes one message; bytes after its last field are `Malformed`.
    pub fn decode(bytes: &[u8]) -> Result<Self, wire::Error> {
        let mut d = Decoder::new(bytes);
        let at = d.offset();
        let msg = match d.uv()? {
            1 => ToCoordinator::Ready { protocol: d.uv()?, tool: d.str()?.to_owned() },
            2 => ToCoordinator::Done {
                observer: d.digest()?,
                shard: d.digest()?,
                cpu_ns: d.uv()?,
                peak_rss: d.uv()?,
            },
            3 => ToCoordinator::Failed { observer: d.digest()?, message: d.str()?.to_owned() },
            _ => return Err(unknown_tag(at)),
        };
        d.finish()?;
        Ok(msg)
    }
}

/// Writes one frame: the payload's length as `u32le`, the payload, then
/// flushes. A payload of 4 GiB or more is `InvalidInput`.
pub fn write_frame(w: &mut impl Write, payload: &[u8]) -> io::Result<()> {
    let len = u32::try_from(payload.len())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "frame of 4 GiB or more"))?;
    w.write_all(&len.to_le_bytes())?;
    w.write_all(payload)?;
    w.flush()
}

/// Reads one frame. `None` at a clean end of input (no byte of a new
/// frame); `UnexpectedEof` if the input ends inside one; `InvalidData`
/// for a length above `max`, which is refused before anything is allocated
/// for it.
pub fn read_frame(r: &mut impl Read, max: usize) -> io::Result<Option<Vec<u8>>> {
    let mut len = [0u8; 4];
    let mut got = 0;
    while got < 4 {
        match r.read(&mut len[got..]) {
            Ok(0) if got == 0 => return Ok(None),
            Ok(0) => {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "end of input inside a frame length",
                ));
            }
            Ok(n) => got += n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    let len = u32::from_le_bytes(len) as usize;
    if len > max {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("frame of {len} bytes exceeds the limit of {max}"),
        ));
    }
    // Grown as bytes arrive, so a peer that announces a large frame and
    // stops costs nothing.
    let mut payload = Vec::with_capacity(len.min(64 << 10));
    r.take(len as u64).read_to_end(&mut payload)?;
    if payload.len() < len {
        return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "end of input inside a frame"));
    }
    Ok(Some(payload))
}

/// The worker's parameters (`--worker-stack`, `--worker-jobs`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServeOptions {
    /// Sent in `Ready`.
    pub tool: String,
    /// Stack reserved for the thread that runs each job.
    pub stack_bytes: usize,
    /// Jobs to run before returning; 0 returns right after `Ready`.
    pub max_jobs: u64,
}

/// The worker's main loop (§5.2): sends `Ready`, then answers each `Job`
/// with `Done` or `Failed` until `max_jobs` jobs have run, `Exit` arrives,
/// the input ends, or a job's task panicked. A message that does not decode
/// is an `InvalidData` error; so is a frame above [`FRAME_MAX`].
pub fn serve(
    mut task: impl Task + Send,
    options: &ServeOptions,
    mut input: impl Read,
    mut output: impl Write,
) -> io::Result<()> {
    let ready = ToCoordinator::Ready { protocol: PROTOCOL, tool: options.tool.clone() };
    write_frame(&mut output, &ready.encode())?;
    let mut done = 0u64;
    let mut cpu_before = proc_cpu_ns();
    while done < options.max_jobs {
        let Some(frame) = read_frame(&mut input, FRAME_MAX)? else { return Ok(()) };
        let job = match ToWorker::decode(&frame) {
            Ok(ToWorker::Job(job)) => job,
            Ok(ToWorker::Exit) => return Ok(()),
            Err(e) => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("bad message: {e}"),
                ));
            }
        };
        // The first job's peak is the process's; a later one's only if the
        // counter could be reset (§5.2).
        let peak_known = done == 0 || reset_peak();
        let (answer, panicked) = run_one(&mut task, &job, options.stack_bytes);
        let answer = match answer {
            Ok(shard) => {
                let cpu_now = proc_cpu_ns();
                let cpu_ns = match (cpu_before, cpu_now) {
                    (Some(b), Some(n)) => n.saturating_sub(b),
                    _ => 0,
                };
                cpu_before = cpu_now;
                let peak_rss = if peak_known { proc_peak_rss() } else { 0 };
                ToCoordinator::Done { observer: job.observer.id, shard, cpu_ns, peak_rss }
            }
            Err(message) => {
                cpu_before = proc_cpu_ns();
                ToCoordinator::Failed {
                    observer: job.observer.id,
                    message: clip_message(&message).to_owned(),
                }
            }
        };
        write_frame(&mut output, &answer.encode())?;
        done += 1;
        if panicked {
            // The task's state is suspect: end, and let the coordinator
            // start a fresh worker for the next observer.
            return Ok(());
        }
    }
    Ok(())
}

/// Runs one job on its own thread and publishes its shard. `Ok` is the
/// shard's content id; the flag says the task panicked.
fn run_one(
    task: &mut (impl Task + Send),
    job: &Job,
    stack: usize,
) -> (Result<Digest, String>, bool) {
    let joined = std::thread::scope(|s| {
        let spawned = std::thread::Builder::new()
            .name("slip-job".into())
            .stack_size(stack)
            .spawn_scoped(s, || task.run(job));
        match spawned {
            Ok(handle) => Ok(handle.join()),
            Err(e) => Err(format!("cannot start the job thread ({stack} bytes of stack): {e}")),
        }
    });
    let shard = match joined {
        Ok(Err(payload)) => return (Err(format!("panicked: {}", panic_message(&*payload))), true),
        Err(message) | Ok(Ok(Err(message))) => return (Err(message), false),
        Ok(Ok(Ok(shard))) => shard,
    };
    (publish(job, &shard), false)
}

/// Checks the task's shard against the job and publishes it.
fn publish(job: &Job, shard: &Shard) -> Result<Digest, String> {
    shard.validate().map_err(|e| format!("the task returned an invalid shard: {e}"))?;
    if shard.observer != job.observer.id || shard.phase != job.phase || shard.round != job.round {
        return Err(format!(
            "the task returned a shard for observer {} {:?} round {}, not observer {} {:?} round {}",
            shard.observer, shard.phase, shard.round, job.observer.id, job.phase, job.round
        ));
    }
    let store = Store::open(job.store_path(), Access::Shared)
        .map_err(|e| format!("cannot open the store: {e}"))?;
    store
        .publish_shard(&job.plan, job.phase, job.round, &job.observer.id, &shard.to_bytes())
        .map_err(|e| format!("cannot publish the shard: {e}"))
}

/// The message of a panic payload.
fn panic_message(payload: &(dyn std::any::Any + Send)) -> &str {
    if let Some(s) = payload.downcast_ref::<&str>() {
        s
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s
    } else {
        "a panic with a non-text payload"
    }
}

/// `/proc` reports times in units of `USER_HZ`, which is 100 on Linux.
const NS_PER_TICK: u64 = 10_000_000;

/// User plus system time of this process (all its threads, live and
/// ended), from `/proc/self/stat`; `None` where unavailable.
fn proc_cpu_ns() -> Option<u64> {
    let text = std::fs::read_to_string("/proc/self/stat").ok()?;
    cpu_ns_from_stat(&text)
}

/// Fields 14 (`utime`) and 15 (`stime`) of a `/proc/<pid>/stat` line, in
/// nanoseconds.
pub fn cpu_ns_from_stat(text: &str) -> Option<u64> {
    let rest = text.rsplit_once(')')?.1;
    // `rest` starts at field 3 (state).
    let mut fields = rest.split_whitespace().skip(11);
    let utime: u64 = fields.next()?.parse().ok()?;
    let stime: u64 = fields.next()?.parse().ok()?;
    Some(utime.saturating_add(stime).saturating_mul(NS_PER_TICK))
}

/// `VmHWM` of this process in bytes; 0 where unavailable.
fn proc_peak_rss() -> u64 {
    std::fs::read_to_string("/proc/self/status")
        .map_or(0, |t| crate::process::Sample::from_proc_status(&t).hwm)
}

/// Resets `VmHWM` to the current resident size (`/proc/self/clear_refs`,
/// value 5); whether that worked.
fn reset_peak() -> bool {
    std::fs::OpenOptions::new()
        .write(true)
        .open("/proc/self/clear_refs")
        .and_then(|mut f| f.write_all(b"5"))
        .is_ok()
}
