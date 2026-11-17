use crate::bitboard::*;
use crate::history::*;
use crate::movegen::*;
use crate::position::Position;
use crate::types::*;

pub const MAX_MOVES: usize = 256;

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Stage {
    TT,
    InitFirstStage,
    GoodCapture,
    GoodQuiet,
    BadMoves,
    QSearch,
    Evasion,
    ProbCut,
}

#[inline(always)]
pub fn partial_insertion_sort(moves: &mut [ExtMove], limit: i32) {
    let len = moves.len();
    if len <= 1 {
        return;
    }

    let mut sorted_end = 0;
    unsafe {
        for p in 1..len {
            if moves.get_unchecked(p).value >= limit {
                let tmp = *moves.get_unchecked(p);
                sorted_end += 1;
                *moves.get_unchecked_mut(p) = *moves.get_unchecked(sorted_end);
                let mut q = sorted_end;
                while q != 0 && moves.get_unchecked(q - 1).value < tmp.value {
                    *moves.get_unchecked_mut(q) = *moves.get_unchecked(q - 1);
                    q -= 1;
                }
                *moves.get_unchecked_mut(q) = tmp;
            }
        }
    }
}

pub struct MovePicker {
    pos: *const Position,
    main_history: *const ButterflyHistory,
    low_ply_history: *const LowPlyHistory,
    capture_history: *const CapturePieceToHistory,
    continuation_history: [*const PieceToHistory; 6],
    shared_history: *const SharedHistories,
    tt_move: Move,
    cur: usize,
    end_cur: usize,
    end_bad_captures: usize,
    end_bad_moves: usize,
    stage: Stage,
    first_stage: Stage,
    threshold: i32,
    depth: Depth,
    ply: usize,
    skip_quiets: bool,
    moves: [std::mem::MaybeUninit<ExtMove>; MAX_MOVES],
}

impl MovePicker {
    // Main search and quiescence search constructor
    pub fn new(
        pos: &Position,
        tt_move: Move,
        depth: Depth,
        main_history: &ButterflyHistory,
        low_ply_history: &LowPlyHistory,
        capture_history: &CapturePieceToHistory,
        continuation_history: &[*const PieceToHistory; 6],
        shared_history: &SharedHistories,
        ply: usize,
    ) -> Self {
        let first_stage = if pos.checkers() != 0 {
            Stage::Evasion
        } else if depth > 0 {
            Stage::GoodCapture
        } else {
            Stage::QSearch
        };

        let stage = if tt_move.is_ok() && pos.legal(tt_move) {
            Stage::TT
        } else {
            Stage::InitFirstStage
        };

        Self {
            pos: pos as *const Position,
            main_history: main_history as *const ButterflyHistory,
            low_ply_history: low_ply_history as *const LowPlyHistory,
            capture_history: capture_history as *const CapturePieceToHistory,
            continuation_history: *continuation_history,
            shared_history: shared_history as *const SharedHistories,
            tt_move,
            cur: 0,
            end_cur: 0,
            end_bad_captures: 0,
            end_bad_moves: 0,
            stage,
            first_stage,
            threshold: 0,
            depth,
            ply,
            skip_quiets: false,
            moves: [const { std::mem::MaybeUninit::uninit() }; MAX_MOVES],
        }
    }

    // ProbCut constructor
    pub fn new_probcut(
        pos: &Position,
        tt_move: Move,
        threshold: i32,
        capture_history: &CapturePieceToHistory,
    ) -> Self {
        debug_assert!(pos.checkers() == 0);

        let first_stage = Stage::ProbCut;
        let stage = if tt_move.is_ok() && pos.capture_stage(tt_move) && pos.legal(tt_move) {
            Stage::TT
        } else {
            Stage::InitFirstStage
        };

        Self {
            pos: pos as *const Position,
            main_history: std::ptr::null(),
            low_ply_history: std::ptr::null(),
            capture_history: capture_history as *const CapturePieceToHistory,
            continuation_history: [std::ptr::null(); 6],
            shared_history: std::ptr::null(),
            tt_move,
            cur: 0,
            end_cur: 0,
            end_bad_captures: 0,
            end_bad_moves: 0,
            stage,
            first_stage,
            threshold,
            depth: 0,
            ply: 0,
            skip_quiets: false,
            moves: [const { std::mem::MaybeUninit::uninit() }; MAX_MOVES],
        }
    }

    #[inline(never)]
    fn generate_and_score(&mut self, gen_type: GenType) -> usize {
        let pos = unsafe { &*self.pos };
        let us = pos.side_to_move();

        let mut raw_moves = [const { std::mem::MaybeUninit::<Move>::uninit() }; MAX_MOVES];
        let count = generate_moves_uninit(pos, &mut raw_moves, 0, gen_type);

        let mut it = self.cur;

        match gen_type {
            GenType::Captures => {
                let capt_hist = unsafe { &*self.capture_history };
                unsafe {
                    for i in 0..count {
                        let m = raw_moves.get_unchecked(i).assume_init();
                        let to = m.to_sq();
                        let pc = pos.moved_piece(m);
                        let captured_piece = pos.piece_on(to);

                        let val = capt_hist[pc as usize][to.idx()][type_of(captured_piece) as usize].get() as i32
                            + 7 * PIECE_VALUE[captured_piece as usize];

                        self.moves.get_unchecked_mut(it).write(ExtMove { mov: m, value: val });
                        it += 1;
                    }
                }
            }

            GenType::Quiets => {
                let mut threat_by_lesser = [0u64; 7];
                threat_by_lesser[PieceType::Knight as usize] = pos.threats_by(PieceType::Pawn);
                threat_by_lesser[PieceType::Bishop as usize] = threat_by_lesser[PieceType::Knight as usize];
                threat_by_lesser[PieceType::Rook as usize] = pos.threats_by(PieceType::Knight)
                    | pos.threats_by(PieceType::Bishop)
                    | threat_by_lesser[PieceType::Knight as usize];
                threat_by_lesser[PieceType::Queen as usize] = pos.threats_by(PieceType::Rook)
                    | threat_by_lesser[PieceType::Rook as usize];

                let main_hist = unsafe { &(*self.main_history)[us as usize] };
                let pawn_hist = unsafe { (*self.shared_history).pawn_entry(pos) };
                let ch0 = self.continuation_history[0];
                let ch1 = self.continuation_history[1];
                let ch2 = self.continuation_history[2];
                let ch3 = self.continuation_history[3];
                let ch5 = self.continuation_history[5];
                let lph = if !self.low_ply_history.is_null() && self.ply < LOW_PLY_HISTORY_SIZE {
                    Some(unsafe { &(*self.low_ply_history)[self.ply] })
                } else {
                    None
                };

                unsafe {
                    for i in 0..count {
                        let m = raw_moves.get_unchecked(i).assume_init();
                        let from = m.from_sq();
                        let to = m.to_sq();
                        let pc = pos.moved_piece(m);
                        let pt = type_of(pc);

                        let to_idx = to.idx();
                        let mut value = 2 * main_hist[m.raw() as usize].get() as i32;
                        value += 2 * pawn_hist[pc as usize][to_idx].get() as i32;
                        if !ch0.is_null() { value += (*ch0)[pc as usize][to_idx].get() as i32; }
                        if !ch1.is_null() { value += (*ch1)[pc as usize][to_idx].get() as i32; }
                        if !ch2.is_null() { value += (*ch2)[pc as usize][to_idx].get() as i32; }
                        if !ch3.is_null() { value += (*ch3)[pc as usize][to_idx].get() as i32; }
                        if !ch5.is_null() { value += (*ch5)[pc as usize][to_idx].get() as i32; }

                        let tbl = *threat_by_lesser.get_unchecked(pt as usize);
                        let v = 20 * (((tbl & square_bb(from)) != 0) as i32
                            - ((tbl & square_bb(to)) != 0) as i32);
                        value += PIECE_VALUE[pt as usize] * v;

                        if let Some(lph) = lph {
                            value += 8 * (lph[m.raw() as usize].get() as i32) / (1 + self.ply as i32);
                        }

                        if (pos.check_squares(pt) & square_bb(to)) != 0 && pos.see_ge(m, -75) {
                            value += 16384;
                        }

                        self.moves.get_unchecked_mut(it).write(ExtMove { mov: m, value });
                        it += 1;
                    }
                }
            }

            GenType::Evasions => {
                let main_hist = unsafe { &(*self.main_history)[us as usize] };
                let ch0 = self.continuation_history[0];

                unsafe {
                    for i in 0..count {
                        let m = raw_moves.get_unchecked(i).assume_init();
                        let to = m.to_sq();
                        let pc = pos.moved_piece(m);
                        let captured_piece = pos.piece_on(to);

                        let value = if pos.capture_stage(m) {
                            PIECE_VALUE[captured_piece as usize] + (1 << 28)
                        } else {
                            let ch0_val = if !ch0.is_null() { (*ch0)[pc as usize][to.idx()].get() as i32 } else { 0 };
                            main_hist[m.raw() as usize].get() as i32 + ch0_val
                        };

                        self.moves.get_unchecked_mut(it).write(ExtMove { mov: m, value });
                        it += 1;
                    }
                }
            }

            GenType::All => unreachable!(),
        }

        it
    }

    #[inline(never)]
    pub fn next_move(&mut self) -> Move {
        const GOOD_QUIET_THRESHOLD: i32 = -14000;
        let pos = unsafe { &*self.pos };

        loop {
            match self.stage {
                Stage::TT => {
                    self.stage = Stage::InitFirstStage;
                    return self.tt_move;
                }

                Stage::InitFirstStage => {
                    self.cur = 0;
                    self.end_bad_moves = 0;

                    if self.first_stage == Stage::Evasion {
                        self.end_cur = self.generate_and_score(GenType::Evasions);
                    } else {
                        self.end_cur = self.generate_and_score(GenType::Captures);
                    }

                    let slice = unsafe {
                        std::slice::from_raw_parts_mut(
                            self.moves.as_mut_ptr().add(self.cur) as *mut ExtMove,
                            self.end_cur - self.cur,
                        )
                    };
                    partial_insertion_sort(slice, i32::MIN);
                    self.stage = self.first_stage;
                    continue;
                }

                Stage::GoodCapture => {
                    while self.cur < self.end_cur {
                        let m = unsafe { self.moves.get_unchecked(self.cur).assume_init() };
                        self.cur += 1;
                        if m.mov != self.tt_move {
                            if pos.see_ge(m.mov, -m.value / 18) {
                                return m.mov;
                            }
                            unsafe { self.moves.get_unchecked_mut(self.end_bad_moves).write(m); }
                            self.end_bad_moves += 1;
                        }
                    }

                    self.end_bad_captures = self.end_bad_moves;

                    if !self.skip_quiets {
                        self.cur = self.end_bad_moves;
                        self.end_cur = self.generate_and_score(GenType::Quiets);
                        let slice = unsafe {
                            std::slice::from_raw_parts_mut(
                                self.moves.as_mut_ptr().add(self.cur) as *mut ExtMove,
                                self.end_cur - self.cur,
                            )
                        };
                        partial_insertion_sort(slice, -3560 * self.depth);
                        self.stage = Stage::GoodQuiet;
                        continue;
                    }

                    self.cur = 0;
                    self.end_cur = self.end_bad_moves;
                    self.stage = Stage::BadMoves;
                    continue;
                }

                Stage::GoodQuiet => {
                    while self.cur < self.end_cur {
                        let m = unsafe { self.moves.get_unchecked(self.cur).assume_init() };
                        self.cur += 1;
                        if m.mov != self.tt_move {
                            if m.value > GOOD_QUIET_THRESHOLD {
                                return m.mov;
                            }
                            unsafe { self.moves.get_unchecked_mut(self.end_bad_moves).write(m); }
                            self.end_bad_moves += 1;
                        }
                    }

                    self.cur = 0;
                    self.end_cur = self.end_bad_moves;
                    self.stage = Stage::BadMoves;
                    continue;
                }

                Stage::BadMoves | Stage::QSearch | Stage::Evasion => {
                    while self.cur < self.end_cur {
                        let m = unsafe { self.moves.get_unchecked(self.cur).assume_init() };
                        self.cur += 1;
                        if m.mov != self.tt_move {
                            return m.mov;
                        }
                    }
                    return Move::none();
                }

                Stage::ProbCut => {
                    while self.cur < self.end_cur {
                        let m = unsafe { self.moves.get_unchecked(self.cur).assume_init() };
                        self.cur += 1;
                        if m.mov != self.tt_move && pos.see_ge(m.mov, self.threshold) {
                            return m.mov;
                        }
                    }
                    return Move::none();
                }
            }
        }
    }

    pub fn skip_quiet_moves(&mut self) {
        self.skip_quiets = true;
        if self.stage == Stage::BadMoves || self.stage == Stage::GoodQuiet {
            self.end_cur = self.end_bad_captures;
            self.end_bad_moves = self.end_bad_captures;
        }
    }
}
