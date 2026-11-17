# Contributing to RustFish

Thanks for your interest in RustFish! This project is a node-for-node faithful Rust port of [Stockfish](https://github.com/official-stockfish/Stockfish), so contributions are judged by one rule above all others:

> **Behavioral parity is the contract.** `bench 16 1 13` must report exactly **1,442,480 nodes** unless a change deliberately tracks an upstream Stockfish change.

## Getting set up

1. Install [Rust](https://rustup.rs) (stable) and [Git LFS](https://git-lfs.com).
2. Clone with LFS enabled (the NNUE network is stored in Git LFS):

   ```bash
   git lfs install
   git clone https://github.com/avunit1/rustfish.git
   cd rustfish/rustfish
   ```

3. Build and test:

   ```bash
   cargo build --release
   cargo test --release
   ./target/release/rustfish bench 16 1 13   # must print 1442480 nodes
   ```

You need a CPU with AVX2 (BMI2 recommended). The primary development platform is Windows x64.

## What a good pull request looks like

- **Keep `bench` at 1,442,480 nodes.** Any change to search, move ordering, evaluation or history updates that alters the node count is a behavioral change. If it is intentional (for example, porting a newer upstream Stockfish commit), say so in the PR description, link the upstream commit, and update the expected value in the tests, `README.md` and CI.
- **Pure performance changes must not change node counts.** Include before/after numbers (`bench` NPS, ideally several runs, plus `go perft 6` timing if you touched move generation).
- **Run the full test suite** (`cargo test --release`). Perft, NNUE evaluation parity and search tests must all pass.
- **No heap allocation in hot paths** (move generation, make/unmake, search, NNUE updates).
- **Document `unsafe`.** Every `unsafe` block, especially SIMD intrinsics and `from_u8_unchecked`-style shortcuts, should have a short comment stating the invariant that makes it sound.
- **Match Stockfish's structure** where practical. `ARCHITECTURE.md` maps each C++ file to its Rust module; keeping that mapping makes upstream changes easier to port.
- **Format and lint:** run `cargo fmt` and `cargo clippy --release`. Existing warnings in the SIMD code may be allowed with a justification.

## Reporting bugs

Please include:

- RustFish version or commit, OS and CPU model
- The UCI command sequence or position (FEN plus moves) that reproduces the issue
- Expected vs. actual behavior (a Stockfish comparison on the same position is very helpful)
- For crashes, the full console output

## Reporting performance regressions

Run `bench 16 1 13` at least three times on an otherwise idle machine and report the NPS range along with your CPU model. Please state whether the binary was a plain `cargo build --release` or a PGO build.

## Benchmark and match testing

`rustfish/verify_superiority.py` runs the throughput, tactical-suite and head-to-head match benchmarks. It needs `python-chess`, `psutil`, [`fastchess`](https://github.com/Disservin/fastchess), a reference Stockfish build and an opening book. Match results for near-identical engines have wide error bars, so treat them as a sanity check, not a verdict.

## License

By contributing you agree that your work is released under the project's license, the [GNU General Public License v3.0](LICENSE), the same license as Stockfish.