//! BLAKE3 in safe Rust: content and tagged digests (`spec/model.md` §3). Work package WP-001.
//!
//! The hash mode only (no key, no key derivation), 32 bytes of output,
//! incremental. The structure follows the BLAKE3 reference implementation
//! (`reference_impl/reference_impl.rs` in the BLAKE3 repository): a chunk
//! state for the current 1024-byte chunk and a stack of chaining values of
//! completed subtrees, merged eagerly so the stack holds one entry per set
//! bit of the chunk count. Portable and single-threaded; a faster
//! implementation may replace it as long as the function is unchanged.

use crate::Digest;

const OUT_LEN: usize = 32;
const BLOCK_LEN: usize = 64;
const CHUNK_LEN: usize = 1024;

const CHUNK_START: u32 = 1 << 0;
const CHUNK_END: u32 = 1 << 1;
const PARENT: u32 = 1 << 2;
const ROOT: u32 = 1 << 3;

const IV: [u32; 8] = [
    0x6A09_E667,
    0xBB67_AE85,
    0x3C6E_F372,
    0xA54F_F53A,
    0x510E_527F,
    0x9B05_688C,
    0x1F83_D9AB,
    0x5BE0_CD19,
];

const MSG_PERMUTATION: [usize; 16] = [2, 6, 3, 10, 7, 0, 4, 13, 1, 11, 12, 5, 9, 14, 15, 8];

/// Enough for 2^54 chunks, that is 2^64 bytes of input.
const MAX_DEPTH: usize = 54;

#[inline]
fn g(state: &mut [u32; 16], a: usize, b: usize, c: usize, d: usize, mx: u32, my: u32) {
    state[a] = state[a].wrapping_add(state[b]).wrapping_add(mx);
    state[d] = (state[d] ^ state[a]).rotate_right(16);
    state[c] = state[c].wrapping_add(state[d]);
    state[b] = (state[b] ^ state[c]).rotate_right(12);
    state[a] = state[a].wrapping_add(state[b]).wrapping_add(my);
    state[d] = (state[d] ^ state[a]).rotate_right(8);
    state[c] = state[c].wrapping_add(state[d]);
    state[b] = (state[b] ^ state[c]).rotate_right(7);
}

#[inline]
fn round(state: &mut [u32; 16], m: &[u32; 16]) {
    // Columns.
    g(state, 0, 4, 8, 12, m[0], m[1]);
    g(state, 1, 5, 9, 13, m[2], m[3]);
    g(state, 2, 6, 10, 14, m[4], m[5]);
    g(state, 3, 7, 11, 15, m[6], m[7]);
    // Diagonals.
    g(state, 0, 5, 10, 15, m[8], m[9]);
    g(state, 1, 6, 11, 12, m[10], m[11]);
    g(state, 2, 7, 8, 13, m[12], m[13]);
    g(state, 3, 4, 9, 14, m[14], m[15]);
}

fn permute(m: &mut [u32; 16]) {
    let mut out = [0u32; 16];
    for (o, &p) in out.iter_mut().zip(MSG_PERMUTATION.iter()) {
        *o = m[p];
    }
    *m = out;
}

fn compress(
    cv: &[u32; 8],
    block: &[u32; 16],
    counter: u64,
    block_len: u32,
    flags: u32,
) -> [u32; 16] {
    let mut state = [
        cv[0],
        cv[1],
        cv[2],
        cv[3],
        cv[4],
        cv[5],
        cv[6],
        cv[7],
        IV[0],
        IV[1],
        IV[2],
        IV[3],
        counter as u32,
        (counter >> 32) as u32,
        block_len,
        flags,
    ];
    let mut m = *block;
    for r in 0..7 {
        round(&mut state, &m);
        if r < 6 {
            permute(&mut m);
        }
    }
    for i in 0..8 {
        state[i] ^= state[i + 8];
        state[i + 8] ^= cv[i];
    }
    state
}

fn first_8(words: &[u32; 16]) -> [u32; 8] {
    let mut out = [0u32; 8];
    out.copy_from_slice(&words[..8]);
    out
}

fn words_of(block: &[u8; BLOCK_LEN]) -> [u32; 16] {
    let mut out = [0u32; 16];
    for (w, b) in out.iter_mut().zip(block.chunks_exact(4)) {
        *w = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
    }
    out
}

/// A node not yet compressed: either the last block of a chunk or a parent.
/// It becomes a chaining value or, if it is the root, the output.
struct Output {
    cv: [u32; 8],
    block: [u32; 16],
    counter: u64,
    block_len: u32,
    flags: u32,
}

impl Output {
    fn chaining_value(&self) -> [u32; 8] {
        first_8(&compress(&self.cv, &self.block, self.counter, self.block_len, self.flags))
    }

    fn root_hash(&self) -> [u8; OUT_LEN] {
        let words = compress(&self.cv, &self.block, 0, self.block_len, self.flags | ROOT);
        let mut out = [0u8; OUT_LEN];
        for (o, w) in out.chunks_exact_mut(4).zip(words.iter()) {
            o.copy_from_slice(&w.to_le_bytes());
        }
        out
    }
}

fn parent_output(left: &[u32; 8], right: &[u32; 8]) -> Output {
    let mut block = [0u32; 16];
    block[..8].copy_from_slice(left);
    block[8..].copy_from_slice(right);
    Output { cv: IV, block, counter: 0, block_len: BLOCK_LEN as u32, flags: PARENT }
}

#[derive(Clone, Debug)]
struct ChunkState {
    cv: [u32; 8],
    chunk_counter: u64,
    block: [u8; BLOCK_LEN],
    block_len: usize,
    blocks_compressed: usize,
}

impl ChunkState {
    fn new(chunk_counter: u64) -> ChunkState {
        ChunkState {
            cv: IV,
            chunk_counter,
            block: [0; BLOCK_LEN],
            block_len: 0,
            blocks_compressed: 0,
        }
    }

    fn len(&self) -> usize {
        BLOCK_LEN * self.blocks_compressed + self.block_len
    }

    fn start_flag(&self) -> u32 {
        if self.blocks_compressed == 0 { CHUNK_START } else { 0 }
    }

    fn update(&mut self, mut input: &[u8]) {
        while !input.is_empty() {
            // Compress a full block only once more input arrives: the last
            // block of a chunk is compressed with CHUNK_END (and maybe ROOT).
            if self.block_len == BLOCK_LEN {
                let words = words_of(&self.block);
                self.cv = first_8(&compress(
                    &self.cv,
                    &words,
                    self.chunk_counter,
                    BLOCK_LEN as u32,
                    self.start_flag(),
                ));
                self.blocks_compressed += 1;
                self.block = [0; BLOCK_LEN];
                self.block_len = 0;
            }
            let take = (BLOCK_LEN - self.block_len).min(input.len());
            self.block[self.block_len..self.block_len + take].copy_from_slice(&input[..take]);
            self.block_len += take;
            input = &input[take..];
        }
    }

    fn output(&self) -> Output {
        Output {
            cv: self.cv,
            block: words_of(&self.block),
            counter: self.chunk_counter,
            block_len: self.block_len as u32,
            flags: self.start_flag() | CHUNK_END,
        }
    }
}

/// Incremental BLAKE3 in the hash mode with 32 bytes of output.
///
/// Feeding the input in any split gives the same digest as one call.
#[derive(Clone, Debug)]
pub struct Hasher {
    chunk: ChunkState,
    cv_stack: [[u32; 8]; MAX_DEPTH],
    cv_stack_len: usize,
}

impl Default for Hasher {
    fn default() -> Hasher {
        Hasher::new()
    }
}

impl Hasher {
    pub fn new() -> Hasher {
        Hasher { chunk: ChunkState::new(0), cv_stack: [[0; 8]; MAX_DEPTH], cv_stack_len: 0 }
    }

    fn push_cv(&mut self, cv: [u32; 8]) {
        debug_assert!(self.cv_stack_len < MAX_DEPTH, "BLAKE3: input exceeds 2^64 bytes");
        self.cv_stack[self.cv_stack_len] = cv;
        self.cv_stack_len += 1;
    }

    fn pop_cv(&mut self) -> [u32; 8] {
        self.cv_stack_len -= 1;
        self.cv_stack[self.cv_stack_len]
    }

    /// Adds the chaining value of a completed chunk, merging every subtree
    /// it completes. `total_chunks` counts chunks completed so far,
    /// including this one: each trailing zero bit is one merge.
    fn add_chunk_cv(&mut self, mut cv: [u32; 8], mut total_chunks: u64) {
        while total_chunks & 1 == 0 {
            cv = parent_output(&self.pop_cv(), &cv).chaining_value();
            total_chunks >>= 1;
        }
        self.push_cv(cv);
    }

    /// Feeds more input.
    pub fn update(&mut self, mut input: &[u8]) -> &mut Self {
        while !input.is_empty() {
            // Finish a full chunk only once more input arrives: the last
            // chunk may be the root.
            if self.chunk.len() == CHUNK_LEN {
                let cv = self.chunk.output().chaining_value();
                let total_chunks = self.chunk.chunk_counter + 1;
                self.add_chunk_cv(cv, total_chunks);
                self.chunk = ChunkState::new(total_chunks);
            }
            let take = (CHUNK_LEN - self.chunk.len()).min(input.len());
            self.chunk.update(&input[..take]);
            input = &input[take..];
        }
        self
    }

    /// The digest of everything fed so far. The hasher is unchanged and may
    /// be fed more.
    pub fn finalize(&self) -> Digest {
        let mut output = self.chunk.output();
        for left in self.cv_stack[..self.cv_stack_len].iter().rev() {
            output = parent_output(left, &output.chaining_value());
        }
        Digest(output.root_hash())
    }
}

/// The tag of an edit set's basis: `H("slip.basis.v1", list<Basis>)`, the
/// shards an edit set was reduced from (§3, `spec/edits.md` §6).
pub const BASIS_TAG: &str = "slip.basis.v1";

/// The content digest of a byte string: `BLAKE3(bytes)` (§3).
pub fn content_digest(bytes: &[u8]) -> Digest {
    Hasher::new().update(bytes).finalize()
}

/// The tagged digest of an encoded value: `BLAKE3(tag ‖ 0x00 ‖ encoded)`
/// (§3). `tag` must be one of the registered ASCII tags; one holding a NUL
/// byte would make the separator ambiguous, and is a programming error.
pub fn tagged_digest(tag: &str, encoded: &[u8]) -> Digest {
    assert!(
        tag.is_ascii() && !tag.contains('\0'),
        "tagged_digest: tag {tag:?} is not ASCII without NUL"
    );
    Hasher::new().update(tag.as_bytes()).update(&[0]).update(encoded).finalize()
}
