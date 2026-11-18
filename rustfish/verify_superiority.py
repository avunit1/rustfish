#!/usr/bin/env python3
"""
Milestone 4: Automated Speed & "Smartness" Benchmark Suite
Compares rustfish.exe (Rust PGO) vs stockfish.exe (C++ PGO) across:
  1. Throughput & Scaling (NPS) on 50 FENs across 1, 4, 8, 16 threads at Depth 22 & 2000ms/pos
  2. Tactical & Positional "Smartness" EPD Suite at 500ms & 2000ms/pos
  3. 200-game Head-to-Head Match via fastchess.exe (5s+0.05s, 1T, 128MB Hash, concurrency=8)
"""

import argparse
import json
import os
import re
import subprocess
import sys
import time
from pathlib import Path

import chess
import chess.engine
import psutil

CPP_BIN = r"D:\Coding\cltest\tools\Stockfish\src\stockfish.exe"
RUST_BIN = r"D:\Coding\cltest\rustfish\target\x86_64-pc-windows-msvc\release\rustfish.exe"
BOOK_PGN = r"D:\Coding\cltest\tools\8moves_v3.pgn"
OUT_DIR = Path(r"D:\Coding\cltest\rustfish")

# 50 diverse positions: opening (10), middlegame (18), tactical (12), endgame (10)
SUITE_50_FENS = [
    # --- Openings (10) ---
    "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
    "rnbqkbnr/pp1ppppp/8/2p5/4P3/8/PPPP1PPP/RNBQKBNR w KQkq c6 0 2",
    "r1bqkbnr/pppp1ppp/2n5/1B2p3/4P3/5N2/PPPP1PPP/RNBQK2R b KQkq - 3 3",
    "rnbqkb1r/ppp2ppp/4pn2/3p4/2PP4/2N5/PP2PPPP/R1BQKBNR w KQkq - 0 4",
    "rnbqkb1r/pppppp1p/5np1/8/2PP4/2N5/PP2PPPP/R1BQKBNR w KQkq - 1 4",
    "r1bqk2r/pppp1ppp/2n2n2/2b1p3/2B1P3/2P2N2/PP1P1PPP/RNBQK2R w KQkq - 1 5",
    "rnbqk2r/ppp1bppp/4pn2/3p4/2PP4/2N2N2/PP2PPPP/R1BQKB1R w KQkq - 4 5",
    "r1bqk2r/pp2bppp/2n1pn2/2pp4/3P4/2NBPN2/PPP2PPP/R1BQ1RK1 w kq - 2 7",
    "r1bq1rk1/2ppbppp/p1n2n2/1p2p3/4P3/1B3N2/PPPP1PPP/RNBQR1K1 w - - 2 8",
    "r2q1rk1/1b1nbppp/pp1ppn2/8/2PNP3/2N1B3/PPQ1BPPP/R4RK1 w - - 2 11",
    # --- Middlegames (18) ---
    "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 10",
    "4rrk1/pp1n3p/3q2pQ/2p1pb2/2PP4/2P3N1/P2B2PP/4RRK1 b - - 7 19",
    "r3r1k1/2p2ppp/p1p1bn2/8/1q2P3/2NPQN2/PPP3PP/R4RK1 b - - 2 15",
    "r1bbk1nr/pp3p1p/2n5/1N4p1/2Np1B2/8/PPP2PPP/2KR1B1R w kq - 0 13",
    "r1bq1rk1/ppp1nppp/4n3/3p3Q/3P4/1BP1B3/PP1N2PP/R4RK1 w - - 1 16",
    "4r1k1/r1q2ppp/ppp2n2/4P3/5Rb1/1N1BQ3/PPP3PP/R5K1 w - - 1 17",
    "2rqkb1r/ppp2p2/2npb1p1/1N1Nn2p/2P1PP2/8/PP2B1PP/R1BQK2R b KQ - 0 11",
    "r1bq1r1k/b1p1npp1/p2p3p/1p6/3PP3/1B2NN2/PP3PPP/R2Q1RK1 w - - 1 16",
    "3r1rk1/p5pp/bpp1pp2/8/q1PP1P2/b3P3/P2NQRPP/1R2B1K1 b - - 6 22",
    "r1q2rk1/2p1bppp/2Pp4/p6b/Q1PNp3/4B3/PP1R1PPP/2K4R w - - 2 18",
    "4k2r/1pb2ppp/1p2p3/1R1p4/3P4/2r1PN2/P4PPP/1R4K1 b - - 3 22",
    "3q2k1/pb3p1p/4pbp1/2r5/PpN2N2/1P2P2P/5PP1/Q2R2K1 b - - 4 26",
    "5rk1/q6p/2p3bR/1pPp1rP1/1P1Pp3/P3B1Q1/1K3P2/R7 w - - 93 90",
    "4rrk1/1p1nq3/p7/2p1P1pp/3P2bp/3Q1Bn1/PPPB4/1K2R1NR w - - 40 21",
    "r3k2r/3nnpbp/q2pp1p1/p7/Pp1PPPP1/4BNN1/1P5P/R2Q1RK1 w kq - 0 16",
    "3Qb1k1/1r2ppb1/pN1n2q1/Pp1Pp1Pr/4P2p/4BP2/4B1R1/1R5K b - - 11 40",
    "1r6/1P4bk/3qr1p1/N6p/3pp2P/6R1/3Q1PP1/1R4K1 w - - 1 42",
    "r1b2rk1/pp2bppp/1qn1pn2/3p4/2PP4/2N1PN2/PP2BPPP/R1BQ1RK1 b - - 2 9",
    # --- Tactical (12) ---
    "rq3rk1/ppp2ppp/1bnpb3/3N2B1/3NP3/7P/PPPQ1PP1/2KR3R w - - 7 14",
    "r1bq1r1k/1pp1n1pp/1p1p4/4p2Q/4Pp2/1BNP4/PPP2PPP/3R1RK1 w - - 2 14",
    "4k3/3q1r2/1N2r1b1/3ppN2/2nPP3/1B1R2n1/2R1Q3/3K4 w - - 5 1",
    "6k1/3b3r/1p1p4/p1n2p2/1PPNpP1q/P3Q1p1/1R1RB1P1/5K2 b - - 0 1",
    "r2r1n2/pp2bk2/2p1p2p/3q4/3PN1QP/2P3R1/P4PP1/5RK1 w - - 0 1",
    "r1b1k2r/ppppnppp/2n2q2/2b5/3NP3/2P1B3/PP3PPP/RN1QKB1R w KQkq - 0 1",
    "2r2rk1/1bqnbpp1/1p1ppn1p/pP6/N1P1P3/P2B1N1P/1B2QPP1/R2R2K1 b - - 0 1",
    "r3r1k1/ppqb1ppp/8/4p1NQ/8/2P5/PP3PPP/R3R1K1 b - - 0 1",
    "1r3k2/4q3/2Pp3b/3Bp3/2Q2p2/1p1P2P1/1P2KP2/3N4 w - - 0 1",
    "6k1/4pp1p/3p2p1/P1pPb3/R7/1r2P1PP/3B1P2/6K1 w - - 0 1",
    "r4rk1/1pp1qppp/p1np1n2/2b1p1B1/2B1P1b1/P1NP1N2/1PP1QPPP/R4RK1 w - - 0 10",
    "r2q1rk1/pb1nbppp/1p2pn2/2p5/2PP4/2N1PN2/PP2BPPP/R1BQ1RK1 w - - 2 10",
    # --- Endgames (10) ---
    "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 11",
    "6k1/6p1/6Pp/ppp5/3pn2P/1P3K2/1PP2P2/3N4 b - - 0 1",
    "3b4/5kp1/1p1p1p1p/pP1PpP1P/P1P1P3/3KN3/8/8 w - - 0 1",
    "8/6pk/1p6/8/PP3p1p/5P2/4KP1q/3Q4 w - - 0 1",
    "7k/3p2pp/4q3/8/4Q3/5Kp1/P6b/8 w - - 0 1",
    "8/2p5/8/2kPKp1p/2p4P/2P5/3P4/8 w - - 0 1",
    "8/1p3pp1/7p/5P1P/2k3P1/8/2K2P2/8 w - - 0 1",
    "8/pp2r1k1/2p1p3/3pP2p/1P1P1P1P/P5KR/8/8 w - - 0 1",
    "8/3p4/p1bk3p/Pp6/1Kp1PpPp/2P2P1P/2P5/5B2 b - - 0 1",
    "6k1/6p1/P6p/r1N5/5p2/7P/1b3PP1/4R1K1 w - - 0 1",
]

# Curated 30-position Tactical & Positional "Smartness" EPD Suite
# Format: (id, category, fen, [accepted best moves in UCI])
TACTICAL_SUITE = [
    # --- Mate-in-N (8) ---
    ("MAT-01", "Mate-in-N", "2q1nk1r/4Rp2/1ppp1P2/6Pp/3p1B2/3P3P/PPP1Q3/6K1 w - - 0 1", ["e7e8"]),
    ("MAT-02", "Mate-in-N", "r1b1kb1r/pppp1ppp/5q2/4n3/3KP3/2N3PN/PPP4P/R1BQ1B1R b kq - 0 1", ["f8c5"]),
    ("MAT-03", "Mate-in-N", "5rk1/1p1q2bp/p2pN1p1/2pP2Bn/2P3P1/1P6/P4QKP/5R2 w - - 0 1", ["f2f8"]),
    ("MAT-04", "Mate-in-N", "r3k2r/ppp2Npp/1b5n/4p2b/2B1P2q/BQP2P2/P5PP/RN5K w kq - 0 1", ["c4b5"]),
    ("MAT-05", "Mate-in-N", "6k1/pp3p1p/2p2np1/3p4/3P1P2/2N1r3/PPQ3PP/2R2R1K b - - 0 1", ["f6g4", "e3e1", "e3c3"]),
    ("MAT-06", "Mate-in-N", "r5k1/5p1p/6p1/3P4/8/2Q3P1/5P1P/4qBK1 w - - 0 1", ["c3h8"]),
    ("MAT-07", "Mate-in-N", "2r3k1/p4p2/3Rp2p/1p2P1pK/8/1P4P1/P3Q2P/1q6 b - - 0 1", ["b1g6"]),
    ("MAT-08", "Mate-in-N", "r1b2rk1/pp3ppp/2n1p3/q1bpP3/8/P1PB1N2/2P2PPP/R1BQK2R w KQ - 0 1", ["d3h7"]),
    # --- Sharp Tactics / WAC (10) ---
    ("WAC-001", "Tactical", "2rr3k/pp3pp1/1nnqbN1p/3pN3/2pP4/2P3Q1/PPB4P/R4RK1 w - - 0 1", ["g3g6"]),
    ("WAC-002", "Tactical", "8/7p/5k2/5p2/p1p2P2/Pr1pPK2/1P1R3P/8 b - - 0 1", ["b3b2"]),
    ("WAC-003", "Tactical", "5rk1/1ppb3p/p1pb4/6q1/3P1p1r/2P1R2P/PP1BQ1P1/5RKN w - - 0 1", ["e3g3"]),
    ("WAC-004", "Tactical", "r1bq2rk/pp3pbp/2p1p1pQ/7P/3P4/2PB1N2/PP3PPR/2KR4 w - - 0 1", ["h6h7"]),
    ("WAC-010", "Tactical", "2br2k1/2q3rn/p2NppQ1/2p1P3/Pp5R/4P3/1P3PPP/3R2K1 w - - 0 1", ["h4h7"]),
    ("WAC-014", "Tactical", "r2rb1k1/pp1q1p1p/2n1p1p1/2bp4/5P2/PP1BPR1Q/1BPN2PP/R5K1 w - - 0 1", ["h3h7"]),
    ("WAC-021", "Tactical", "r1bqr1k1/1p1nbppp/p2p4/3P4/3N4/2N1B3/PP2BPPP/R2Q1RK1 b - - 0 1", ["e7f6", "d7e5", "d7f6"]),
    ("WAC-031", "Tactical", "3r2k1/pb1q1pp1/1p2pb1p/8/3N4/P2QB3/1P3PPP/1Br1R1K1 w - - 0 1", ["d3h7"]),
    ("WAC-046", "Tactical", "r1b3k1/ppp2ppp/8/4r3/2P2n2/1P1R1N2/P4PPP/R5K1 b - - 0 1", ["f4e2"]),
    ("WAC-100", "Tactical", "r1bk3r/pppq1ppp/5n2/4N1N1/2Bp4/Bn6/P4PPP/4R1K1 w - - 0 1", ["e5f7"]),
    # --- Zugzwang & Endgame Studies (6) ---
    ("ZUG-01", "Zugzwang", "8/8/p1p5/1p5p/1P5p/8/PPP2K1p/4R1rk w - - 0 1", ["e1f1"]),
    ("ZUG-02", "Zugzwang", "8/k7/3p4/p2P1p2/P2P1P2/8/8/K7 w - - 0 1", ["a1b1"]),
    ("ZUG-03", "Zugzwang", "1k6/1p6/1P6/8/8/5B2/8/4K3 w - - 0 1", ["f3c6", "e1d2", "e1f2", "e1e2"]),
    ("ZUG-04", "Zugzwang", "8/5p2/8/6k1/8/5K2/6P1/8 w - - 0 1", ["f3e4", "f3g3", "g2g3"]),
    ("ZUG-05", "Zugzwang", "8/8/1p1r1k2/p1pPN1p1/P3KnP1/1P6/8/3R4 b - - 0 1", ["f4d5", "f6e7"]),
    ("ZUG-06", "Zugzwang", "8/p7/8/1P6/K1k3p1/6P1/7P/8 w - - 0 1", ["h2h4", "b5b6"]),
    # --- Quiet Positional Sacrifices & Deep Positional (6) ---
    ("POS-01", "Positional", "1q1r3k/3P1pp1/ppBR1n1p/4Q2P/P4P2/8/5PK1/8 w - - 0 1", ["d6f6"]),
    ("POS-02", "Positional", "r4rk1/1pp1qppp/p1np1n2/2b1p1B1/2B1P1b1/P1NP1N2/1PP1QPPP/R4RK1 w - - 0 10", ["c3d5", "h2h3"]),
    ("POS-03", "Positional", "2r1r1k1/pb1n1pbp/1p4p1/3pP3/q1pP4/P1N2N1P/1PB2PP1/R2QR1K1 b - - 0 1", ["a4a5", "a4c6"]),
    ("POS-04", "Positional", "r1b2rk1/2q1bppp/p2ppn2/1p6/3QP3/1BN1B3/PPP2PPP/R4RK1 w - - 0 12", ["a2a4", "f2f3", "a1d1", "a1c1"]),
    ("POS-05", "Positional", "r2q1rk1/pp2bppp/2p1bn2/4p3/4P3/1NN1B3/PPP1BPPP/R2Q1RK1 w - - 0 10", ["f2f4", "d1d8", "a2a4"]),
    ("POS-06", "Positional", "r1bq1rk1/pp2nppp/2n1p3/3pP3/3P4/P1PB1N2/2P2PPP/R1BQ1RK1 w - - 0 10", ["d3h7"]),
]


def get_engine_pid(engine: chess.engine.SimpleEngine) -> int | None:
    try:
        return engine.transport.get_pid()
    except Exception:
        pass
    try:
        return engine._protocol.transport.get_pid()
    except Exception:
        return None


def get_peak_rss_mb(pid: int | None) -> float:
    if pid is None:
        return 0.0
    try:
        proc = psutil.Process(pid)
        mi = proc.memory_info()
        # Windows psutil exposes peak_wset (peak working set / RSS)
        peak = getattr(mi, "peak_wset", mi.rss)
        return peak / (1024.0 * 1024.0)
    except Exception:
        return 0.0


def run_part1_nps(threads_list=(1, 4, 8, 16), depth_target=22, time_ms=2000, hash_mb=128):
    print("\n" + "=" * 78)
    print("PART 1: THROUGHPUT & SCALING (NPS) — 50 POSITIONS")
    print("=" * 78)

    engines = [
        ("C++ Stockfish", CPP_BIN),
        ("Rust rustfish", RUST_BIN),
    ]

    results = {
        "binary_size_mb": {
            name: round(os.path.getsize(path) / (1024.0 * 1024.0), 2)
            for name, path in engines
        },
        "depth_fixed": {},
        "time_fixed": {},
        "peak_rss_mb": {},
    }

    for name, path in engines:
        print(f"Binary size [{name}]: {results['binary_size_mb'][name]:.2f} MB")

    # Mode A: Fixed Depth (depth 22)
    print(f"\n--- Mode A: Fixed Depth (depth={depth_target}, Hash={hash_mb}MB, 50 positions) ---")
    for threads in threads_list:
        results["depth_fixed"][str(threads)] = {}
        results["peak_rss_mb"][str(threads)] = {}
        for name, path in engines:
            engine = chess.engine.SimpleEngine.popen_uci(path)
            pid = get_engine_pid(engine)
            engine.configure({"Threads": threads, "Hash": hash_mb})

            total_nodes = 0
            total_wall = 0.0
            for idx, fen in enumerate(SUITE_50_FENS):
                board = chess.Board(fen)
                t0 = time.perf_counter()
                info = engine.analyse(board, chess.engine.Limit(depth=depth_target), game=idx)
                dt = time.perf_counter() - t0
                nodes = info.get("nodes", 0)
                total_nodes += nodes
                total_wall += dt

            peak_rss = get_peak_rss_mb(pid)
            engine.quit()

            agg_nps = int(total_nodes / max(total_wall, 1e-6))
            results["depth_fixed"][str(threads)][name] = {
                "total_nodes": total_nodes,
                "total_time_s": round(total_wall, 3),
                "nps": agg_nps,
            }
            results["peak_rss_mb"][str(threads)][name] = round(peak_rss, 2)
            print(
                f"  [Depth {depth_target:2d} | {threads:2d}T] {name:<18}: "
                f"nodes={total_nodes:>12,}  time={total_wall:6.2f}s  "
                f"NPS={agg_nps:>10,}  PeakRSS={peak_rss:6.1f} MB"
            )
            sys.stdout.flush()

    # Mode B: Fixed Time (2000ms/pos)
    print(f"\n--- Mode B: Fixed Time ({time_ms}ms/pos, Hash={hash_mb}MB, 50 positions) ---")
    limit_sec = time_ms / 1000.0
    for threads in threads_list:
        results["time_fixed"][str(threads)] = {}
        for name, path in engines:
            engine = chess.engine.SimpleEngine.popen_uci(path)
            pid = get_engine_pid(engine)
            engine.configure({"Threads": threads, "Hash": hash_mb})

            total_nodes = 0
            total_engine_time = 0.0
            total_depth = 0
            for idx, fen in enumerate(SUITE_50_FENS):
                board = chess.Board(fen)
                t0 = time.perf_counter()
                info = engine.analyse(board, chess.engine.Limit(time=limit_sec), game=idx)
                dt = time.perf_counter() - t0
                nodes = info.get("nodes", 0)
                depth = info.get("depth", 0)
                etime = info.get("time", dt)
                total_nodes += nodes
                total_engine_time += max(etime, 0.001)
                total_depth += depth

            peak_rss = get_peak_rss_mb(pid)
            engine.quit()

            agg_nps = int(total_nodes / max(total_engine_time, 1e-6))
            avg_depth = round(total_depth / len(SUITE_50_FENS), 2)
            results["time_fixed"][str(threads)][name] = {
                "total_nodes": total_nodes,
                "total_time_s": round(total_engine_time, 3),
                "nps": agg_nps,
                "avg_depth": avg_depth,
            }
            prev_rss = results["peak_rss_mb"][str(threads)].get(name, 0.0)
            results["peak_rss_mb"][str(threads)][name] = round(max(prev_rss, peak_rss), 2)
            print(
                f"  [{time_ms:4d}ms | {threads:2d}T] {name:<18}: "
                f"nodes={total_nodes:>12,}  time={total_engine_time:6.2f}s  "
                f"NPS={agg_nps:>10,}  avgDepth={avg_depth:5.2f}  PeakRSS={peak_rss:6.1f} MB"
            )
            sys.stdout.flush()

    return results


def run_part2_tactical(times_ms=(500, 2000), hash_mb=128):
    print("\n" + "=" * 78)
    print(f"PART 2: TACTICAL & POSITIONAL 'SMARTNESS' SUITE ({len(TACTICAL_SUITE)} POSITIONS)")
    print("=" * 78)

    engines = [
        ("C++ Stockfish", CPP_BIN),
        ("Rust rustfish", RUST_BIN),
    ]

    results = {}

    for t_ms in times_ms:
        t_sec = t_ms / 1000.0
        results[str(t_ms)] = {}
        print(f"\n--- Time per position: {t_ms}ms (1 thread, Hash={hash_mb}MB) ---")

        for name, path in engines:
            engine = chess.engine.SimpleEngine.popen_uci(path)
            engine.configure({"Threads": 1, "Hash": hash_mb})

            solved = 0
            solve_times = []
            details = []

            for idx, (pid_str, cat, fen, accepted_uci) in enumerate(TACTICAL_SUITE):
                board = chess.Board(fen)
                accepted_set = set(accepted_uci)

                first_solved_ms = None
                last_move = None
                final_depth = 0
                t0 = time.perf_counter()

                with engine.analysis(board, chess.engine.Limit(time=t_sec), game=idx) as analysis:
                    for info in analysis:
                        pv = info.get("pv")
                        if pv:
                            mv_uci = pv[0].uci()
                            elapsed_ms = info.get("time", time.perf_counter() - t0) * 1000.0
                            final_depth = info.get("depth", final_depth)
                            if mv_uci in accepted_set:
                                if first_solved_ms is None:
                                    first_solved_ms = elapsed_ms
                            else:
                                first_solved_ms = None
                            last_move = mv_uci

                is_solved = last_move in accepted_set
                if is_solved:
                    solved += 1
                    st = first_solved_ms if first_solved_ms is not None else t_ms
                    solve_times.append(st)
                details.append({
                    "id": pid_str,
                    "category": cat,
                    "solved": is_solved,
                    "move": last_move,
                    "solve_ms": round(first_solved_ms, 1) if is_solved and first_solved_ms is not None else None,
                    "depth": final_depth,
                })

            engine.quit()

            solve_rate = 100.0 * solved / len(TACTICAL_SUITE)
            avg_tts = sum(solve_times) / len(solve_times) if solve_times else 0.0
            results[str(t_ms)][name] = {
                "solved": solved,
                "total": len(TACTICAL_SUITE),
                "solve_rate_pct": round(solve_rate, 2),
                "avg_solve_ms": round(avg_tts, 2),
                "details": details,
            }
            print(
                f"  [{t_ms:4d}ms] {name:<18}: "
                f"Solved {solved:2d}/{len(TACTICAL_SUITE)} ({solve_rate:5.1f}%)  "
                f"Avg TTS = {avg_tts:6.1f} ms"
            )
            sys.stdout.flush()

    return results


def run_part3_fastchess(rounds=100, concurrency=8, hash_mb=128):
    print("\n" + "=" * 78)
    print(f"PART 3: HEAD-TO-HEAD ENGINE MATCH (fastchess.exe, {rounds * 2} games, 5s+0.05s)")
    print("=" * 78)

    pgn_out = OUT_DIR / "fastchess_match.pgn"
    if pgn_out.exists():
        pgn_out.unlink()

    cmd = [
        "fastchess.exe",
        "-engine", f"cmd={RUST_BIN}", "name=rustfish",
        "-engine", f"cmd={CPP_BIN}", "name=stockfish-cpp",
        "-each", "tc=5+0.05", "option.Threads=1", f"option.Hash={hash_mb}",
        "-openings", f"file={BOOK_PGN}", "format=pgn", "order=random", "plies=8",
        "-rounds", str(rounds),
        "-repeat",
        "-concurrency", str(concurrency),
        "-pgnout", f"file={pgn_out}",
    ]
    print("Command:", " ".join(cmd))
    sys.stdout.flush()

    proc = subprocess.Popen(
        cmd, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, bufsize=1
    )
    output_lines = []
    for line in proc.stdout:
        output_lines.append(line)
        if any(k in line for k in [
            "Score of", "Elo difference", "Elo:", "SPRT", "Finished game",
            "Games:", "Wins:", "Time losses", "illegal", "crash", "disconnect",
            "Rank", "Ptnml", "LOS"
        ]):
            print(line, end="")
            sys.stdout.flush()
    proc.wait()

    full_out = "".join(output_lines)

    # Parse final Score / Elo / WDL
    wins = losses = draws = 0
    elo_diff = 0.0
    elo_err = 0.0
    los = ""

    score_matches = re.findall(
        r"Games:\s*(\d+),\s*Wins:\s*(\d+),\s*Losses:\s*(\d+),\s*Draws:\s*(\d+)",
        full_out,
    )
    if score_matches:
        _, wins, losses, draws = map(int, score_matches[-1])
    else:
        score_matches2 = re.findall(
            r"Score of rustfish vs stockfish-cpp:\s*(\d+)\s*-\s*(\d+)\s*-\s*(\d+)",
            full_out,
        )
        if score_matches2:
            wins, losses, draws = map(int, score_matches2[-1])

    elo_matches = re.findall(
        r"(?m)^Elo(?:\s+difference)?:\s*([+-]?\d+(?:\.\d+)?|inf|-inf|nan)\s*\+/-\s*(\d+(?:\.\d+)?|inf|nan)",
        full_out,
    )
    if elo_matches:
        try:
            elo_diff = float(elo_matches[-1][0])
        except ValueError:
            elo_diff = 0.0
        try:
            elo_err = float(elo_matches[-1][1])
        except ValueError:
            elo_err = 0.0

    los_matches = re.findall(r"LOS:\s*([0-9.]+ %)", full_out)
    if los_matches:
        los = los_matches[-1]

    # Check PGN for crashes, time forfeits, illegal moves
    time_losses = 0
    illegal_moves = 0
    crashes = 0
    total_pgn_games = 0
    if pgn_out.exists():
        pgn_text = pgn_out.read_text(encoding="utf-8", errors="ignore")
        total_pgn_games = pgn_text.count("[Event ")
        time_losses = len(re.findall(r"forfeits on time|time forfeit", pgn_text, re.I))
        illegal_moves = len(re.findall(r"illegal move", pgn_text, re.I))
        crashes = len(re.findall(r"disconnect|crash|stalled", pgn_text, re.I))

    res = {
        "returncode": proc.returncode,
        "games": wins + losses + draws,
        "pgn_games": total_pgn_games,
        "wins": wins,
        "losses": losses,
        "draws": draws,
        "elo_diff": elo_diff,
        "elo_err": elo_err,
        "los": los,
        "time_losses": time_losses,
        "illegal_moves": illegal_moves,
        "crashes": crashes,
        "raw_tail": "\n".join(output_lines[-30:]),
    }

    print("\n--- Fastchess Match Summary ---")
    print(f"  Games: {res['games']} (W={wins}, D={draws}, L={losses})")
    print(f"  Elo difference: {elo_diff:+.2f} +/- {elo_err:.2f} (LOS: {los})")
    print(f"  Time losses: {time_losses}, Illegal moves: {illegal_moves}, Crashes: {crashes}")
    return res


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--part", choices=["1", "2", "3", "all"], default="all")
    args = parser.parse_args()

    results_path = OUT_DIR / "benchmark_results.json"
    all_results = {}
    if results_path.exists():
        try:
            all_results = json.loads(results_path.read_text(encoding="utf-8"))
        except Exception:
            all_results = {}

    if args.part in ("1", "all"):
        all_results["part1_nps"] = run_part1_nps()
        results_path.write_text(json.dumps(all_results, indent=2), encoding="utf-8")

    if args.part in ("2", "all"):
        all_results["part2_tactical"] = run_part2_tactical()
        results_path.write_text(json.dumps(all_results, indent=2), encoding="utf-8")

    if args.part in ("3", "all"):
        all_results["part3_fastchess"] = run_part3_fastchess()
        results_path.write_text(json.dumps(all_results, indent=2), encoding="utf-8")

    print(f"\nSaved benchmark results to {results_path}")


if __name__ == "__main__":
    main()

