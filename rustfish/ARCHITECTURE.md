# RustFish 1:1 Rust Architecture Specification

## 1. Reference C++ Stockfish Baseline Data
- **Git Commit / Version**: `dev-20261115-5d03b6e5`
- **Build Target**: `make -j16 profile-build ARCH=x86-64-avx2 COMP=mingw`
- **Default NNUE Net**: `nn-bddd9d57eb87.nnue` (SHA256: `bddd9d57eb87bf09a6c4e1a3b3f1cc18855c83ee45fdbe18018ac3763b56b38e`, 138,360,528 bytes)
- **Reference Bench Output (`./stockfish.exe bench`)**:
  - **Total Nodes Searched**: `1,442,480`
  - **Elapsed Time**: `1,100 ms`
  - **NPS**: `1,311,345 nodes/sec`
  - **Final Position Best Move**: `bestmove c2c3 ponder g7g6`

---

## 2. Module Mapping: C++ to Rust

| C++ Header / Source | Rust Module (`src/`) | Responsibility & Notes |
| :--- | :--- | :--- |
| `types.h` | `types.rs` | Primitive types (`Square`, `Piece`, `PieceType`, `Color`, `Move`, `Value`, `Depth`, `CastlingRights`, `Bound`), `DirtyPiece`, `DirtyThreat`, `DirtyThreats`, `DirtyPawnPairs`, `Dirties`. |
| `bitboard.h`, `bitboard.cpp` | `bitboard.rs` | 64-bit bitboard constants (`FileABB`..`FileHBB`, `Rank1BB`..`Rank8BB`), bit manipulation (`lsb`, `msb`, `popcount`, `pop_lsb`), ray shifts. |
| `attacks.h`, `attacks.cpp` | `attacks.rs` | AVX2 `DualMagic` hyperbola quintessence slider attacks (`both_attacks_bb`), lookup tables (`LineBB`, `BetweenBB`, `inverse_RayPassBB`, `DiagBB`, `AntiDiagBB`), step attacks (`knight_attack`, `king_attack`, `pawn_attacks_bb`). |
| `position.h`, `position.cpp` | `position.rs` | `#[repr(C)]` `StateInfo` (64-byte copied prefix), `Position` with cached `cur_state: *mut StateInfo`, FEN parser/serializer, `do_move`, `undo_move`, `do_null_move`, `undo_null_move`, `see_ge`, Zobrist hashing, checker/blocker detection. |
| `movegen.h`, `movegen.cpp` | `movegen.rs` | Zero-allocation monomorphized legal move generator (`generate_moves_for<GEN>`). Stack-allocated `MoveList` / `MaybeUninit<Move>` buffers. |
| `perft.h` | `perft.rs` (or in tests) | Perft leaf-node counting engine to validate move generation against test positions. |
| `memory.h`, `memory.cpp` | `memory.rs` | `CachePadded<T>` (64-byte alignment), non-locked `RelaxedAtomicU64`, cross-platform page-aligned & 2 MB large-page allocators (`PageAlignedSlice<T>`, `PageAlignedBox<T>`). |
| `history.h` | `history.rs` | History statistics tables (`StatsEntry`, `ButterflyHistory`, `LowPlyHistory`, `CapturePieceToHistory`, `PieceToHistory`, `ContinuationHistory`, `PawnHistory`, `CorrectionHistory`, `SharedHistories` with parallel `clear_range` and large-page backing). |
| `tt.h`, `tt.cpp` | `tt.rs` | Transposition table with 32-byte `Cluster` (3 `TTEntry` + 64-bit packed 21-bit keys), `TTData`, `TTWriter`, lockless atomic probes/updates, `CachePadded` generation, parallel `clear_range`, large-page backing, `hashfull`. |
| `movepick.h`, `movepick.cpp` | `movepick.rs` | Staged move selector (`MovePicker`) with monomorphized `generate_and_score<GEN>`, `new_qsearch`, pointer-based continuation history, zero null-checks. |
| `timeman.h`, `timeman.cpp` | `timeman.rs` | Time management calculation (`optimumTime`, `maximumTime`, nodes-as-time). |
| `search.h`, `search.cpp` | `search.rs` | Principal Variation Search (`search`, `qsearch`) using raw `ss: *mut Stack` (7 leading / 5 trailing sentinel slots), iterative deepening, aspiration windows, zero-alloc `RootMove` reuse, pruning/LMR/singular extensions, 64-byte aligned `Worker`. |
| `thread.h`, `thread.cpp`, `thread_native.h` | `thread.rs` | Persistent Lazy SMP `ThreadPool` (`HelperThread` parked on `Mutex+Condvar`, first-touch worker allocation, cache-line isolated `SharedControl`, parallel `clear()`/`clear_tt()`, best-worker voting). |
| `score.h`, `score.cpp` | `score.rs` | Centipawn score formatting, mate formatting, WDL conversion. |
| `evaluate.h`, `evaluate.cpp` | `evaluate.rs` | Root evaluation function `evaluate()`, material scaling (`simple_eval`, `scale_evaluation` with optimism and rule50 dampening), trace string. |
| `nnue/nnue_common.h` | `nnue/common.rs` | Constants (`OutputScale = 16`, `WeightScaleBits = 6`, `FtMaxVal = 255`, `HiddenOneVal = 128`), `OrientTBL`, byte ordering. |
| `nnue/simd.h` | `nnue/simd.rs` | AVX2 SIMD vector helpers: `vec_load`, `vec_store`, `_mm256_maddubs_epi16`, `_mm256_madd_epi16`, `_mm256_packus_epi16`, `m256_hadd`, `m256_add_dpbusd_epi32`. |
| `nnue/nnz_helper.h` | `nnue/nnz.rs` | Non-zero feature indices tracking for sparse affine forward propagation. |
| `nnue/features/k32h3.h`, `.cpp` | `nnue/features/k32h3.rs` | King bucket 32-state + 3-block PSQ feature extraction, dirty index calculation. |
| `nnue/features/full_threats.h`, `.cpp` | `nnue/features/threats.rs` | Threat feature extraction (59,808 dimensions), incremental dirty threat tracking. |
| `nnue/features/pp_3wide.h`, `.cpp` | `nnue/features/pawn_pairs.rs` | Pawn pair feature extraction (4,560 dimensions), dirty pawn pair tracking. |
| `nnue/nnue_accumulator.h`, `.cpp` | `nnue/accumulator.rs` | `Accumulator`, `AccumulatorCaches` (Finny Tables), `AccumulatorState`, `AccumulatorStack`, forward/backward incremental updates and cache refreshes. |
| `nnue/nnue_feature_transformer.h` | `nnue/transformer.rs` | `FeatureTransformer`, weight array permutations (`PackusEpi16Order`, `WideningPairOrder`), parameter deserializer, AVX2 `transform_perspective`. |
| `nnue/layers/*.h` | `nnue/layers/*.rs` | `AffineTransformSparseInput` (FC0: 1024 -> 32), `SqrClippedReLU` (AC0: 32), `ClippedReLU` (AC0: 32), `AffineTransform` (FC1: 64 -> 32, FC2: 128 -> 1). |
| `nnue/network.h`, `network.cpp` | `nnue/network.rs` | Network struct containing `FeatureTransformer` + 192 `NetworkArchitecture` layer stacks, embedded default `.nnue` binary, `layer_stack` bucket selector, `evaluate()`. |
| `syzygy/tbprobe.h`, `tbprobe.cpp` | `syzygy/tbprobe.rs` | Syzygy 3-4-5-6-7 man tablebase probe interface (WDL, DTZ, root probe). |
| `uci.h`, `uci.cpp`, `ucioption.h`, `.cpp` | `uci.rs` | UCI protocol loop (`uci`, `isready`, `setoption`, `ucinewgame`, `position`, `go`, `stop`, `quit`, `bench`). |
| `benchmark.h`, `benchmark.cpp` | `benchmark.rs` | Benchmark positions (51 default FENs), `bench` execution, node and NPS aggregation. |
| `misc.h`, `misc.cpp` | `misc.rs` | `PRNG` (xorshift64star), `mul_hi64`, `now()`, sync printing. |
| `main.cpp` | `main.rs` | Binary entry point: parses CLI arguments (`bench`, `compiler`), starts UCI loop. |

---

## 3. Exact Struct Layouts and Memory Alignment

### 3.1 Transposition Table: `Cluster` and `TTEntry`
Stockfish enforces 32-byte cache efficiency for clusters:
```rust
#[repr(C, align(32))]
pub struct Cluster {
    pub keys: AtomicU64,              // Packed 3 x 21-bit Zobrist keys (63 bits used)
    pub entry: [TTEntry; 3],          // 3 entries of 8 bytes = 24 bytes (Total: 32 bytes)
}

#[repr(C)]
pub struct TTEntry {
    pub depth8: AtomicU8,             // Search depth + DEPTH_NONE offset
    pub gen_bound8: AtomicU8,         // Generation (5 bits) | Bound (2 bits) | PV (1 bit)
    pub move16: AtomicU16,            // Best move raw u16
    pub value16: AtomicI16,           // Search score
    pub eval16: AtomicI16,            // Static eval score
}
```

### 3.2 NNUE Accumulator: `Accumulator` and `AccumulatorCaches`
Stockfish aligns accumulator buffers to 64-byte CPU cache lines:
```rust
pub const L1: usize = 1024;
pub const L2: usize = 32;
pub const L3: usize = 32;
pub const LAYER_STACKS: usize = 192;
pub const CACHE_LINE_SIZE: usize = 64;

#[repr(C, align(64))]
#[derive(Clone, Copy)]
pub struct Accumulator {
    pub accumulation: [[i16; L1]; 2], // White and Black perspectives (1024 * 2 * 2 = 4096 bytes)
    pub computed: [bool; 2],
}

#[repr(C, align(64))]
pub struct CacheEntry {
    pub accumulation: [i16; L1],
    pub pieces: [Piece; 64],
    pub piece_bb: Bitboard,
}

pub struct AccumulatorCaches {
    // 3 blocks x 64 squares x 2 colors
    pub entries: [[[CacheEntry; 2]; 64]; 3],
}
```

### 3.3 Stack-Allocated Move Generation & MovePicker
To achieve zero heap allocation per node:
```rust
pub const MAX_MOVES: usize = 256;
pub const MAX_PLY: usize = 246;

pub struct MoveList {
    pub moves: [Move; MAX_MOVES],
    pub count: usize,
}

pub struct ExtMove {
    pub mov: Move,
    pub value: i32,
}

pub struct MovePicker {
    // Embedded stack storage:
    moves: [MaybeUninit<ExtMove>; MAX_MOVES],
    // ...
}
```

### 3.4 `StateInfo` Prefix Layout & `Position::cur_state`
`StateInfo` is `#[repr(C)]` with all fields copied across plies packed into a contiguous 64-byte prefix (`STATE_COPY_BYTES = core::mem::offset_of!(StateInfo, key)`):
```rust
#[repr(C)]
pub struct StateInfo {
    // Copied prefix (64 bytes, 1 cache line) via ptr::copy_nonoverlapping:
    pub material_key: Key,
    pub pawn_key: Key,
    pub minor_piece_key: Key,
    pub non_pawn_key: [Key; COLOR_NB],
    pub non_pawn_material: [Value; COLOR_NB],
    pub castling_rights: u8,
    pub ep_square: Square,
    pub rule50: i32,
    pub plies_from_null: i32,

    // Recomputed per move (not copied in do_move):
    pub key: Key,
    pub checkers_bb: Bitboard,
    pub blockers_for_king: [Bitboard; COLOR_NB],
    pub pinners: [Bitboard; COLOR_NB],
    pub check_squares: [Bitboard; PIECE_TYPE_NB],
    pub threats: UnsafeCell<[Bitboard; PIECE_TYPE_NB]>,
    pub captured_piece: Piece,
    pub repetition: i32,
    pub piece_placement: [u8; 32],
}
```
`Position` keeps `state_stack: Vec<StateInfo>` and `state_idx: usize` private and caches `cur_state: *mut StateInfo` pointing directly to the active heap slot inside `state_stack`. Because `state_stack` resides on the heap, moving `Position` by value preserves pointer validity; every operation that can allocate or re-index (`new`, `clone`, `copy_from`, `ensure_capacity`, `set`, `do_move_inner`, `undo_move`, `do_null_move`, `undo_null_move`) updates `cur_state` immediately.

### 3.5 Memory Allocation & Cache Isolation (`src/memory.rs`)
- **`CachePadded<T>`** (`#[repr(C, align(64))]`): Isolates hot shared control fields (`stop`, `increase_depth`, TT `generation8`) onto separate 64-byte cache lines to prevent false sharing across SMT threads.
- **`RelaxedAtomicU64`**: Single-writer / multi-reader counter whose `inc()` performs a relaxed load-add-store, compiling to a plain non-locked `add`/`inc` on x86-64 rather than hardware `LOCK XADD`.
- **`PageAlignedSlice<T>` & `PageAlignedBox<T>`**:
  - **Windows:** For large buffers (`TranspositionTable`, `SharedHistories`), dynamically resolves `advapi32.dll` (`OpenProcessToken`, `LookupPrivilegeValueA`, `AdjustTokenPrivileges`), temporarily enables `SeLockMemoryPrivilege`, rounds size up to `GetLargePageMinimum()`, and tries `VirtualAlloc(..., MEM_RESERVE | MEM_COMMIT | MEM_LARGE_PAGES, PAGE_READWRITE)`. Falls back cleanly to standard 4 KB `VirtualAlloc` if the privilege is unavailable; freed with `VirtualFree(..., MEM_RELEASE)`.
  - **Linux:** Uses 2 MiB aligned `std::alloc::alloc_zeroed` + `madvise(..., MADV_HUGEPAGE)` for large buffers, 4 KB alignment otherwise, freed with matching `std::alloc::dealloc`.
  - **macOS / other non-Windows:** Uses 4 KB page-aligned `std::alloc::alloc_zeroed` / `std::alloc::dealloc`.

### 3.6 Persistent Lazy SMP `ThreadPool` (`src/thread.rs`)
- **Persistent Worker Pool:** `HelperThread` spawns a dedicated OS thread (8 MiB stack) once per `set_threads` call, allocates and first-touches its `PageAlignedBox<Worker>` inside the helper thread's own OS execution context, and parks on a `(Mutex<JobState>, Condvar)` between jobs.
- **Drop Order Safety:** `helpers: Vec<HelperThread>` is the first field of `ThreadPool`, guaranteeing all helper OS threads are signaled and joined before `main_worker`, `shared_history`, `tt`, or `shared_ctrl` are dropped.
- **Lockless Coordination & Parallel Clear:** `SharedControl` holds cache-line-padded `stop`, `increase_depth`, and raw worker pointers for lockless node/TB-hit/best-move-change aggregation. `clear()` and `clear_tt()` partition `TranspositionTable` and `SharedHistories` clearing across all workers in parallel. On non-fixed-depth searches, `get_best_worker_idx()` applies Stockfish's depth + score voting scheme across all workers.

---

## 4. Zobrist Hashing Constants & Generation
Stockfish uses a PRNG (`xorshift64star`) seeded with `1070372`:
```rust
pub struct ZobristKeys {
    pub psq: [[u64; 64]; 16],
    pub enpassant: [u64; 8],
    pub castling: [u64; 16],
    pub side: u64,
    pub no_pawns: u64,
}
```
Formula for `next()`:
```rust
s ^= s >> 12;
s ^= s << 25;
s ^= s >> 27;
key = s.wrapping_mul(2685821657736338717);
```
Key generation sequence:
1. `psq[pc][sq]` for `pc` in `[W_PAWN, W_KNIGHT, W_BISHOP, W_ROOK, W_QUEEN, W_KING, B_PAWN, B_KNIGHT, B_BISHOP, B_ROOK, B_QUEEN, B_KING]`, `sq` from 0 to 63.
2. Zero out promoting pawn squares: `psq[W_PAWN][A8..H8] = 0`, `psq[B_PAWN][A1..H1] = 0`.
3. `enpassant[0..8]`.
4. `castling[0..16]`.
5. `side`.
6. `no_pawns`.

---

## 5. AVX2 & BMI2 Intrinsics Mapping

1. **Dual-Hyperbola Slider Attacks (`both_attacks_bb`)**:
   - `_mm256_load_si256`: Load precomputed `DualMagic` masks (file, diag, antidiag).
   - `_mm256_sub_epi64`: Compute forward difference `o - r`.
   - `_mm256_shuffle_epi8`: Byte swap within 128-bit lanes.
   - `_mm256_permute4x64_epi64`: Interleave file, diag, antidiag attack components.
2. **NNUE Feature Transformer Accumulation**:
   - `_mm256_load_si256`, `_mm256_store_si256`: Vectorized accumulator loads and stores.
   - `_mm256_add_epi16`, `_mm256_sub_epi16`: Vectorized addition and subtraction of feature weights into accumulator lanes.
   - Paired widening addition via `_mm256_unpacklo_epi8`, `_mm256_unpackhi_epi8`, and `_mm256_maddubs_epi16(x, ones)`.
3. **NNUE Forward Propagation**:
   - Activation clipping: `_mm256_min_epi16`, `_mm256_max_epi16`, `_mm256_slli_epi16`, `_mm256_mulhi_epi16`, `_mm256_packus_epi16`.
   - Affine dot-product: `_mm256_maddubs_epi16`, `_mm256_madd_epi16`, `_mm256_add_epi32`.
   - Horizontal add: `_mm256_extracti128_si256`, `_mm_shuffle_epi32`, `_mm_add_epi32`.
4. **BMI2 / Bit manipulation**:
   - `_pext_u64` (when BMI2 enabled), `popcnt` via `u64::count_ones()`, `ctz` via `u64::trailing_zeros()`, `clz` via `u64::leading_zeros()`.

---

## 6. Verification Milestones & Gating Matrix

- **Gate 1**: Exact Perft node counts on standard test positions (Initial position, Kiwipete, Positions 3..6).
- **Gate 2**: Static NNUE evaluation score parity: 20 diverse FENs must match C++ Stockfish `eval` centipawn values to the exact integer.
- **Gate 3**: Single-thread `rustfish.exe bench` must search exactly `1,442,480` nodes and match best moves node-for-node.
- **Gate 4**: Automated speed, scaling, tactical smartness, and 200-game head-to-head match (>= 0 Elo) vs C++ Stockfish using `fastchess.exe`.

