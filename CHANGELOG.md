# Changelog

All notable changes to RustFish are documented in this file.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project uses [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added
- GitHub Actions CI: tests and the `bench` node-parity gate on every push and pull request.
- `CONTRIBUTING.md` and `NOTICE` (Stockfish attribution).

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

[Unreleased]: https://github.com/avunit1/rustfish/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/avunit1/rustfish/releases/tag/v0.1.0