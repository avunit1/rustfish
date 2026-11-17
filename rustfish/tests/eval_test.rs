use rustfish::evaluate::{scale_evaluation, to_cp};
use rustfish::nnue::{AccumulatorCaches, AccumulatorStack, Network};
use rustfish::position::Position;
use rustfish::types::{Color, VALUE_ZERO};

struct EvalTestCase {
    fen: &'static str,
    expected_raw_nnue: i32,
    expected_cp: i32, // white side centipawns (from Final evaluation)
}

const TEST_CASES: [EvalTestCase; 20] = [
    // 0: startpos
    EvalTestCase {
        fen: "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
        expected_raw_nnue: 45,
        expected_cp: 20,
    },
    // 1: kiwipete
    EvalTestCase {
        fen: "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
        expected_raw_nnue: -625,
        expected_cp: -281,
    },
    // 2: endgame_pawns
    EvalTestCase {
        fen: "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1",
        expected_raw_nnue: 20,
        expected_cp: 6,
    },
    // 3: complex_mid
    EvalTestCase {
        fen: "rnbq1k1r/pp1Pbppp/2p5/8/2B5/8/PPP1NnPP/RNBQK2R w KQ - 1 8",
        expected_raw_nnue: 833,
        expected_cp: 358,
    },
    // 4: fine_70
    EvalTestCase {
        fen: "r4rk1/1pp1qppp/p1np1n2/2b1p1B1/2B1P1b1/P1NP1N2/1PP1QPPP/R4RK1 w - - 0 10",
        expected_raw_nnue: 64,
        expected_cp: 29,
    },
    // 5: e4_black
    EvalTestCase {
        fen: "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq e3 0 1",
        expected_raw_nnue: 1,
        expected_cp: 0,
    },
    // 6: petrov
    EvalTestCase {
        fen: "r1bqkb1r/pppp1ppp/2n5/4p3/4n3/5N2/PPPP1PPP/RNBQKB1R w KQkq - 0 5",
        expected_raw_nnue: -386,
        expected_cp: -171,
    },
    // 7: italian
    EvalTestCase {
        fen: "r1bq1rk1/ppp2ppp/2np1n2/2b1p3/2B1P3/2NP1N2/PPP2PPP/R1BQ1RK1 w - - 0 7",
        expected_raw_nnue: 26,
        expected_cp: 12,
    },
    // 8: french_slav
    EvalTestCase {
        fen: "r1bqk2r/pp2bppp/2n1pn2/2pp4/3P4/2PBPN2/PP1N1PPP/R1BQK2R w KQkq - 4 7",
        expected_raw_nnue: 18,
        expected_cp: 8,
    },
    // 9: ruy_lopez
    EvalTestCase {
        fen: "r2q1rk1/1pp1bppp/p1np1n2/4p3/2B1P1b1/2NP1N2/PPPB1PPP/R2Q1RK1 w - - 2 9",
        expected_raw_nnue: -4,
        expected_cp: -2,
    },
    // 10: qgd
    EvalTestCase {
        fen: "r1b2rk1/pp1n1ppp/2p1pn2/q2p4/2PP4/P1NBPN2/1PQ2PPP/R3K2R b KQ - 0 10",
        expected_raw_nnue: -108,
        expected_cp: 46,
    },
    // 11: rook_endgame
    EvalTestCase {
        fen: "2r3k1/p4p2/3p2p1/1p1P3p/2P1r3/1P4P1/P4PKP/2R2R2 w - - 0 25",
        expected_raw_nnue: 159,
        expected_cp: 57,
    },
    // 12: pawn_endgame
    EvalTestCase {
        fen: "8/5pk1/4p3/3pP1p1/2pP2P1/1pP5/1P4K1/8 w - - 0 40",
        expected_raw_nnue: -152,
        expected_cp: -47,
    },
    // 13: symm_pawn
    EvalTestCase {
        fen: "8/8/1p1k4/p1pP1p2/P1P2P2/8/8/6K1 w - - 0 50",
        expected_raw_nnue: 23,
        expected_cp: 7,
    },
    // 14: kp_endgame
    EvalTestCase {
        fen: "8/8/8/8/8/4k3/4P3/4K3 w - - 0 1",
        expected_raw_nnue: 0,
        expected_cp: 0,
    },
    // 15: k_and_p
    EvalTestCase {
        fen: "8/8/8/8/8/1k6/1P6/1K6 b - - 0 1",
        expected_raw_nnue: 2,
        expected_cp: -1,
    },
    // 16: nimzo
    EvalTestCase {
        fen: "rnb1k1nr/pp1p1ppp/4p3/8/1b1NP3/2N5/PPP2PPP/R1BQKB1R w KQkq - 1 6",
        expected_raw_nnue: 3264,
        expected_cp: 1367,
    },
    // 17: scotch
    EvalTestCase {
        fen: "r1bqk2r/ppppbppp/2n5/4P3/2B5/2P2N2/P4PPP/R1BQK2R w KQkq - 1 9",
        expected_raw_nnue: 2,
        expected_cp: 1,
    },
    // 18: closed_pos
    EvalTestCase {
        fen: "r1b1k2r/pppp1ppp/2n2n2/4p3/1b2P3/2NP1N2/PPPB1PPP/R2QKB1R w KQkq - 3 6",
        expected_raw_nnue: 3255,
        expected_cp: 1369,
    },
    // 19: open_game
    EvalTestCase {
        fen: "r1bqkb1r/pppp1ppp/5n2/4p3/4P3/2N2N2/PPPP1PPP/R1BQKB1R w KQkq - 2 4",
        expected_raw_nnue: 2431,
        expected_cp: 1066,
    },
];

#[test]
fn test_gate_2_nnue_static_eval_20_fens() {
    let network = Network::load_embedded();
    let mut caches = AccumulatorCaches::new(&network.feature_transformer.biases);

    for (i, tc) in TEST_CASES.iter().enumerate() {
        let mut pos = Position::new();
        pos.set(tc.fen, false).expect("Valid FEN");
        let mut accumulators = AccumulatorStack::new();

        let raw_nnue = unsafe { network.evaluate(&pos, &mut accumulators, &mut caches) };
        let scaled = scale_evaluation(raw_nnue, VALUE_ZERO, &pos);

        let white_scaled = if pos.side_to_move() == Color::White {
            scaled
        } else {
            -scaled
        };
        let cp_scaled = to_cp(white_scaled, &pos);

        println!(
            "FEN {:2}: raw_nnue={:5} (exp {:5}), cp_scaled={:5} (exp {:5})",
            i, raw_nnue, tc.expected_raw_nnue, cp_scaled, tc.expected_cp
        );

        assert_eq!(
            raw_nnue, tc.expected_raw_nnue,
            "FEN {}: raw NNUE mismatch: got {}, expected {}",
            i, raw_nnue, tc.expected_raw_nnue
        );

        assert_eq!(
            cp_scaled, tc.expected_cp,
            "FEN {}: centipawn eval mismatch: got {}, expected {}",
            i, cp_scaled, tc.expected_cp
        );
    }
}

#[test]
fn test_incremental_eval_b1c3() {
    let network = rustfish::nnue::network::get_network();
    let mut pos = Position::new();
    pos.set(rustfish::types::START_FEN, false).unwrap();

    let mut stack = AccumulatorStack::new();
    let mut caches = AccumulatorCaches::new(&network.feature_transformer.biases);

    // Initial evaluation
    let eval0 = unsafe { network.evaluate(&pos, &mut stack, &mut caches) };
    eprintln!("eval0 (startpos): {}", eval0);

    // Push and do move
    let dirties = stack.push(&pos);
    let m = rustfish::types::Move::make(
        rustfish::types::Square::SqB1,
        rustfish::types::Square::SqC3,
        rustfish::types::MoveType::Normal,
        rustfish::types::PieceType::Knight,
    );
    pos.do_move(m, false, Some(&mut dirties.dirties));

    // Incremental evaluation
    let eval_inc = unsafe { network.evaluate(&pos, &mut stack, &mut caches) };
    eprintln!("eval_inc (after b1c3): {}", eval_inc);

    // Fresh evaluation
    let mut fresh_stack = AccumulatorStack::new();
    let mut fresh_caches = AccumulatorCaches::new(&network.feature_transformer.biases);
    let eval_fresh = unsafe { network.evaluate(&pos, &mut fresh_stack, &mut fresh_caches) };
    eprintln!("eval_fresh (after b1c3): {}", eval_fresh);

    // Check accumulators
    let inc_acc = stack.latest();
    let fresh_acc = fresh_stack.latest();

    for c in [Color::White, Color::Black] {
        let c_idx = c as usize;
        let mut diffs = 0;
        for i in 0..1024 {
            if inc_acc.accumulation[c_idx][i] != fresh_acc.accumulation[c_idx][i] {
                if diffs < 10 {
                    eprintln!(
                        "Diff {:?} [{}]: inc={} fresh={}",
                        c, i, inc_acc.accumulation[c_idx][i], fresh_acc.accumulation[c_idx][i]
                    );
                }
                diffs += 1;
            }
        }
        eprintln!("{:?} total diffs: {}", c, diffs);
    }

    assert_eq!(eval_inc, eval_fresh);
}

#[test]
fn test_all_root_moves_incremental() {
    let network = rustfish::nnue::network::get_network();
    let mut pos = Position::new();
    pos.set(rustfish::types::START_FEN, false).unwrap();

    let mut stack = AccumulatorStack::new();
    let mut caches = AccumulatorCaches::new(&network.feature_transformer.biases);

    // Initial evaluation
    let eval0 = unsafe { network.evaluate(&pos, &mut stack, &mut caches) };
    assert_eq!(eval0, 45);

    let mut moves = [rustfish::types::Move::none(); rustfish::types::MAX_MOVES];
    let count = rustfish::movegen::generate_moves_all(&pos, &mut moves, 0);

    for m in &moves[..count] {
        let m = *m;
        let dirties = stack.push(&pos);
        pos.do_move(m, false, Some(&mut dirties.dirties));

        let eval_inc = unsafe { network.evaluate(&pos, &mut stack, &mut caches) };

        let mut fresh_stack = AccumulatorStack::new();
        let mut fresh_caches = AccumulatorCaches::new(&network.feature_transformer.biases);
        let eval_fresh = unsafe { network.evaluate(&pos, &mut fresh_stack, &mut fresh_caches) };

        let m_str = rustfish::uci::format_move(m, false);
        if eval_inc != eval_fresh {
            eprintln!("MISMATCH on {}: inc={}, fresh={}", m_str, eval_inc, eval_fresh);
            // Check accumulators
            let inc_acc = stack.latest();
            let fresh_acc = fresh_stack.latest();

            for c in [Color::White, Color::Black] {
                let c_idx = c as usize;
                let mut diffs = 0;
                for i in 0..1024 {
                    if inc_acc.accumulation[c_idx][i] != fresh_acc.accumulation[c_idx][i] {
                        if diffs < 5 {
                            eprintln!(
                                "  Diff {:?} [{}]: inc={} fresh={}",
                                c, i, inc_acc.accumulation[c_idx][i], fresh_acc.accumulation[c_idx][i]
                            );
                        }
                        diffs += 1;
                    }
                }
                eprintln!("  {:?} total diffs: {}", c, diffs);
            }
        } else {
            eprintln!("MATCH on {}: eval={}", m_str, eval_inc);
        }

        pos.undo_move(m);
        stack.pop();

        assert_eq!(eval_inc, eval_fresh, "Move {} produced eval mismatch!", m_str);
    }
}


