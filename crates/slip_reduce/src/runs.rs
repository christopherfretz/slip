//! The temporary files of `spec/edits.md` §5.6: runs of collapsed records,
//! merged `fan_in` at a time until one remains, and the basis file the
//! second pass checks the shards against.
//!
//! The files are private to one call of `reduce`: named with the process
//! id and a counter under `Options::scratch`, removed when the call
//! returns, whatever it returns ([`Cleanup`]). Their format is not a
//! container and is not specified; nothing of it reaches the edit set.

use std::cmp::Reverse;
use std::collections::{BTreeMap, BinaryHeap};
use std::fs::{File, OpenOptions};
use std::io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use slip_model::Digest;

use crate::meter::{Held, Meter};
use crate::records::{Key, Val, decode, encode};

/// Distinguishes the calls of one process.
static CALLS: AtomicU64 = AtomicU64::new(0);

/// Where one call's temporary files are.
#[derive(Debug)]
pub(crate) struct Scratch {
    dir: PathBuf,
    stem: String,
}

impl Scratch {
    pub fn new(dir: &Path) -> Scratch {
        let call = CALLS.fetch_add(1, Ordering::Relaxed);
        Scratch {
            dir: dir.to_path_buf(),
            stem: format!("slip-reduce-{}-{call}", std::process::id()),
        }
    }

    /// The run file `index` of merge level `level` (level 0: the runs of
    /// shards).
    pub fn run(&self, level: usize, index: usize) -> PathBuf {
        self.dir.join(format!("{}-{level}-{index}", self.stem))
    }

    /// The basis file: one fixed-size entry per observer.
    pub fn basis(&self) -> PathBuf {
        self.dir.join(format!("{}-basis", self.stem))
    }
}

/// Removes every temporary file of a call when dropped. It knows the
/// number of runs of the first pass and `fan_in`, from which the files of
/// every merge level follow, not their names: its memory does not grow
/// with the number of runs or of levels (landed WP-142 §1).
#[derive(Debug)]
pub(crate) struct Cleanup<'a> {
    scratch: &'a Scratch,
    /// The runs of the first pass (the files of level 0), 0 before it
    /// starts.
    pub runs: usize,
    pub fan_in: usize,
    pub basis: bool,
}

impl<'a> Cleanup<'a> {
    pub fn new(scratch: &'a Scratch, fan_in: usize) -> Cleanup<'a> {
        Cleanup { scratch, runs: 0, fan_in, basis: false }
    }
}

/// The number of files of each merge level, level 0 first, for `runs`
/// runs of the first pass: level `l + 1` has `ceil(count / fan_in)` files
/// while level `l` has more than one, as the merge makes them. Empty for
/// no runs.
pub(crate) fn level_counts(runs: usize, fan_in: usize) -> impl Iterator<Item = usize> {
    let fan_in = fan_in.max(2);
    std::iter::successors((runs > 0).then_some(runs), move |&c| (c > 1).then(|| c.div_ceil(fan_in)))
}

impl Drop for Cleanup<'_> {
    fn drop(&mut self) {
        // A file a failed call did not make is not there to remove.
        for (level, count) in level_counts(self.runs, self.fan_in).enumerate() {
            for index in 0..count {
                let _ = std::fs::remove_file(self.scratch.run(level, index));
            }
        }
        if self.basis {
            let _ = std::fs::remove_file(self.scratch.basis());
        }
    }
}

pub(crate) fn io(path: &Path, e: &std::io::Error) -> String {
    format!("{}: {e}", path.display())
}

/// Writes records in key order.
pub(crate) struct RunWriter<'m> {
    path: PathBuf,
    out: BufWriter<File>,
    _open: Held<'m>,
}

impl<'m> RunWriter<'m> {
    pub fn create(path: PathBuf, meter: &'m Meter) -> Result<RunWriter<'m>, String> {
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|e| io(&path, &e))?;
        Ok(RunWriter { path, out: BufWriter::new(file), _open: meter.files.hold() })
    }

    pub fn write(&mut self, key: &Key, val: &Val) -> Result<(), String> {
        let bytes = encode(key, val);
        let mut len = Vec::with_capacity(10);
        let mut v = bytes.len() as u64;
        loop {
            let b = (v & 0x7f) as u8;
            v >>= 7;
            if v == 0 {
                len.push(b);
                break;
            }
            len.push(b | 0x80);
        }
        self.out.write_all(&len).map_err(|e| io(&self.path, &e))?;
        self.out.write_all(&bytes).map_err(|e| io(&self.path, &e))
    }

    pub fn finish(mut self) -> Result<(), String> {
        self.out.flush().map_err(|e| io(&self.path, &e))
    }
}

/// Writes a run of collapsed records.
pub(crate) fn write_run(
    path: PathBuf,
    records: &BTreeMap<Key, Val>,
    meter: &Meter,
) -> Result<(), String> {
    let mut w = RunWriter::create(path, meter)?;
    for (k, v) in records {
        w.write(k, v)?;
    }
    w.finish()
}

/// Reads records back.
pub(crate) struct RunReader<'m> {
    path: PathBuf,
    input: BufReader<File>,
    buf: Vec<u8>,
    _open: Held<'m>,
}

impl<'m> RunReader<'m> {
    pub fn open(path: PathBuf, meter: &'m Meter) -> Result<RunReader<'m>, String> {
        let file = File::open(&path).map_err(|e| io(&path, &e))?;
        Ok(RunReader {
            path,
            input: BufReader::new(file),
            buf: Vec::new(),
            _open: meter.files.hold(),
        })
    }

    /// The next record, or `None` at the end.
    pub fn next(&mut self) -> Result<Option<(Key, Val)>, String> {
        let mut len: u64 = 0;
        let mut shift = 0;
        loop {
            let mut b = [0u8; 1];
            let n = self.input.read(&mut b).map_err(|e| io(&self.path, &e))?;
            if n == 0 {
                if shift == 0 {
                    return Ok(None);
                }
                return Err(format!("{}: a record's length is cut short", self.path.display()));
            }
            if shift > 63 {
                return Err(format!("{}: a record's length is too long", self.path.display()));
            }
            len |= u64::from(b[0] & 0x7f) << shift;
            shift += 7;
            if b[0] & 0x80 == 0 {
                break;
            }
        }
        let len = usize::try_from(len)
            .map_err(|_| format!("{}: a record too large", self.path.display()))?;
        self.buf.clear();
        self.buf.resize(len, 0);
        self.input.read_exact(&mut self.buf).map_err(|e| io(&self.path, &e))?;
        decode(&self.buf).map(Some).map_err(|e| format!("{}: {e}", self.path.display()))
    }
}

/// Merges sorted run files into one, joining records of one key. It holds
/// one record per input and the one being joined (`reduce.peak.records`).
pub(crate) fn merge(inputs: &[PathBuf], out: PathBuf, meter: &Meter) -> Result<(), String> {
    let mut readers = Vec::with_capacity(inputs.len());
    for p in inputs {
        readers.push(RunReader::open(p.clone(), meter)?);
    }
    let mut vals: Vec<Option<Val>> = vec![None; readers.len()];
    let mut heap: BinaryHeap<Reverse<(Key, usize)>> = BinaryHeap::new();
    for (i, r) in readers.iter_mut().enumerate() {
        if let Some((k, v)) = r.next()? {
            vals[i] = Some(v);
            heap.push(Reverse((k, i)));
        }
    }
    let mut w = RunWriter::create(out, meter)?;
    while let Some(Reverse((key, i))) = heap.pop() {
        let mut val = vals[i].take().ok_or_else(|| "merge: a value is missing".to_string())?;
        advance(&mut readers, &mut vals, &mut heap, i)?;
        // The inputs' records (one per input still open) and `val`.
        meter.records.at(heap.len() + 1);
        while heap.peek().is_some_and(|Reverse((k, _))| *k == key) {
            let Some(Reverse((_, j))) = heap.pop() else { break };
            let other = vals[j].take().ok_or_else(|| "merge: a value is missing".to_string())?;
            val.join(&other)?;
            advance(&mut readers, &mut vals, &mut heap, j)?;
        }
        w.write(&key, &val)?;
    }
    w.finish()
}

fn advance(
    readers: &mut [RunReader<'_>],
    vals: &mut [Option<Val>],
    heap: &mut BinaryHeap<Reverse<(Key, usize)>>,
    i: usize,
) -> Result<(), String> {
    if let Some((k, v)) = readers[i].next()? {
        vals[i] = Some(v);
        heap.push(Reverse((k, i)));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// The basis file: entry `i` at offset `i * BASIS_ENTRY`, the observer, a
// byte that is 1 if the shard is `ok`, and its content id (zeros if not).

pub(crate) const BASIS_ENTRY: u64 = 65;

/// One observer's entry of the basis.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct BasisEntry {
    pub observer: Digest,
    pub shard: Option<Digest>,
}

fn entry_bytes(e: &BasisEntry) -> [u8; 65] {
    let mut b = [0u8; 65];
    b[..32].copy_from_slice(e.observer.as_bytes());
    if let Some(s) = e.shard {
        b[32] = 1;
        b[33..].copy_from_slice(s.as_bytes());
    }
    b
}

fn entry_of(b: &[u8; 65]) -> Result<BasisEntry, String> {
    let mut o = [0u8; 32];
    o.copy_from_slice(&b[..32]);
    let mut s = [0u8; 32];
    s.copy_from_slice(&b[33..]);
    let shard = match b[32] {
        0 => None,
        1 => Some(Digest(s)),
        _ => return Err("the basis file is corrupt".into()),
    };
    Ok(BasisEntry { observer: Digest(o), shard })
}

/// Creates the basis file for `n` observers.
pub(crate) fn create_basis(path: &Path, n: usize, meter: &Meter) -> Result<(), String> {
    let f = OpenOptions::new().write(true).create_new(true).open(path).map_err(|e| io(path, &e))?;
    let _open = meter.files.hold();
    f.set_len(n as u64 * BASIS_ENTRY).map_err(|e| io(path, &e))
}

/// Writes the entries of observers `start..start + entries.len()`.
pub(crate) fn write_basis(
    path: &Path,
    start: usize,
    entries: &[BasisEntry],
    meter: &Meter,
) -> Result<(), String> {
    let mut f = OpenOptions::new().write(true).open(path).map_err(|e| io(path, &e))?;
    let _open = meter.files.hold();
    f.seek(SeekFrom::Start(start as u64 * BASIS_ENTRY)).map_err(|e| io(path, &e))?;
    let mut out = BufWriter::new(f);
    for e in entries {
        out.write_all(&entry_bytes(e)).map_err(|e| io(path, &e))?;
    }
    out.flush().map_err(|e| io(path, &e))
}

/// Reads entries `start..` of the basis file in order.
pub(crate) struct BasisReader<'m> {
    path: PathBuf,
    input: BufReader<File>,
    _open: Held<'m>,
}

impl<'m> BasisReader<'m> {
    pub fn open(path: &Path, start: usize, meter: &'m Meter) -> Result<BasisReader<'m>, String> {
        let mut f = File::open(path).map_err(|e| io(path, &e))?;
        let open = meter.files.hold();
        f.seek(SeekFrom::Start(start as u64 * BASIS_ENTRY)).map_err(|e| io(path, &e))?;
        Ok(BasisReader { path: path.to_path_buf(), input: BufReader::new(f), _open: open })
    }

    pub fn next(&mut self) -> Result<BasisEntry, String> {
        let mut b = [0u8; 65];
        self.input.read_exact(&mut b).map_err(|e| io(&self.path, &e))?;
        entry_of(&b)
    }
}

#[cfg(test)]
mod tests {
    use super::level_counts;

    /// The levels as `reduce` makes them: the first pass's runs, then each
    /// merge level until one file remains (the list `Cleanup` kept before
    /// landed WP-142, from the second review's probe).
    fn made(n: usize, fan_in: usize) -> Vec<usize> {
        let mut count = n.div_ceil(fan_in).max(1);
        let mut levels = vec![count];
        while count > 1 {
            count = count.div_ceil(fan_in);
            levels.push(count);
        }
        levels
    }

    /// Landed WP-142 §1: the computed levels are the levels made, for
    /// every `n` up to 5,000 and `fan_in` 2 to 65.
    #[test]
    fn the_levels_computed_are_the_levels_made() {
        assert_eq!(level_counts(0, 2).count(), 0);
        for fan_in in 2..=65usize {
            for n in 0..=5000usize {
                let runs = n.div_ceil(fan_in).max(1);
                let got: Vec<usize> = level_counts(runs, fan_in).collect();
                assert_eq!(got, made(n, fan_in), "n {n} fan_in {fan_in}");
            }
        }
    }
}
