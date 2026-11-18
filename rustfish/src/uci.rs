use std::io::{self, BufRead};
use crate::benchmark::run_benchmark;
use crate::movegen::*;
use crate::position::*;
use crate::thread::ThreadPool;
use crate::timeman::LimitsType;
use crate::types::*;

pub fn square_to_string(sq: Square) -> String {
    if sq == Square::SqNone {
        return "-".to_string();
    }
    let f = (b'a' + file_of(sq) as u8) as char;
    let r = (b'1' + rank_of(sq) as u8) as char;
    format!("{}{}", f, r)
}

pub fn move_to_string(m: Move, chess960: bool) -> String {
    if m == Move::none() {
        return "(none)".to_string();
    }
    if m == Move::null() {
        return "0000".to_string();
    }

    let from = m.from_sq();
    let mut to = m.to_sq();

    if m.move_type() == MoveType::Castling && !chess960 {
        let us = if rank_of(from) == Rank::Rank1 {
            Color::White
        } else {
            Color::Black
        };
        let king_side = (to as u8) > (from as u8);
        to = relative_square(us, if king_side { Square::SqG1 } else { Square::SqC1 });
    }

    let mut res = format!("{}{}", square_to_string(from), square_to_string(to));
    if m.move_type() == MoveType::Promotion {
        let promo_char = match m.promotion_type() {
            PieceType::Knight => 'n',
            PieceType::Bishop => 'b',
            PieceType::Rook => 'r',
            PieceType::Queen => 'q',
            _ => ' ',
        };
        res.push(promo_char);
    }
    res
}

pub fn format_move(m: Move, chess960: bool) -> String {
    move_to_string(m, chess960)
}

pub fn parse_move(pos: &mut Position, s: &str) -> Option<Move> {
    if s.len() < 4 {
        return None;
    }
    let bytes = s.as_bytes();
    if bytes[0] < b'a' || bytes[0] > b'h' || bytes[1] < b'1' || bytes[1] > b'8' {
        return None;
    }
    if bytes[2] < b'a' || bytes[2] > b'h' || bytes[3] < b'1' || bytes[3] > b'8' {
        return None;
    }

    let from = make_square(File::from_u8(bytes[0] - b'a'), Rank::from_u8(bytes[1] - b'1'));
    let to = make_square(File::from_u8(bytes[2] - b'a'), Rank::from_u8(bytes[3] - b'1'));

    let promo = if s.len() >= 5 {
        match bytes[4] {
            b'n' | b'N' => PieceType::Knight,
            b'b' | b'B' => PieceType::Bishop,
            b'r' | b'R' => PieceType::Rook,
            b'q' | b'Q' => PieceType::Queen,
            _ => PieceType::NoPieceType,
        }
    } else {
        PieceType::NoPieceType
    };

    let mut list = [Move::none(); MAX_MOVES];
    let count = generate_moves_all(pos, &mut list, 0);

    for &m in &list[..count] {
        if m.from_sq() == from {
            if m.move_type() == MoveType::Castling && !pos.is_chess960() {
                let us = pos.side_to_move();
                let king_side = (m.to_sq() as u8) > (from as u8);
                let standard_to = relative_square(us, if king_side { Square::SqG1 } else { Square::SqC1 });
                if to == standard_to {
                    return Some(m);
                }
            } else if m.to_sq() == to {
                if m.move_type() == MoveType::Promotion {
                    if m.promotion_type() == promo {
                        return Some(m);
                    }
                } else if promo == PieceType::NoPieceType {
                    return Some(m);
                }
            }
        }
    }

    None
}

pub fn perft_nodes<const ROOT: bool>(pos: &mut Position, depth: u32) -> u64 {
    let mut moves = [const { core::mem::MaybeUninit::<Move>::uninit() }; MAX_MOVES];
    let count = generate_moves_uninit(pos, &mut moves, 0, GenType::All);
    if !ROOT && depth == 1 {
        return count as u64;
    }

    let mut nodes: u64 = 0;
    let leaf = depth == 2;
    for i in 0..count {
        let m = unsafe { moves.get_unchecked(i).assume_init() };
        let cnt = if ROOT && depth <= 1 {
            1
        } else {
            let gives_check = pos.gives_check(m);
            pos.do_move(m, gives_check, None);
            let c = if leaf {
                let mut leaf_moves = [const { core::mem::MaybeUninit::<Move>::uninit() }; MAX_MOVES];
                generate_moves_uninit(pos, &mut leaf_moves, 0, GenType::All) as u64
            } else {
                perft_nodes::<false>(pos, depth - 1)
            };
            pos.undo_move(m);
            c
        };
        nodes += cnt;
        if ROOT {
            println!("{}: {}", format_move(m, pos.is_chess960()), cnt);
        }
    }
    nodes
}

pub fn uci_loop() {
    let mut pool = ThreadPool::new(1, 16);
    let mut pos = Position::new();
    pos.set(START_FEN, false).unwrap();
    let mut chess960 = false;

    let stdin = io::stdin();
    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let mut tokens = line.split_whitespace();
        let cmd = match tokens.next() {
            Some(c) => c,
            None => continue,
        };

        match cmd {
            "uci" => {
                println!("id name RustFish {}", env!("CARGO_PKG_VERSION"));
                println!("id author Google DeepMind / Antigravity");
                println!("option name Hash type spin default 16 min 1 max 33554432");
                println!("option name Threads type spin default 1 min 1 max 1024");
                println!("option name UCI_Chess960 type check default false");
                println!("uciok");
            }
            "isready" => {
                println!("readyok");
            }
            "ucinewgame" => {
                pool.clear();
            }
            "setoption" => {
                let mut name = String::new();
                let mut val = String::new();
                let mut reading_name = false;
                let mut reading_val = false;
                for t in tokens {
                    if t == "name" {
                        reading_name = true;
                        reading_val = false;
                    } else if t == "value" {
                        reading_name = false;
                        reading_val = true;
                    } else if reading_name {
                        if !name.is_empty() {
                            name.push(' ');
                        }
                        name.push_str(t);
                    } else if reading_val {
                        if !val.is_empty() {
                            val.push(' ');
                        }
                        val.push_str(t);
                    }
                }
                match name.to_lowercase().as_str() {
                    "hash" => {
                        if let Ok(mb) = val.parse::<usize>() {
                            pool.set_hash(mb);
                        }
                    }
                    "threads" => {
                        if let Ok(th) = val.parse::<usize>() {
                            pool.set_threads(th);
                        }
                    }
                    "uci_chess960" => {
                        chess960 = val.to_lowercase() == "true";
                    }
                    _ => {}
                }
            }
            "position" => {
                let first = tokens.next();
                let mut fen_str = String::new();
                let mut moves_token_seen = false;

                if first == Some("startpos") {
                    fen_str = START_FEN.to_string();
                } else if first == Some("fen") {
                    while let Some(t) = tokens.next() {
                        if t == "moves" {
                            moves_token_seen = true;
                            break;
                        }
                        if !fen_str.is_empty() {
                            fen_str.push(' ');
                        }
                        fen_str.push_str(t);
                    }
                }

                if let Err(e) = pos.set(&fen_str, chess960) {
                    eprintln!("Error setting position: {}", e);
                    continue;
                }

                if first == Some("startpos") {
                    if let Some(t) = tokens.next() {
                        if t == "moves" {
                            moves_token_seen = true;
                        }
                    }
                }

                if moves_token_seen {
                    for m_str in tokens {
                        if let Some(m) = parse_move(&mut pos, m_str) {
                            pos.ensure_capacity(MAX_PLY + 64);
                            let gives_check = pos.gives_check(m);
                            pos.do_move(m, gives_check, None);
                        }
                    }
                }
            }
            "go" => {
                let mut limits = LimitsType::default();
                limits.start_time = crate::timeman::now();
                let mut perft_depth: u32 = 0;
                while let Some(t) = tokens.next() {
                    match t {
                        "perft" => {
                            if let Some(d) = tokens.next().and_then(|s| s.parse().ok()) {
                                perft_depth = d;
                            }
                        }
                        "depth" => {
                            if let Some(d) = tokens.next().and_then(|s| s.parse().ok()) {
                                limits.depth = d;
                            }
                        }
                        "movetime" => {
                            if let Some(mt) = tokens.next().and_then(|s| s.parse().ok()) {
                                limits.movetime = mt;
                            }
                        }
                        "nodes" => {
                            if let Some(n) = tokens.next().and_then(|s| s.parse().ok()) {
                                limits.nodes = n;
                            }
                        }
                        "wtime" => {
                            if let Some(wt) = tokens.next().and_then(|s| s.parse().ok()) {
                                limits.time[Color::White as usize] = wt;
                            }
                        }
                        "btime" => {
                            if let Some(bt) = tokens.next().and_then(|s| s.parse().ok()) {
                                limits.time[Color::Black as usize] = bt;
                            }
                        }
                        "winc" => {
                            if let Some(wi) = tokens.next().and_then(|s| s.parse().ok()) {
                                limits.inc[Color::White as usize] = wi;
                            }
                        }
                        "binc" => {
                            if let Some(bi) = tokens.next().and_then(|s| s.parse().ok()) {
                                limits.inc[Color::Black as usize] = bi;
                            }
                        }
                        "movestogo" => {
                            if let Some(mtg) = tokens.next().and_then(|s| s.parse().ok()) {
                                limits.movestogo = mtg;
                            }
                        }
                        "infinite" => {
                            limits.infinite = true;
                        }
                        _ => {}
                    }
                }

                if perft_depth > 0 {
                    let nodes = perft_nodes::<true>(&mut pos, perft_depth);
                    println!("\nNodes searched: {}", nodes);
                    continue;
                }

                pool.start_thinking(&pos, limits);
                let best_move = pool.best_move();
                println!("bestmove {}", format_move(best_move, pos.is_chess960()));
            }
            "stop" => {
                pool.set_stop(true);
            }
            "bench" => {
                let tt_size = tokens.next().and_then(|s| s.parse().ok()).unwrap_or(16);
                let threads = tokens.next().and_then(|s| s.parse().ok()).unwrap_or(1);
                let depth = tokens.next().and_then(|s| s.parse().ok()).unwrap_or(13);
                run_benchmark(tt_size, threads, depth);
            }
            "eval" => {
                let worker = pool.main_worker_mut();
                let eval = unsafe {
                    crate::evaluate::evaluate(
                        crate::nnue::network::get_network(),
                        &pos,
                        &mut worker.accumulator_stack,
                        &mut worker.refresh_table,
                        0,
                    )
                };
                println!("Evaluation: {} cp", eval);
            }
            "d" => {
                println!("Fen: {}", pos.fen());
                println!("Key: {:016X}", pos.key());
            }
            "quit" => break,
            _ => {}
        }
    }
}
