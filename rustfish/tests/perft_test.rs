use rustfish::perft::run_perft;
use rustfish::types::START_FEN;

#[test]
fn test_perft_startpos() {
    assert_eq!(run_perft(START_FEN, 1, false).unwrap(), 20);
    assert_eq!(run_perft(START_FEN, 2, false).unwrap(), 400);
    assert_eq!(run_perft(START_FEN, 3, false).unwrap(), 8902);
    assert_eq!(run_perft(START_FEN, 4, false).unwrap(), 197281);
    assert_eq!(run_perft(START_FEN, 5, false).unwrap(), 4865609);
}

#[test]
fn test_perft_kiwipete() {
    let fen = "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1";
    assert_eq!(run_perft(fen, 1, false).unwrap(), 48);
    assert_eq!(run_perft(fen, 2, false).unwrap(), 2039);
    assert_eq!(run_perft(fen, 3, false).unwrap(), 97862);
    assert_eq!(run_perft(fen, 4, false).unwrap(), 4085603);
    assert_eq!(run_perft(fen, 5, false).unwrap(), 193690690);
}

#[test]
fn test_perft_pos3() {
    let fen = "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1";
    assert_eq!(run_perft(fen, 1, false).unwrap(), 14);
    assert_eq!(run_perft(fen, 2, false).unwrap(), 191);
    assert_eq!(run_perft(fen, 3, false).unwrap(), 2812);
    assert_eq!(run_perft(fen, 4, false).unwrap(), 43238);
    assert_eq!(run_perft(fen, 5, false).unwrap(), 674624);
    assert_eq!(run_perft(fen, 6, false).unwrap(), 11030083);
}

#[test]
fn test_perft_pos4() {
    let fen = "r3k2r/Pppp1ppp/1b3nbN/nP6/BBP1P3/q4N2/Pp1P2PP/R2Q1RK1 w kq - 0 1";
    assert_eq!(run_perft(fen, 1, false).unwrap(), 6);
    assert_eq!(run_perft(fen, 2, false).unwrap(), 264);
    assert_eq!(run_perft(fen, 3, false).unwrap(), 9467);
    assert_eq!(run_perft(fen, 4, false).unwrap(), 422333);
    assert_eq!(run_perft(fen, 5, false).unwrap(), 15833292);
}

#[test]
fn test_perft_pos5() {
    let fen = "rnbq1k1r/pp1Pbppp/2p5/8/2B5/8/PPP1NnPP/RNBQK2R w KQ - 1 8";
    assert_eq!(run_perft(fen, 1, false).unwrap(), 44);
    assert_eq!(run_perft(fen, 2, false).unwrap(), 1486);
    assert_eq!(run_perft(fen, 3, false).unwrap(), 62379);
    assert_eq!(run_perft(fen, 4, false).unwrap(), 2103487);
    assert_eq!(run_perft(fen, 5, false).unwrap(), 89941194);
}

#[test]
fn test_perft_pos6() {
    let fen = "r4rk1/1pp1qppp/p1np1n2/2b1p1B1/2B1P1b1/P1NP1N2/1PP1QPPP/R4RK1 w - - 0 10";
    assert_eq!(run_perft(fen, 1, false).unwrap(), 46);
    assert_eq!(run_perft(fen, 2, false).unwrap(), 2079);
    assert_eq!(run_perft(fen, 3, false).unwrap(), 89890);
    assert_eq!(run_perft(fen, 4, false).unwrap(), 3894594);
    assert_eq!(run_perft(fen, 5, false).unwrap(), 164075551);
}
