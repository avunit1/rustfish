# Changelog

All notable changes to RustFish are documented in this file.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project uses [Semantic Versioning](https://semver.org/).

## [Unreleased]

## [0.2.0] - 2026-11-18

Major performance, Lazy SMP scaling, cross-platform memory, and pointer-safety release: RustFish now surpasses PGO C++ Stockfish in single-thread throughput (`101–104%` NPS) and closes the 8–16 thread scaling gap (`97.6%–99.1%` on the 50-position suite; `111.4%–117.5%` on `bench`), while preserving 100% exact single-thread search parity.

### Added
- `src/memory.rs`: 64-byte cache-line padding (`CachePadded<T>`), non-locked single-writer `RelaxedAtomicU64` counters, and OS page-aligned containers (`PageAlignedSlice<T>`, `PageAlignedBox<T>`) with Windows `VirtualAlloc` / 2 MB `MEM_LARGE_PAGES` (`SeLockMemoryPrivilege` + `GetLargePageMinimum()`) support, Linux 2 MiB `MADV_HUGEPAGE`, and cross-platform 4 KB `std::alloc` fallbacks.
- Persistent OS worker `ThreadPool` (`src/thread.rs`) with condition-variable parking, first-touch thread-local `PageAlignedBox<Worker>` allocation, parallel `TranspositionTable` and `SharedHistories` clearing, and C++ Stockfish's best-worker voting (`get_best_worker_idx`).
- Dynamic UCI version reporting (`id name RustFish 0.2.0` via `CARGO_PKG_VERSION`).
- Unit tests for `Position::cur_state` pointer safety under value moves/clones/resizes, page-aligned and large-page fallback allocation, and dynamic UCI `Hash`/`Threads` resizing.
- GitHub Actions CI: tests and the `bench` node-parity gate on every push and pull request.
- `CONTRIBUTING.md` and `NOTICE` (Stockfish attribution).

### Changed
- **Single-thread throughput (`94–96%` → `101.25%–104.12%` of PGO C++ Stockfish):**
  - Converted large lookup tables (`PSEUDO_ATTACKS`, `RANK_ATTACKS`, `DUAL_MAGICS`, `LINE_BB`, `BETWEEN_BB`, `ZOBRIST`, `REDUCTIONS`, etc.) from `pub const` to `pub static`, eliminating 4 KB stack `memcpy`s and `__chkstk` calls inside `update_piece_threats`.
  - Replaced `apply_combined_dispatch` in `src/nnue/accumulator.rs` with const-generic `#[inline(always)] apply_combined_const<REM, ADD>`, eliminating a 4,904-byte stack frame and Windows x64 `xmm6..xmm15` callee-saved spills.
  - Switched sparse-input `fc_0` (`src/nnue/layers/affine_transform_sparse_input.rs`) to a 4-accumulator AVX2 loop matching C++ Stockfish.
  - Reordered `StateInfo` as `#[repr(C)]` with a 64-byte copied prefix (`ptr::copy_nonoverlapping`) in `do_move`/`do_null_move` and cached `Position::cur_state: *mut StateInfo` (eliminating `state_idx` multiplications on every state read).
  - Refactored `search`, `qsearch`, and history helpers to pass raw `ss: *mut Stack` (with 7 leading and 5 trailing sentinel slots), monomorphized `MovePicker::generate_and_score<GEN>`, and removed all runtime bounds checks and division panics from hot paths (`0` panics, `0` `__chkstk`, `0` `LOCK` instructions across hot functions in release disassembly).
- **High-thread-count Lazy SMP scaling (`71.4%–85.0%` → `97.6%–99.1%` on 50-position suite; `111.4%–117.5%` on `bench 128 8/16 16`):**
  - Replaced per-node `AtomicU64::fetch_add` (`lock xadd`) with relaxed load-add-store (`RelaxedAtomicU64`) and isolated shared control flags and per-thread counters onto separate 64-byte cache lines.
  - Eliminated concurrent heap allocations from `RootMove` PV updates via in-place `clone_from` and `reset_from` reuse.
  - Replaced per-search OS thread spawning and 16× serial `SharedHistories` clearing with the persistent pool and parallel slice clearing.
- Updated `README.md` and `ARCHITECTURE.md` with v0.2.0 benchmark tables, `src/memory.rs` / `StateInfo` / persistent `ThreadPool` documentation, and `RUSTFLAGS="-C target-cpu=native"` + `LLVM_PROFILE_FILE` dual-trained PGO build instructions.

### Fixed
- Removed shadowed branchy `pawn_pair_bb` helper in `src/nnue/features/pp_3wide.rs` so it uses the O(1) table lookup in `src/bitboard.rs`.
- Passed `None` instead of constructing and discarding `Dirties` on early TT cutoffs at `depth >= 7`, skipping redundant `update_piece_threats` work.
- Reordered `ThreadPool` fields so helper OS threads always join before `main_worker` and shared tables drop; tightened `Position::do_move_inner` raw-pointer writes to avoid aliasing across `&mut self` calls.
- Removed redundant `package.homepage` metadata warning in `Cargo.toml` and updated `verify_superiority.py` reference binary path.

### Verified
- `bench 16 1 13`: **1,442,480 nodes** exact (`1,108–1,123 ms`, up to **1,301,877 NPS**, **101.25%** of PGO C++ Stockfish); `bench 128 1 16`: **6,281,007 nodes** exact (**103.13%**).
- 50-position suite (Hash=128 MB): Fixed Depth 22 1T = **46,951,018 nodes** exact (**104.1%**, `46.14s` vs `48.04s`), 4T **99.5%**, 8T **97.6%**, 16T **98.5%**; Fixed Time 2000 ms: 1T **101.6%**, 4T **98.8%**, 8T **99.1%**, 16T **98.7%**; lower peak RSS than C++ Stockfish at every thread count.
- 30-position Tactical EPD suite: 23/30 at 500 ms (Avg TTS **35.2 ms** vs 36.5 ms) and 22/30 at 2000 ms (Avg TTS **62.9 ms** vs 63.8 ms).
- 200-game `fastchess` match vs. PGO C++ Stockfish (5s+0.05s, 1T, 128 MB): **+1.74 ± 12.28 Elo** (W=11, D=179, L=10, LOS 60.93%, 0 crashes / 0 time losses / 0 illegal moves).
- `cargo test --release`: **15/15 tests passing**, zero compiler warnings.

## [0.1.0] - 2026-11-18

First public release: a complete, node-for-node faithful Rust rewrite of Stockfish.

### Added
- Full Stockfish search: Principal Variation Search, transposition table, staged move picker, butterfly / continuation / capture / correction histories, Lazy SMP thread pool, time management.
- Complete NNUE evaluation (full-threats and PP-3wide feature sets, sparse-input affine layers, squared clipped ReLU) with AVX2 intrinsics and incremental accumulator updates. The official `nn-bddd9d57eb87.nnue` network is embedded in the binary.
- Syzygy tablebase probing.
- UCI front-end, including `go perft <depth>` and the standard `bench` command.
- Windows shared-memory mapping of NNUE weights so concurrent engine processes share one physical copy.
- `verify_superiority.py` benchmark suite (throughput and scaling, tactical EPD suite, `fastchess` head-to-head match).
- GitHub Actions release workflow producing Windows, Linux and macOS (Intel) binaries with SHA-256 checksums.

### Verified
- `bench 16 1 13`: **1,442,480 nodes**, identical to C++ Stockfish across all 51 positions.
- Depth 22 over a 50-position suite (1 thread): 46,951,018 nodes, identical to C++ Stockfish.
- NNUE static evaluation matches C++ Stockfish exactly on 20 diverse positions.
- Perft: 6 / 6 test suites pass; `go perft 6` from the start position = 119,060,324 nodes.
- UCI compliance: 40 / 40 (`fastchess --compliance`).
- 200-game match vs. C++ Stockfish (5s+0.05s, 1 thread): +10.43 ± 11.63 Elo with no crashes, time losses or illegal moves.

### Performance
- Single-thread `bench`: ~95% of PGO-compiled C++ Stockfish.
- `go perft 6`: ~15% faster than C++ Stockfish (825 ms vs. 951 ms).
- 8 concurrent `bench` processes: faster than C++ Stockfish (4,897 ms vs. 5,811 ms).

### Known limitations
- Multi-thread scaling at 8 and 16 threads trails Stockfish (about 71-85% of its NPS).
- AVX2-only; no Apple Silicon / ARM support.

[Unreleased]: https://github.com/avunit1/rustfish/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/avunit1/rustfish/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/avunit1/rustfish/releases/tag/v0.1.0