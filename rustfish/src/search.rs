use std::sync::atomic::Ordering;
use std::sync::Arc;
use crate::memory::RelaxedAtomicU64;
use crate::thread::SharedControl;
use crate::types::*;
use crate::position::Position;
use crate::history::*;
use crate::movepick::*;
use crate::tt::TranspositionTable;
use crate::timeman::{LimitsType, TimeManagement};

pub const MAX_MOVES: usize = 256;
pub const NODES_LIMIT_OUTPUT: u64 = 10_000_000;

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum NodeType {
    NonPV,
    PV,
    Root,
}

pub static REDUCTIONS: [i32; MAX_MOVES] = [
    0,0,15,24,31,36,40,43,46,49,51,53,55,57,59,60,
    62,63,64,66,67,68,69,70,71,72,73,73,74,75,76,77,
    77,78,79,79,80,81,81,82,82,83,83,84,84,85,85,86,
    86,87,87,88,88,89,89,89,90,90,91,91,91,92,92,92,
    93,93,94,94,94,95,95,95,95,96,96,96,97,97,97,98,
    98,98,98,99,99,99,99,100,100,100,100,101,101,101,101,102,
    102,102,102,103,103,103,103,103,104,104,104,104,105,105,105,105,
    105,106,106,106,106,106,107,107,107,107,107,107,108,108,108,108,
    108,109,109,109,109,109,109,110,110,110,110,110,110,111,111,111,
    111,111,111,111,112,112,112,112,112,112,113,113,113,113,113,113,
    113,114,114,114,114,114,114,114,114,115,115,115,115,115,115,115,
    116,116,116,116,116,116,116,116,117,117,117,117,117,117,117,117,
    117,118,118,118,118,118,118,118,118,118,119,119,119,119,119,119,
    119,119,119,120,120,120,120,120,120,120,120,120,121,121,121,121,
    121,121,121,121,121,121,122,122,122,122,122,122,122,122,122,122,
    122,123,123,123,123,123,123,123,123,123,123,123,124,124,124,124,
];

#[inline(always)]
pub fn reduction(i: bool, d: Depth, mn: usize) -> i32 {
    let d_clamped = (d as usize).min(MAX_MOVES - 1);
    let mn_clamped = mn.min(MAX_MOVES - 1);
    let reduction_scale = unsafe {
        *REDUCTIONS.get_unchecked(d_clamped) * *REDUCTIONS.get_unchecked(mn_clamped)
    };
    reduction_scale + (!i as i32) * reduction_scale * 197 / 512 + 948
}

#[inline(always)]
pub fn lmr_divisor(depth: Depth) -> i32 {
    let d = depth.clamp(0, 16);
    3000 + 7 * (d - 8) * (d - 8)
}

#[derive(Clone, Debug)]
pub struct PVMoves {
    moves: [core::mem::MaybeUninit<Move>; MAX_PLY + 1],
    pub length: usize,
}

impl Default for PVMoves {
    #[inline(always)]
    fn default() -> Self {
        Self {
            moves: [const { core::mem::MaybeUninit::uninit() }; MAX_PLY + 1],
            length: 0,
        }
    }
}

impl PVMoves {
    #[inline(always)]
    pub fn clear(&mut self) {
        self.length = 0;
    }

    #[inline(always)]
    pub fn push_back(&mut self, m: Move) {
        debug_assert!(self.length < MAX_PLY + 1);
        unsafe { self.moves.get_unchecked_mut(self.length).write(m); }
        self.length += 1;
    }

    #[inline(always)]
    pub fn as_slice(&self) -> &[Move] {
        unsafe { core::slice::from_raw_parts(self.moves.as_ptr() as *const Move, self.length) }
    }

    #[inline(always)]
    pub fn update(&mut self, m: Move, child_pv: Option<&PVMoves>) {
        self.moves[0].write(m);
        if let Some(cpv) = child_pv {
            let len = cpv.length.min(MAX_PLY);
            unsafe {
                core::ptr::copy_nonoverlapping(
                    cpv.moves.as_ptr(),
                    self.moves.as_mut_ptr().add(1),
                    len,
                );
            }
            self.length = len + 1;
        } else {
            self.length = 1;
        }
    }
}

#[derive(Copy, Clone)]
pub struct Stack {
    pub pv: *mut PVMoves,
    pub continuation_history: *const PieceToHistory,
    pub continuation_correction_history: *mut [[StatsEntry<CORRECTION_HISTORY_LIMIT>; SQUARE_NB]; PIECE_NB],
    pub ply: usize,
    pub current_move: Move,
    pub excluded_move: Move,
    pub static_eval: Value,
    pub stat_score: i32,
    pub move_count: i32,
    pub in_check: bool,
    pub tt_pv: bool,
    pub tt_hit: bool,
    pub follow_pv: bool,
    pub cutoff_cnt: i32,
    pub reduction: i32,
    pub prior_nmp_fail_high: i32,
}

impl Default for Stack {
    fn default() -> Self {
        Self {
            pv: std::ptr::null_mut(),
            continuation_history: std::ptr::null(),
            continuation_correction_history: std::ptr::null_mut(),
            ply: 0,
            current_move: Move::none(),
            excluded_move: Move::none(),
            static_eval: VALUE_NONE,
            stat_score: 0,
            move_count: 0,
            in_check: false,
            tt_pv: false,
            tt_hit: false,
            follow_pv: false,
            cutoff_cnt: 0,
            reduction: 0,
            prior_nmp_fail_high: 0,
        }
    }
}

#[derive(Clone, Debug)]
pub struct RootMove {
    pub effort: u64,
    pub score: Value,
    pub previous_score: Value,
    pub average_score: Value,
    pub mean_squared_score: i64,
    pub uci_score: Value,
    pub inexact_lower: bool,
    pub inexact_upper: bool,
    pub previous_score_exact: bool,
    pub sel_depth: i32,
    pub tb_rank: i32,
    pub tb_score: Value,
    pub pv: Vec<Move>,
    pub previous_pv: Vec<Move>,
}

impl RootMove {
    pub fn new(m: Move) -> Self {
        let mut pv = Vec::with_capacity(32);
        pv.push(m);
        Self {
            effort: 0,
            score: -VALUE_INFINITE,
            previous_score: -VALUE_INFINITE,
            average_score: -VALUE_INFINITE,
            mean_squared_score: -(VALUE_INFINITE as i64) * (VALUE_INFINITE as i64),
            uci_score: -VALUE_INFINITE,
            inexact_lower: false,
            inexact_upper: false,
            previous_score_exact: false,
            sel_depth: 0,
            tb_rank: 0,
            tb_score: VALUE_NONE,
            pv,
            previous_pv: Vec::with_capacity(32),
        }
    }

    /// Resets this RootMove in-place from `src`, reusing existing Vec capacity (zero heap allocs).
    #[inline(always)]
    pub fn reset_from(&mut self, src: &RootMove) {
        self.effort = src.effort;
        self.score = src.score;
        self.previous_score = src.previous_score;
        self.average_score = src.average_score;
        self.mean_squared_score = src.mean_squared_score;
        self.uci_score = src.uci_score;
        self.inexact_lower = src.inexact_lower;
        self.inexact_upper = src.inexact_upper;
        self.previous_score_exact = src.previous_score_exact;
        self.sel_depth = src.sel_depth;
        self.tb_rank = src.tb_rank;
        self.tb_score = src.tb_score;
        self.pv.clone_from(&src.pv);
        self.previous_pv.clear();
    }

    #[inline(always)]
    pub fn is_inexact(&self) -> bool {
        self.inexact_lower || self.inexact_upper
    }

    #[inline(always)]
    pub fn is_exact_loss(&self) -> bool {
        self.score != -VALUE_INFINITE && is_loss(self.score) && !self.is_inexact()
    }

    #[inline(always)]
    pub fn unset_inexact(&mut self) {
        self.inexact_lower = false;
        self.inexact_upper = false;
    }
}

impl PartialEq for RootMove {
    fn eq(&self, other: &Self) -> bool {
        self.pv[0] == other.pv[0]
    }
}

impl Eq for RootMove {}

impl PartialOrd for RootMove {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for RootMove {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        if other.score != self.score {
            other.score.cmp(&self.score)
        } else {
            other.previous_score.cmp(&self.previous_score)
        }
    }
}

#[inline(always)]
pub unsafe fn correction_value(
    shared: &SharedHistories,
    pos: &Position,
    ss: *const Stack,
) -> i32 {
    let us = pos.side_to_move();
    let m = (*ss.sub(1)).current_move;
    let pcv = shared.pawn_correction_entry(pos)[us as usize].pawn.get() as i32;
    let micv = shared.minor_piece_correction_entry(pos)[us as usize].minor.get() as i32;
    let wnpcv = shared.nonpawn_correction_entry(Color::White, pos)[us as usize].non_pawn_white.get() as i32;
    let bnpcv = shared.nonpawn_correction_entry(Color::Black, pos)[us as usize].non_pawn_black.get() as i32;

    let cntcv = if m.is_ok() {
        let to = m.to_sq().idx();
        let pc = pos.piece_on(m.to_sq()) as usize;
        let h2 = (*(*ss.sub(2)).continuation_correction_history)[pc][to].get() as i32;
        let h4 = (*(*ss.sub(4)).continuation_correction_history)[pc][to].get() as i32;
        let h6 = (*(*ss.sub(6)).continuation_correction_history)[pc][to].get() as i32;
        7885 * (h2 + h4) + 6307 * h6
    } else {
        80695
    };

    13806 * pcv + 9512 * micv + 11615 * (wnpcv + bnpcv) + cntcv
}

#[inline(always)]
pub fn to_corrected_static_eval(v: Value, cv: i32) -> Value {
    (v + cv / 131072).clamp(VALUE_TB_LOSS_IN_MAX_PLY + 1, VALUE_TB_WIN_IN_MAX_PLY - 1)
}

#[inline(always)]
pub fn value_draw(nodes: u64) -> Value {
    VALUE_DRAW - 1 + ((nodes & 0x2) as Value)
}

#[inline(always)]
pub fn value_to_tt(v: Value, ply: usize) -> Value {
    if is_win(v) {
        v + (ply as Value)
    } else if is_loss(v) {
        v - (ply as Value)
    } else {
        v
    }
}

#[inline(always)]
pub fn value_from_tt(v: Value, ply: usize, r50c: i32) -> Value {
    if !is_valid(v) {
        return VALUE_NONE;
    }
    if is_win(v) {
        if is_mate(v) && VALUE_MATE - v > 100 - r50c {
            return VALUE_TB_WIN_IN_MAX_PLY - 1;
        }
        if VALUE_TB - v > 100 - r50c {
            return VALUE_TB_WIN_IN_MAX_PLY - 1;
        }
        return v - (ply as Value);
    }
    if is_loss(v) {
        if is_mated(v) && VALUE_MATE + v > 100 - r50c {
            return VALUE_TB_LOSS_IN_MAX_PLY + 1;
        }
        if VALUE_TB + v > 100 - r50c {
            return VALUE_TB_LOSS_IN_MAX_PLY + 1;
        }
        return v + (ply as Value);
    }
    v
}

pub unsafe fn update_continuation_histories(
    ss: *mut Stack,
    pc: Piece,
    to: Square,
    bonus: i32,
) {
    const CONTHIST_BONUSES: [(usize, i32); 5] = [(1, 520), (2, 390), (3, 145), (4, 251), (6, 209)];
    const CMHC_MULTIPLIERS: [i32; 6] = [94, 103, 110, 106, 119, 126];
    let mut positive_count = 0usize;

    let to_idx = to.idx();
    let in_check = (*ss).in_check;
    for (i, weight) in CONTHIST_BONUSES {
        if in_check && i > 2 {
            break;
        }
        let s = ss.sub(i);
        if (*s).current_move.is_ok() {
            let history_entry = &(*(*s).continuation_history)[pc as usize][to_idx];
            if history_entry.get() > 0 {
                positive_count += 1;
            }
            let multiplier = *CMHC_MULTIPLIERS.get_unchecked(positive_count.min(5));
            let bonus_val = bonus * weight * multiplier / 65536 + 73 * ((i < 2) as i32);
            history_entry.update(bonus_val);
        }
    }
}

pub unsafe fn update_quiet_histories(
    pos: &Position,
    ss: *mut Stack,
    main_history: &mut ButterflyHistory,
    low_ply_history: &mut LowPlyHistory,
    shared_history: &SharedHistories,
    m: Move,
    bonus: i32,
) {
    let us = pos.side_to_move();
    main_history[us as usize][m.raw() as usize].update(bonus);

    let ply = (*ss).ply;
    if ply < LOW_PLY_HISTORY_SIZE {
        low_ply_history.get_unchecked_mut(ply)[m.raw() as usize].update(bonus * 712 / 1024);
    }

    let pc = pos.moved_piece(m);
    let to = m.to_sq();
    update_continuation_histories(ss, pc, to, bonus * 750 / 1024);

    let pawn_bonus = bonus * (if bonus > -4 { 1104 } else { 459 }) / 1024;
    shared_history.pawn_entry(pos)[pc as usize][to.idx()].update(pawn_bonus);
}

#[inline(never)]
pub unsafe fn update_all_stats(
    pos: &Position,
    ss: *mut Stack,
    main_history: &mut ButterflyHistory,
    low_ply_history: &mut LowPlyHistory,
    capture_history: &mut CapturePieceToHistory,
    shared_history: &SharedHistories,
    best_move: Move,
    prev_sq: Square,
    quiets_searched: &[Move],
    captures_searched: &[Move],
    depth: Depth,
    tt_move: Move,
) {
    let moved_piece = pos.moved_piece(best_move);
    let ss_m1 = &*ss.sub(1);

    let mut bonus = (133 * depth - 81).min(1487)
        + 364 * ((best_move == tt_move) as i32)
        + ss_m1.stat_score / 28;
    let malus = (968 * depth - 235).min(2244);

    let count = (quiets_searched.len() + captures_searched.len()) as u64;
    bonus = bonus.wrapping_add(((bonus as i64 as u64).wrapping_mul(count) / 256) as u32 as i32);

    if !pos.capture_stage(best_move) {
        update_quiet_histories(
            pos,
            ss,
            main_history,
            low_ply_history,
            shared_history,
            best_move,
            bonus * 899 / 1024,
        );

        let mut actual_malus = malus * 1159 / 1024;
        for &m in quiets_searched.iter() {
            actual_malus = actual_malus * 921 / 1024;
            update_quiet_histories(
                pos,
                ss,
                main_history,
                low_ply_history,
                shared_history,
                m,
                -actual_malus,
            );
        }
    } else {
        let captured_pt = type_of(pos.piece_on(best_move.to_sq()));
        capture_history[moved_piece as usize][best_move.to_sq().idx()][captured_pt as usize]
            .update(bonus * 1427 / 1024);
    }

    if prev_sq != Square::SqNone
        && (ss_m1.move_count == 1 + (ss_m1.tt_hit as i32))
        && pos.captured_piece() == Piece::NoPiece
    {
        update_continuation_histories(
            ss.sub(1),
            pos.piece_on(prev_sq),
            prev_sq,
            -malus * 713 / 1024,
        );
    }

    for &m in captures_searched {
        let pc = pos.moved_piece(m);
        let captured_pt = type_of(pos.piece_on(m.to_sq()));
        capture_history[pc as usize][m.to_sq().idx()][captured_pt as usize]
            .update(-malus * 1489 / 1024);
    }
}

#[inline(never)]
pub unsafe fn update_correction_history(
    pos: &Position,
    ss: *mut Stack,
    shared: &SharedHistories,
    bonus: i32,
) {
    let m = (*ss.sub(1)).current_move;
    let us = pos.side_to_move();

    const NON_PAWN_WEIGHT: i32 = 186;

    shared.pawn_correction_entry(pos)[us as usize].pawn.update(bonus);
    shared.minor_piece_correction_entry(pos)[us as usize].minor.update(bonus * 150 / 128);
    shared.nonpawn_correction_entry(Color::White, pos)[us as usize].non_pawn_white.update(bonus * NON_PAWN_WEIGHT / 128);
    shared.nonpawn_correction_entry(Color::Black, pos)[us as usize].non_pawn_black.update(bonus * NON_PAWN_WEIGHT / 128);

    if m.is_ok() {
        let to = m.to_sq().idx();
        let pc = pos.piece_on(m.to_sq()) as usize;
        (*(*ss.sub(2)).continuation_correction_history)[pc][to].update(bonus * 130 / 128);
        (*(*ss.sub(4)).continuation_correction_history)[pc][to].update(bonus * 70 / 128);
        (*(*ss.sub(6)).continuation_correction_history)[pc][to].update(bonus * 35 / 128);
    }
}

#[inline(always)]
pub unsafe fn is_shuffling(m: Move, ss: *const Stack, pos: &Position) -> bool {
    if pos.capture_stage(m) || pos.rule50_count() < 10 {
        return false;
    }
    if pos.state().plies_from_null < 6 || (*ss).ply < 20 {
        return false;
    }
    m.from_sq() == (*ss.sub(2)).current_move.to_sq()
        && (*ss.sub(2)).current_move.from_sq() == (*ss.sub(4)).current_move.to_sq()
}

#[repr(C, align(64))]
pub struct Worker {
    pub main_history: ButterflyHistory,
    pub low_ply_history: LowPlyHistory,
    pub capture_history: CapturePieceToHistory,
    pub continuation_correction_history: ContinuationCorrectionHistory,
    pub tt_move_history: TTMoveHistory,

    pub accumulator_stack: crate::nnue::accumulator::AccumulatorStack,
    pub refresh_table: crate::nnue::accumulator::AccumulatorCaches,
    pub network: &'static crate::nnue::Network,

    pub nodes: RelaxedAtomicU64,
    pub tb_hits: RelaxedAtomicU64,
    pub best_move_changes: RelaxedAtomicU64,
    pub sel_depth: i32,
    pub nmp_min_ply: usize,

    pub optimism: [Value; COLOR_NB],
    pub root_pos: Position,
    pub root_moves: Vec<RootMove>,
    pub root_depth: Depth,
    pub seek_mate: bool,

    pub last_iteration_idx_pv: Vec<Move>,
    pub thread_idx: usize,
    pub limits: LimitsType,
    pub shared_ctrl: Arc<SharedControl>,
    pub tm: TimeManagement,
    pub original_time_adjust: f64,
    pub best_previous_score: Value,
    pub best_previous_average_score: Value,
    pub previous_time_reduction: f64,
    pub pv_idx: usize,
    pub iter_value: [Value; 4],
    pub calls_cnt: i32,
}

impl Worker {
    pub fn new(
        thread_idx: usize,
        shared_ctrl: Arc<SharedControl>,
    ) -> Self {
        let network = crate::nnue::network::get_network();
        Self {
            main_history: make_butterfly_history(),
            low_ply_history: make_low_ply_history(),
            capture_history: unsafe { std::mem::zeroed() },
            continuation_correction_history: make_continuation_correction_history(),
            tt_move_history: StatsEntry::new(0),
            accumulator_stack: crate::nnue::accumulator::AccumulatorStack::new(),
            refresh_table: crate::nnue::accumulator::AccumulatorCaches::new(&network.feature_transformer.biases),
            network,
            nodes: RelaxedAtomicU64::new(0),
            tb_hits: RelaxedAtomicU64::new(0),
            best_move_changes: RelaxedAtomicU64::new(0),
            sel_depth: 0,
            nmp_min_ply: 0,
            optimism: [0; COLOR_NB],
            root_pos: Position::new(),
            root_moves: Vec::with_capacity(64),
            root_depth: 0,
            seek_mate: false,
            last_iteration_idx_pv: Vec::with_capacity(MAX_PLY),
            thread_idx,
            limits: LimitsType::default(),
            shared_ctrl,
            tm: TimeManagement::new(),
            original_time_adjust: -1.0,
            best_previous_score: VALUE_INFINITE,
            best_previous_average_score: VALUE_INFINITE,
            previous_time_reduction: 0.85,
            pv_idx: 0,
            iter_value: [0; 4],
            calls_cnt: 0,
        }
    }

    #[inline(always)]
    pub fn is_mainthread(&self) -> bool {
        self.thread_idx == 0
    }

    pub fn setup_search_with_moves(&mut self, pos: &Position, limits: &LimitsType, root_moves: &[RootMove]) {
        self.accumulator_stack.reset();
        self.limits = limits.clone();
        if self.is_mainthread() {
            let ponder = self.limits.ponder_mode;
            self.tm.init(
                &mut self.limits,
                pos.side_to_move(),
                pos.game_ply() as usize,
                0,
                10,
                ponder,
                &mut self.original_time_adjust,
            );
        }
        self.nodes.store(0);
        self.tb_hits.store(0);
        self.best_move_changes.store(0);
        self.calls_cnt = 0;
        self.nmp_min_ply = 0;
        self.root_depth = 0;
        self.root_pos.copy_from(pos);
        let n = root_moves.len();
        let existing = self.root_moves.len();
        let common = existing.min(n);
        for i in 0..common {
            self.root_moves[i].reset_from(&root_moves[i]);
        }
        if n < existing {
            self.root_moves.truncate(n);
        } else {
            for rm in &root_moves[common..n] {
                self.root_moves.push(rm.clone());
            }
        }
    }

    pub fn clear_local(&mut self) {
        for c in 0..COLOR_NB {
            for i in 0..UINT_16_HISTORY_SIZE {
                self.main_history[c][i].set(-5);
            }
        }
        for pc in 0..PIECE_NB {
            for sq in 0..SQUARE_NB {
                for pt in 0..PIECE_TYPE_NB {
                    self.capture_history[pc][sq][pt].set(-742);
                }
            }
        }
        self.tt_move_history.set(0);
        self.original_time_adjust = -1.0;
        self.best_previous_score = VALUE_INFINITE;
        self.best_previous_average_score = VALUE_INFINITE;
        self.previous_time_reduction = 0.85;
        self.tm.clear();

        for p1 in 0..PIECE_NB {
            for s1 in 0..SQUARE_NB {
                for p2 in 0..PIECE_NB {
                    for s2 in 0..SQUARE_NB {
                        self.continuation_correction_history[p1][s1][p2][s2].set(5);
                    }
                }
            }
        }
        self.refresh_table.clear(&self.network.feature_transformer.biases);
    }

    #[inline(never)]
    pub fn evaluate(&mut self, pos: &Position) -> Value {
        unsafe {
            let opt = self.optimism[pos.side_to_move() as usize];
            let nnue = self.network.evaluate(pos, &mut self.accumulator_stack, &mut self.refresh_table);
            crate::evaluate::scale_evaluation(nnue, opt, pos)
        }
    }

    #[inline(always)]
    fn check_time(&mut self) {
        self.calls_cnt -= 1;
        if self.calls_cnt > 0 {
            return;
        }
        self.check_time_slow();
    }

    #[cold]
    #[inline(never)]
    fn check_time_slow(&mut self) {
        self.calls_cnt = if self.limits.nodes != 0 {
            512.min((self.limits.nodes / 1024) as i32)
        } else {
            512
        };

        let elapsed = self.tm.elapsed(|| self.shared_ctrl.nodes_searched());

        if (self.limits.use_time_management() && elapsed > self.tm.maximum())
            || (self.limits.movetime != 0 && elapsed >= self.limits.movetime)
            || (self.limits.nodes != 0 && self.shared_ctrl.nodes_searched() >= self.limits.nodes)
        {
            self.shared_ctrl.stop.store(true, Ordering::Relaxed);
        }
    }

    #[inline(always)]
    unsafe fn do_move(
        &mut self,
        pos: &mut Position,
        m: Move,
        gives_check: bool,
        capture: bool,
        ss: *mut Stack,
        tt: &TranspositionTable,
        shared_history: &SharedHistories,
    ) {
        tt.prefetch(pos.prefetch_key(m));
        #[cfg(target_arch = "x86_64")]
        {
            let pc = pos.moved_piece(m) as usize;
            let to = m.to_sq().idx();
            let p1 = &(*(*ss.sub(1)).continuation_correction_history)[pc][to] as *const _ as *const i8;
            let p3 = &(*(*ss.sub(3)).continuation_correction_history)[pc][to] as *const _ as *const i8;
            let p5 = &(*(*ss.sub(5)).continuation_correction_history)[pc][to] as *const _ as *const i8;
            core::arch::x86_64::_mm_prefetch(p1, core::arch::x86_64::_MM_HINT_T0);
            core::arch::x86_64::_mm_prefetch(p3, core::arch::x86_64::_MM_HINT_T0);
            core::arch::x86_64::_mm_prefetch(p5, core::arch::x86_64::_MM_HINT_T0);
        }
        self.nodes.inc();
        let dirties = self.accumulator_stack.push(pos);
        pos.do_move_search(m, gives_check, &mut dirties.dirties, shared_history);

        let dp = dirties.dirties.dirty_piece;
        let ss_cur = &mut *ss;
        ss_cur.current_move = m;
        let in_check = ss_cur.in_check as usize;
        let cap = capture as usize;
        let dp_pc = dp.pc as usize;
        let to = m.to_sq().idx();

        ss_cur.continuation_history =
            &shared_history.continuation_history[in_check][cap][dp_pc][to];
        ss_cur.continuation_correction_history =
            &mut self.continuation_correction_history[dp_pc][to];
    }

    #[inline(always)]
    fn undo_move(&mut self, pos: &mut Position, m: Move) {
        pos.undo_move(m);
        self.accumulator_stack.pop();
    }

    #[inline(always)]
    unsafe fn do_null_move(
        &mut self,
        pos: &mut Position,
        ss: *mut Stack,
        tt: &TranspositionTable,
        shared_history: &SharedHistories,
    ) {
        pos.do_null_move();
        tt.prefetch(pos.key());
        let ss_cur = &mut *ss;
        ss_cur.current_move = Move::null();
        ss_cur.continuation_history =
            &shared_history.continuation_history[0][0][Piece::NoPiece as usize][0];
        ss_cur.continuation_correction_history =
            &mut self.continuation_correction_history[Piece::NoPiece as usize][0];
    }

    #[inline(always)]
    fn undo_null_move(&mut self, pos: &mut Position) {
        pos.undo_null_move();
    }

    // Main search function
    pub unsafe fn search<const PV_NODE: bool, const ROOT_NODE: bool>(
        &mut self,
        pos: &mut Position,
        ss: *mut Stack,
        mut alpha: Value,
        mut beta: Value,
        mut depth: Depth,
        cut_node: bool,
        tt: &TranspositionTable,
        shared_history: &SharedHistories,
    ) -> Value {
        let all_node = !(PV_NODE || cut_node);

        if depth <= 0 {
            return self.qsearch::<PV_NODE>(pos, ss, alpha, beta, tt, shared_history);
        }

        depth = depth.min(MAX_PLY as i32 - 1);
        core::hint::assert_unchecked(depth >= 1 && depth < MAX_PLY as i32);

        let ply = (*ss).ply;

        if !ROOT_NODE && alpha < VALUE_DRAW && pos.upcoming_repetition(ply) {
            alpha = value_draw(self.nodes.load());
            if alpha >= beta {
                return alpha;
            }
        }

        let mut pv = PVMoves::default();
        let mut best_value = -VALUE_INFINITE;
        let max_value = VALUE_INFINITE;
        let mut best_move = Move::none();

        let in_check = pos.checkers() != 0;
        (*ss).in_check = in_check;
        let prior_capture = pos.captured_piece() != Piece::NoPiece;
        let us = pos.side_to_move();
        (*ss).move_count = 0;

        let ss_m1 = ss.sub(1);
        let ss_m2 = ss.sub(2);
        let ss_p1 = ss.add(1);
        let ss_p2 = ss.add(2);

        (*ss).follow_pv = ROOT_NODE
            || ((*ss_m1).follow_pv
                && (ply - 1 < self.last_iteration_idx_pv.len()
                    && (*ss_m1).current_move == *self.last_iteration_idx_pv.get_unchecked(ply - 1)));

        if self.is_mainthread() {
            self.check_time();
        }

        if PV_NODE && self.sel_depth < (ply + 1) as i32 {
            self.sel_depth = (ply + 1) as i32;
        }

        if !ROOT_NODE {
            if self.shared_ctrl.stop.load(Ordering::Relaxed) || pos.is_draw(ply) || ply >= MAX_PLY {
                return if ply >= MAX_PLY && !in_check {
                    self.evaluate(pos)
                } else {
                    value_draw(self.nodes.load())
                };
            }

            alpha = alpha.max(mated_in(ply));
            beta = beta.min(mate_in(ply + 1));
            if alpha >= beta {
                return alpha;
            }
        }

        let prev_sq = if (*ss_m1).current_move.is_ok() {
            (*ss_m1).current_move.to_sq()
        } else {
            Square::SqNone
        };

        let prior_reduction = (*ss_m1).reduction;
        (*ss_m1).reduction = 0;
        (*ss).stat_score = 0;
        (*ss_p2).cutoff_cnt = 0;
        (*ss_p1).prior_nmp_fail_high = 0;

        let correction_val = correction_value(shared_history, pos, ss);

        let excluded_move = (*ss).excluded_move;
        let pos_key = pos.key();
        let (tt_hit, mut tt_data, tt_writer) = tt.probe(pos_key);

        (*ss).tt_hit = tt_hit;
        tt_data.move_ = if ROOT_NODE {
            self.root_moves[self.pv_idx].pv[0]
        } else if tt_hit {
            tt_data.move_
        } else {
            Move::none()
        };
        tt_data.value = if tt_hit {
            value_from_tt(tt_data.value, ply, pos.rule50_count() as i32)
        } else {
            VALUE_NONE
        };
        (*ss).tt_pv = if excluded_move.is_ok() {
            (*ss).tt_pv
        } else {
            PV_NODE || (tt_hit && tt_data.is_pv)
        };
        let tt_capture = tt_data.move_.is_ok() && pos.capture_stage(tt_data.move_);

        let mut unadjusted_static_eval = VALUE_NONE;
        let mut eval: Value;

        if in_check {
            (*ss).static_eval = (*ss_m2).static_eval;
            eval = (*ss).static_eval;
        } else if excluded_move.is_ok() {
            unadjusted_static_eval = (*ss).static_eval;
            eval = (*ss).static_eval;
        } else if tt_hit {
            unadjusted_static_eval = tt_data.eval;
            if !is_valid(unadjusted_static_eval) {
                unadjusted_static_eval = self.evaluate(pos);
            }
            (*ss).static_eval = to_corrected_static_eval(unadjusted_static_eval, correction_val);
            eval = (*ss).static_eval;

            if is_valid(tt_data.value)
                && ((tt_data.bound as u8) & (if tt_data.value > eval { Bound::Lower as u8 } else { Bound::Upper as u8 })) != 0
            {
                eval = tt_data.value;
            }
        } else {
            unadjusted_static_eval = self.evaluate(pos);
            (*ss).static_eval = to_corrected_static_eval(unadjusted_static_eval, correction_val);
            eval = (*ss).static_eval;

            tt_writer.write(
                pos_key,
                VALUE_NONE,
                (*ss).tt_pv,
                Bound::None,
                DEPTH_UNSEARCHED,
                Move::none(),
                unadjusted_static_eval,
                tt.generation(),
            );
        }

        let mut improving = (*ss).static_eval > (*ss_m2).static_eval;
        let opponent_worsening = (*ss).static_eval > -(*ss_m1).static_eval;

        if prior_reduction >= 3 && !opponent_worsening {
            depth += 1;
        }
        if prior_reduction >= 2 && depth >= 2 && (*ss).static_eval + (*ss_m1).static_eval > 166 {
            depth -= 1;
        }

        // Early TT Cutoff
        if !PV_NODE && !excluded_move.is_ok() && is_valid(tt_data.value)
            && tt_data.depth > depth - (if tt_data.value <= beta { 1 } else { 0 })
        {
            if ((tt_data.bound as u8) & (if tt_data.value >= beta { Bound::Lower as u8 } else { Bound::Upper as u8 })) != 0
                && (cut_node == (tt_data.value >= beta) || depth > 4)
            {
                if tt_data.move_.is_ok() && tt_data.value >= beta {
                    if !tt_capture {
                        update_quiet_histories(
                            pos,
                            ss,
                            &mut self.main_history,
                            &mut self.low_ply_history,
                            shared_history,
                            tt_data.move_,
                            131 * depth,
                        );
                    }
                    if prev_sq != Square::SqNone && (*ss_m1).move_count < 5 && !prior_capture {
                        update_continuation_histories(
                            ss_m1,
                            pos.piece_on(prev_sq),
                            prev_sq,
                            -2210,
                        );
                    }
                }

                if pos.rule50_count() < 96 {
                    if depth >= 7 && tt_data.move_.is_ok() && pos.legal(tt_data.move_) && !is_decisive(tt_data.value) {
                        pos.do_move(tt_data.move_, pos.gives_check(tt_data.move_), None);
                        let next_pos_key = pos.key();
                        let (_, tt_data_next, _) = tt.probe(next_pos_key);
                        pos.undo_move(tt_data.move_);

                        if !is_valid(tt_data_next.value) {
                            return tt_data.value;
                        }
                        if (tt_data.value >= beta) == (-tt_data_next.value >= beta) {
                            return tt_data.value;
                        }
                    } else {
                        return tt_data.value;
                    }
                }
            } else if depth > 5 {
                tt_writer.penalize(1);
            }
        }

        if !in_check {
            if (*ss_m1).current_move.is_ok() && !(*ss_m1).in_check && !prior_capture {
                let eval_diff = (-((*ss_m1).static_eval + (*ss).static_eval)).clamp(-189, 194) + 60;
                self.main_history[(!us) as usize][(*ss_m1).current_move.raw() as usize].update(eval_diff * 11);
                if !tt_hit && type_of(pos.piece_on(prev_sq)) != PieceType::Pawn
                    && (*ss_m1).current_move.move_type() != MoveType::Promotion
                {
                    shared_history.pawn_entry(pos)[pos.piece_on(prev_sq) as usize][prev_sq.idx()].update(eval_diff * 13);
                }
            }

            // Step 8: Razoring
            if all_node && eval < alpha - 342 * depth && !self.seek_mate {
                return self.qsearch::<false>(pos, ss, alpha, beta, tt, shared_history);
            }

            // Step 9: Futility pruning: child node
            if !(*ss).tt_pv
                && depth < (if self.seek_mate { 6 } else { 19 })
                && eval >= beta
                && (!tt_data.move_.is_ok() || tt_capture)
                && !is_loss(beta)
                && !is_win(eval)
            {
                let mut futility_mult = (45 + depth * 4).min(85);
                futility_mult -= 20 * (!tt_hit as i32);

                let futility_margin = futility_mult * depth
                    - (2789 * (improving as i32) + 335 * (opponent_worsening as i32)) * futility_mult / 1024
                    + correction_val.abs() / 198435;

                if eval - futility_margin >= beta {
                    return (661 * beta + 363 * eval) / 1024;
                }
            }

            // Step 10: Null move search
            if cut_node
                && (*ss).static_eval + 50 * (*ss).prior_nmp_fail_high >= beta - 13 * depth - 47 * (improving as i32) + 365
                && !excluded_move.is_ok()
                && pos.non_pawn_material_c(us) != 0
                && ply >= self.nmp_min_ply
                && beta >= -2000
            {
                let mut r = 7 + depth / 3 + 0.max(((*ss).static_eval - beta) / 256) - (self.thread_idx as i32 & 1);
                if self.seek_mate {
                    r -= r / 2;
                }

                self.do_null_move(pos, ss, tt, shared_history);

                let null_value = -self.search::<false, false>(
                    pos,
                    ss_p1,
                    -beta,
                    -beta + 1,
                    depth - r,
                    false,
                    tt,
                    shared_history,
                );

                self.undo_null_move(pos);

                if null_value >= beta && !is_win(null_value) {
                    if self.nmp_min_ply != 0 || depth < 16 {
                        (*ss).prior_nmp_fail_high += 1;
                        return null_value;
                    }

                    self.nmp_min_ply = ply + 3 * (depth - r) as usize / 4;
                    let v = self.search::<false, false>(
                        pos,
                        ss,
                        beta - 1,
                        beta,
                        depth - r,
                        false,
                        tt,
                        shared_history,
                    );
                    self.nmp_min_ply = 0;

                    if v >= beta {
                        (*ss).prior_nmp_fail_high += 1;
                        return null_value;
                    }
                }
            }

            improving |= (*ss).static_eval >= beta;

            // Step 11: IIR
            if !(*ss).follow_pv && !all_node && depth >= 6 && !tt_data.move_.is_ok() {
                depth -= 1;
            }

            // Step 12: ProbCut
            let prob_cut_beta = beta + 241 - 64 * (improving as i32);
            if depth >= 3 && !is_decisive(beta) && !(is_valid(tt_data.value) && tt_data.value < prob_cut_beta) {
                let mut mp = MovePicker::new_probcut(
                    pos,
                    tt_data.move_,
                    prob_cut_beta - (*ss).static_eval,
                    &self.capture_history,
                );
                let prob_cut_depth = depth - (if improving { 5 } else { 3 });

                loop {
                    let m = mp.next_move();
                    if !m.is_ok() {
                        break;
                    }
                    if m == excluded_move {
                        continue;
                    }

                    let gives_check = pos.gives_check(m);
                    self.do_move(pos, m, gives_check, true, ss, tt, shared_history);

                    let mut v = -self.qsearch::<false>(pos, ss_p1, -prob_cut_beta, -prob_cut_beta + 1, tt, shared_history);

                    if v >= prob_cut_beta && prob_cut_depth > 0 {
                        v = -self.search::<false, false>(
                            pos,
                            ss_p1,
                            -prob_cut_beta,
                            -prob_cut_beta + 1,
                            prob_cut_depth,
                            !cut_node,
                            tt,
                            shared_history,
                        );
                    }

                    self.undo_move(pos, m);

                    if v >= prob_cut_beta {
                        tt_writer.write(
                            pos_key,
                            value_to_tt(v, ply),
                            (*ss).tt_pv,
                            Bound::Lower,
                            prob_cut_depth + 1,
                            m,
                            unadjusted_static_eval,
                            tt.generation(),
                        );

                        if !is_decisive(v) {
                            return v - (prob_cut_beta - beta);
                        }
                    }
                }
            }
        }

        // Step 13: small ProbCut
        let prob_cut_beta = beta + 428;
        if ((tt_data.bound as u8) & (Bound::Lower as u8)) != 0
            && tt_data.depth >= depth - 4
            && tt_data.value >= prob_cut_beta
            && !is_decisive(beta)
            && is_valid(tt_data.value)
            && !is_decisive(tt_data.value)
            && !self.seek_mate
        {
            return prob_cut_beta;
        }

        let cont_hist: [*const PieceToHistory; 6] = [
            (*ss_m1).continuation_history,
            (*ss_m2).continuation_history,
            (*ss.sub(3)).continuation_history,
            (*ss.sub(4)).continuation_history,
            (*ss.sub(5)).continuation_history,
            (*ss.sub(6)).continuation_history,
        ];

        let mut mp = MovePicker::new(
            pos,
            tt_data.move_,
            depth,
            &self.main_history,
            &self.low_ply_history,
            &self.capture_history,
            &cont_hist,
            shared_history,
            ply,
        );

        let mut move_count = 0;
        let mut quiets_searched = [Move::none(); 32];
        let mut quiets_cnt = 0usize;
        let mut captures_searched = [Move::none(); 32];
        let mut captures_cnt = 0usize;

        loop {
            let m = mp.next_move();
            if !m.is_ok() {
                break;
            }

            if m == excluded_move {
                continue;
            }

            if ROOT_NODE && !self.root_moves[self.pv_idx..].iter().any(|rm| rm.pv[0] == m) {
                continue;
            }

            move_count += 1;
            (*ss).move_count = move_count;

            if PV_NODE {
                (*ss_p1).pv = std::ptr::null_mut();
            }

            let mut extension = 0;
            let capture = pos.capture_stage(m);
            let moved_piece = pos.moved_piece(m);
            let gives_check = pos.gives_check(m);

            let mut new_depth = depth - 1;
            let mut r = reduction(improving, depth, move_count as usize);

            if PV_NODE {
                r -= 512;
            }
            if (*ss).tt_pv {
                r += 929;
            }

            let to_sq = m.to_sq();
            let to_idx = to_sq.idx();

            // Step 15: Pruning at shallow depths
            if !ROOT_NODE && pos.non_pawn_material_c(us) != 0 && !is_loss(best_value) {
                if move_count >= (3 + depth * depth) / (2 - (improving as i32)) {
                    mp.skip_quiet_moves();
                }

                let mut lmr_depth = new_depth - r / 1024;

                if capture || gives_check {
                    let captured_piece = pos.piece_on(to_sq);
                    let capt_hist = self.capture_history[moved_piece as usize][to_idx][type_of(captured_piece) as usize].get() as i32;

                    if !gives_check && lmr_depth < 8 {
                        let futility_value = (*ss).static_eval + 234 + 247 * lmr_depth
                            + *PIECE_VALUE.get_unchecked(captured_piece as usize) + 134 * capt_hist / 1024;
                        if futility_value <= alpha {
                            continue;
                        }
                    }

                    let margin = 177 * depth + capt_hist * 34 / 1024;
                    if (alpha >= VALUE_DRAW || pos.non_pawn_material_c(us) != *PIECE_VALUE.get_unchecked(moved_piece as usize))
                        && !pos.see_ge(m, -margin)
                    {
                        continue;
                    }
                } else if !(*ss).follow_pv || !PV_NODE {
                    let mut history = (*cont_hist[0])[moved_piece as usize][to_idx].get() as i32
                        + (*cont_hist[1])[moved_piece as usize][to_idx].get() as i32
                        + shared_history.pawn_entry(pos)[moved_piece as usize][to_idx].get() as i32;

                    if history < -4136 * depth {
                        continue;
                    }

                    history += 69 * (self.main_history[us as usize][m.raw() as usize].get() as i32) / 32;
                    lmr_depth += history / lmr_divisor(depth);

                    let futility_value = (*ss).static_eval + 119 * lmr_depth
                        + 90 * (((*ss).static_eval > alpha) as i32) + 164;

                    if !in_check && lmr_depth < 12 && futility_value <= alpha {
                        if best_value <= futility_value && !is_decisive(best_value) && !is_win(futility_value) {
                            best_value = futility_value;
                        }
                        continue;
                    }

                    lmr_depth = lmr_depth.max(0);
                    if !pos.see_ge(m, -23 * lmr_depth * lmr_depth) {
                        continue;
                    }
                }
            }

            // Step 16: Singular Extensions
            if !ROOT_NODE && m == tt_data.move_ && !excluded_move.is_ok() && depth >= 6 + ((*ss).tt_pv as i32)
                && is_valid(tt_data.value) && !is_decisive(tt_data.value)
                && ((tt_data.bound as u8) & (Bound::Lower as u8)) != 0
                && tt_data.depth >= depth - 3 && !is_shuffling(m, ss, pos) && !self.seek_mate
            {
                let singular_beta = tt_data.value - (59 + 66 * (((*ss).tt_pv && !PV_NODE) as i32)) * depth / 63;
                let singular_depth = new_depth / 2;

                (*ss).excluded_move = m;
                let v = self.search::<false, false>(
                    pos,
                    ss,
                    singular_beta - 1,
                    singular_beta,
                    singular_depth,
                    cut_node,
                    tt,
                    shared_history,
                );
                (*ss).excluded_move = Move::none();

                if v < singular_beta {
                    let corr_val_adj = correction_val.abs() / 198368;
                    let double_margin = -2 + 204 * (PV_NODE as i32) - 152 * (!tt_capture as i32) - corr_val_adj
                        - 1175 * (self.tt_move_history.get() as i32) / 114178
                        - ((ply > self.root_depth as usize) as i32) * 38;
                    let triple_margin = 70 + 279 * (PV_NODE as i32) - 188 * (!tt_capture as i32)
                        + 81 * ((*ss).tt_pv as i32) - corr_val_adj
                        - ((ply > self.root_depth as usize) as i32) * 43;

                    extension = 1 + ((v < singular_beta - double_margin) as i32) + ((v < singular_beta - triple_margin) as i32);
                    depth += 1;
                } else if v >= beta && !is_decisive(v) {
                    self.tt_move_history.update(-421 - 110 * depth);
                    if !in_check && v > (*ss).static_eval {
                        let bonus = ((v - (*ss).static_eval) * 664 / 1024)
                            .clamp(-CORRECTION_HISTORY_LIMIT / 4, CORRECTION_HISTORY_LIMIT / 4);
                        update_correction_history(pos, ss, shared_history, bonus);
                    }
                    return v;
                } else if tt_data.value >= beta || cut_node {
                    extension = -3;
                }
            }

            let node_count = if ROOT_NODE { self.nodes.load() } else { 0 };

            // Step 17: Make the move
            self.do_move(pos, m, gives_check, capture, ss, tt, shared_history);
            new_depth += extension;

            // Step 18: LMR
            if (*ss).tt_pv {
                r -= 3023 + (PV_NODE as i32) * 1004 + ((tt_data.value > alpha) as i32) * 885
                    + ((tt_data.depth >= depth) as i32) * (816 + (cut_node as i32) * 940);
            }
            r += 697;
            r -= move_count * 65;
            r -= correction_val.abs() / 26310;

            if cut_node {
                r += 4026 + 933 * (!tt_data.move_.is_ok() as i32);
            }
            if tt_capture {
                r += 1079;
            }
            if (*ss_p1).cutoff_cnt > 1 {
                r += 264 + 1095 * (((*ss_p1).cutoff_cnt > 2) as i32) + 1138 * (all_node as i32);
            } else if m == tt_data.move_ {
                r -= 2179;
            }

            if capture {
                let cap_pc = pos.captured_piece();
                (*ss).stat_score = 873 * (*PIECE_VALUE.get_unchecked(cap_pc as usize)) / 128
                    + self.capture_history[moved_piece as usize][to_idx][type_of(cap_pc) as usize].get() as i32;
            } else {
                (*ss).stat_score = (2252 * (self.main_history[us as usize][m.raw() as usize].get() as i32)
                    + 1126 * ((*cont_hist[0])[moved_piece as usize][to_idx].get() as i32)
                    + 1093 * ((*cont_hist[1])[moved_piece as usize][to_idx].get() as i32)) / 1024;
            }

            r -= (*ss).stat_score * 439 / 4096;

            if !capture && !is_decisive(alpha) {
                r += 3 * (alpha - eval).clamp(-64, 96);
            }
            if all_node {
                core::hint::assert_unchecked(depth >= 1);
                r += r * 276 / (256 * depth + 268);
            }
            if self.seek_mate && r > 0 {
                r += (if gives_check { -r } else { r }) / 4;
            }

            let mut value: Value;

            // Apply LMR
            if depth >= 2 && move_count > 1 {
                let d = 1.max(new_depth + (-r / 1024).min(if ply < 2 * self.root_depth as usize { 2 } else { 0 })) + (PV_NODE as i32);
                (*ss).reduction = new_depth - d;
                value = -self.search::<false, false>(pos, ss_p1, -(alpha + 1), -alpha, d, true, tt, shared_history);
                (*ss).reduction = 0;

                if value > alpha {
                    let do_deeper = d < new_depth && value > best_value + 53;
                    let do_shallower = value < best_value + 8;
                    new_depth += (do_deeper as i32) - (do_shallower as i32);

                    if new_depth > d {
                        value = -self.search::<false, false>(pos, ss_p1, -(alpha + 1), -alpha, new_depth, !cut_node, tt, shared_history);
                    }

                    update_continuation_histories(ss, moved_piece, to_sq, 1334);
                }
            } else if !PV_NODE || move_count > 1 {
                if !tt_data.move_.is_ok() {
                    r += 1127;
                }
                value = -self.search::<false, false>(
                    pos,
                    ss_p1,
                    -(alpha + 1),
                    -alpha,
                    new_depth - ((r > 5234) as i32) - ((r > 5487 && new_depth > 2) as i32),
                    !cut_node,
                    tt,
                    shared_history,
                );
            } else {
                value = alpha + 1; // trigger PV search
            }

            // Step 20: PV search
            if PV_NODE && (move_count == 1 || value > alpha) {
                (*ss_p1).pv = &mut pv;
                pv.clear();

                if m == tt_data.move_
                    && ((is_valid(tt_data.value) && is_decisive(tt_data.value) && tt_data.depth > 0)
                        || tt_data.depth > 1)
                {
                    new_depth = new_depth.max(1);
                }

                value = -self.search::<true, false>(pos, ss_p1, -beta, -alpha, new_depth, false, tt, shared_history);
            }

            // Step 21: Undo move
            self.undo_move(pos, m);

            if self.shared_ctrl.stop.load(Ordering::Relaxed) {
                return VALUE_ZERO;
            }

            if ROOT_NODE {
                let rm_idx = self.root_moves.iter().position(|rm| rm.pv[0] == m).unwrap();
                let n = self.nodes.load() - node_count;

                let rm = &mut self.root_moves[rm_idx];
                rm.effort += n;

                let e_prev = 1.max(rm.effort - n);
                const SCALE: u64 = 32;
                const CHI_NUM: u64 = 3;
                const CHI_DEN: u64 = 2;
                const MIN_WEIGHT: u64 = 12;
                const MAX_WEIGHT: u64 = 24;

                let w = ((SCALE * n * CHI_DEN) / (n * CHI_DEN + CHI_NUM * e_prev)).clamp(MIN_WEIGHT, MAX_WEIGHT);
                let w_mss = w.min(16);
                let v2 = (value as i64) * (value.abs() as i64);

                if rm.average_score == -VALUE_INFINITE {
                    rm.average_score = value;
                } else {
                    let num = (value as i64 * w as i64 + rm.average_score as i64 * (SCALE - w) as i64) as u64;
                    rm.average_score = (num / SCALE) as Value;
                }

                if rm.mean_squared_score == -(VALUE_INFINITE as i64) * (VALUE_INFINITE as i64) {
                    rm.mean_squared_score = (value as i64) * (value.abs() as i64);
                } else {
                    let num_mss = (v2 * w_mss as i64 + rm.mean_squared_score * (SCALE - w_mss) as i64) as u64;
                    rm.mean_squared_score = (num_mss / SCALE) as i32 as i64;
                }

                if move_count == 1 || value > alpha {
                    rm.score = value;
                    rm.uci_score = value;
                    rm.sel_depth = self.sel_depth;
                    rm.unset_inexact();

                    if value >= beta {
                        rm.inexact_lower = true;
                        rm.uci_score = beta;
                    } else if value <= alpha {
                        rm.inexact_upper = true;
                        rm.uci_score = alpha;
                    }

                    rm.pv.truncate(1);
                    if !(*ss_p1).pv.is_null() {
                        rm.pv.extend_from_slice(pv.as_slice());
                    }

                    if move_count > 1 && self.pv_idx == 0 {
                        self.best_move_changes.inc();
                    }
                } else {
                    rm.score = -VALUE_INFINITE;
                }
            }

            let inc = (value == best_value && ply + 2 >= self.root_depth as usize && (self.nodes.load() & 14) == 0 && !is_win(value.abs() + 1)) as i32;

            if value + inc > best_value {
                best_value = value;

                if value + inc > alpha {
                    best_move = m;

                    if PV_NODE && !ROOT_NODE {
                        if !(*ss).pv.is_null() {
                            let child_pv = if !(*ss_p1).pv.is_null() {
                                Some(&pv)
                            } else {
                                None
                            };
                            (*(*ss).pv).update(m, child_pv);
                        }
                    }

                    if value >= beta {
                        (*ss).cutoff_cnt += ((extension < 2) || PV_NODE) as i32;
                        break;
                    }

                    if depth > 3 && depth < 12 && !is_decisive(value) {
                        depth -= 3;
                    }

                    alpha = value;
                }
            }

            if m != best_move && move_count <= 32 {
                if capture {
                    *captures_searched.get_unchecked_mut(captures_cnt) = m;
                    captures_cnt += 1;
                } else {
                    *quiets_searched.get_unchecked_mut(quiets_cnt) = m;
                    quiets_cnt += 1;
                }
            }
        }

        if best_value >= beta && !is_decisive(best_value) && !is_decisive(alpha) {
            core::hint::assert_unchecked(depth >= 1);
            best_value = (best_value * depth + beta) / (depth + 1);
        }

        if move_count == 0 {
            best_value = if excluded_move.is_ok() {
                alpha
            } else if in_check {
                mated_in(ply)
            } else {
                VALUE_DRAW
            };
        } else if best_move.is_ok() {
            update_all_stats(
                pos,
                ss,
                &mut self.main_history,
                &mut self.low_ply_history,
                &mut self.capture_history,
                shared_history,
                best_move,
                prev_sq,
                quiets_searched.get_unchecked(..quiets_cnt),
                captures_searched.get_unchecked(..captures_cnt),
                depth,
                tt_data.move_,
            );
            if !PV_NODE {
                self.tt_move_history.update(if best_move == tt_data.move_ { 918 } else { -747 });
            }
        } else if !prior_capture && prev_sq != Square::SqNone {
            let mut bonus_scale = -241;
            bonus_scale -= (*ss_m1).stat_score / 98;
            bonus_scale += (59 * depth).min(420);
            bonus_scale += 186 * (((*ss_m1).move_count > 9) as i32);
            bonus_scale += 142 * ((!in_check && best_value <= (*ss).static_eval - 106) as i32);
            bonus_scale += 159 * ((!(*ss_m1).in_check && best_value <= -(*ss_m1).static_eval - 68) as i32);

            bonus_scale = bonus_scale.max(0);
            let scaled_bonus = (150 * depth - 85).min(1337) * bonus_scale;

            update_continuation_histories(ss_m1, pos.piece_on(prev_sq), prev_sq, scaled_bonus * 263 / 16384);
            self.main_history[(!us) as usize][(*ss_m1).current_move.raw() as usize].update(scaled_bonus * 215 / 32768);

            if type_of(pos.piece_on(prev_sq)) != PieceType::Pawn && (*ss_m1).current_move.move_type() != MoveType::Promotion {
                shared_history.pawn_entry(pos)[pos.piece_on(prev_sq) as usize][prev_sq.idx()].update(scaled_bonus * 324 / 8192);
            }
        } else if prev_sq != Square::SqNone {
            let captured_piece = pos.captured_piece();
            if captured_piece != Piece::NoPiece {
                self.capture_history[pos.piece_on(prev_sq) as usize][prev_sq.idx()][type_of(captured_piece) as usize].update(892);
            }
        }

        if PV_NODE {
            best_value = best_value.min(max_value);
        }

        if best_value <= alpha {
            (*ss).tt_pv = (*ss).tt_pv || (*ss_m1).tt_pv;
        }

        if !excluded_move.is_ok() && !(ROOT_NODE && self.pv_idx != 0) {
            let bound = if best_value >= beta {
                Bound::Lower
            } else if PV_NODE && best_move.is_ok() {
                Bound::Exact
            } else {
                Bound::Upper
            };

            tt_writer.write(
                pos_key,
                value_to_tt(best_value, ply),
                (*ss).tt_pv,
                bound,
                if move_count != 0 { depth } else { (MAX_PLY as i32 - 1).min(depth + 6) },
                best_move,
                unadjusted_static_eval,
                tt.generation(),
            );
        }

        if !in_check && !(best_move.is_ok() && pos.capture(best_move))
            && (best_value > (*ss).static_eval) == best_move.is_ok()
        {
            let bonus = (((best_value - (*ss).static_eval) * depth * (if best_move.is_ok() { 12 } else { 18 })) / 128)
                .clamp(-CORRECTION_HISTORY_LIMIT / 4, CORRECTION_HISTORY_LIMIT / 4);
            update_correction_history(pos, ss, shared_history, 1061 * bonus / 1024);
        }

        best_value
    }

    // Quiescence search function
    pub unsafe fn qsearch<const PV_NODE: bool>(
        &mut self,
        pos: &mut Position,
        ss: *mut Stack,
        mut alpha: Value,
        beta: Value,
        tt: &TranspositionTable,
        shared_history: &SharedHistories,
    ) -> Value {
        let ply = (*ss).ply;
        if alpha < VALUE_DRAW && pos.upcoming_repetition(ply) {
            alpha = value_draw(self.nodes.load());
            if alpha >= beta {
                return alpha;
            }
        }

        let ss_m1 = ss.sub(1);
        let ss_p1 = ss.add(1);

        let mut pv = PVMoves::default();
        if PV_NODE {
            (*ss_p1).pv = &mut pv;
            if !(*ss).pv.is_null() {
                (*(*ss).pv).clear();
            }
        }

        let mut best_move = Move::none();
        let in_check = pos.checkers() != 0;
        (*ss).in_check = in_check;
        let mut move_count = 0;

        if PV_NODE && self.sel_depth < (ply + 1) as i32 {
            self.sel_depth = (ply + 1) as i32;
        }

        if pos.is_draw(ply) || ply >= MAX_PLY {
            return if ply >= MAX_PLY && !in_check {
                self.evaluate(pos)
            } else {
                VALUE_DRAW
            };
        }

        let pos_key = pos.key();
        let (tt_hit, mut tt_data, tt_writer) = tt.probe(pos_key);

        (*ss).tt_hit = tt_hit;
        tt_data.move_ = if tt_hit { tt_data.move_ } else { Move::none() };
        tt_data.value = if tt_hit {
            value_from_tt(tt_data.value, ply, pos.rule50_count() as i32)
        } else {
            VALUE_NONE
        };
        let pv_hit = tt_hit && tt_data.is_pv;

        if !PV_NODE && tt_data.depth >= DEPTH_QS && is_valid(tt_data.value)
            && ((tt_data.bound as u8) & (if tt_data.value >= beta { Bound::Lower as u8 } else { Bound::Upper as u8 })) != 0
        {
            return tt_data.value;
        }

        let mut unadjusted_static_eval = VALUE_NONE;
        let mut best_value: Value;
        let mut futility_base = -VALUE_INFINITE;

        if in_check {
            best_value = -VALUE_INFINITE;
        } else {
            let correction_val = correction_value(shared_history, pos, ss);
            if tt_hit {
                unadjusted_static_eval = tt_data.eval;
                if !is_valid(unadjusted_static_eval) {
                    unadjusted_static_eval = self.evaluate(pos);
                }
                (*ss).static_eval = to_corrected_static_eval(unadjusted_static_eval, correction_val);
                best_value = (*ss).static_eval;

                if is_valid(tt_data.value) && !is_decisive(tt_data.value)
                    && ((tt_data.bound as u8) & (if tt_data.value > best_value { Bound::Lower as u8 } else { Bound::Upper as u8 })) != 0
                {
                    best_value = tt_data.value;
                }
            } else {
                unadjusted_static_eval = self.evaluate(pos);
                (*ss).static_eval = to_corrected_static_eval(unadjusted_static_eval, correction_val);
                best_value = (*ss).static_eval;
            }

            if best_value >= beta {
                best_value = (441 * best_value + 583 * beta) / 1024;
                if !tt_hit {
                    tt_writer.write(
                        pos_key,
                        VALUE_NONE,
                        false,
                        Bound::Lower,
                        DEPTH_UNSEARCHED,
                        Move::none(),
                        unadjusted_static_eval,
                        tt.generation(),
                    );
                }
                return best_value;
            }

            if best_value > alpha {
                alpha = best_value;
            }

            futility_base = (*ss).static_eval + 306;
        }

        let prev_sq = if (*ss_m1).current_move.is_ok() {
            (*ss_m1).current_move.to_sq()
        } else {
            Square::SqNone
        };

        let ch0 = (*ss_m1).continuation_history;
        let mut mp = MovePicker::new_qsearch(
            pos,
            tt_data.move_,
            &self.main_history,
            &self.capture_history,
            &ch0,
        );

        loop {
            let m = mp.next_move();
            if !m.is_ok() {
                break;
            }

            let gives_check = pos.gives_check(m);
            let capture = pos.capture_stage(m);
            move_count += 1;

            if !is_loss(best_value) {
                if !gives_check && m.to_sq() != prev_sq && !is_loss(futility_base) && m.move_type() != MoveType::Promotion {
                    if move_count > 2 {
                        continue;
                    }

                    let futility_value = futility_base + *PIECE_VALUE.get_unchecked(pos.piece_on(m.to_sq()) as usize);
                    if futility_value <= alpha {
                        best_value = best_value.max(futility_value);
                        continue;
                    }

                    if !pos.see_ge(m, alpha - futility_base) {
                        best_value = best_value.max(alpha.min(futility_base));
                        continue;
                    }
                }

                if !capture {
                    continue;
                }

                if !pos.see_ge(m, -74) {
                    continue;
                }
            }

            if PV_NODE {
                pv.clear();
            }
            self.do_move(pos, m, gives_check, capture, ss, tt, shared_history);
            let value = -self.qsearch::<PV_NODE>(pos, ss_p1, -beta, -alpha, tt, shared_history);
            self.undo_move(pos, m);

            if value > best_value {
                best_value = value;

                if value > alpha {
                    best_move = m;

                    if PV_NODE {
                        if !(*ss).pv.is_null() {
                            (*(*ss).pv).update(m, Some(&pv));
                        }
                    }

                    if value < beta {
                        alpha = value;
                    } else {
                        break;
                    }
                }
            }
        }

        if move_count == 0 {
            if in_check {
                return mated_in(ply);
            }

            let us = pos.side_to_move();
            if (crate::bitboard::pawn_single_push_bb(us, pos.pieces_c_pt(us, PieceType::Pawn)) & !pos.pieces()) == 0
                && pos.non_pawn_material_c(us) == 0
                && (type_of(pos.captured_piece()) as u8) >= (PieceType::Knight as u8)
                && !pos.can_move()
            {
                best_value = VALUE_DRAW;
            }
        }

        if !is_decisive(best_value) && best_value > beta {
            best_value = (462 * best_value + 562 * beta) / 1024;
        }

        tt_writer.write(
            pos_key,
            value_to_tt(best_value, ply),
            pv_hit,
            if best_value >= beta { Bound::Lower } else { Bound::Upper },
            DEPTH_QS,
            best_move,
            unadjusted_static_eval,
            tt.generation(),
        );

        best_value
    }

    // Iterative deepening search loop
    pub fn iterative_deepening(&mut self, tt: &TranspositionTable, shared_history: &SharedHistories) {
        if self.root_moves.is_empty() {
            return;
        }
        let mut pv = PVMoves::default();
        // Leading padding: 7 slots before ply 0 so ss.sub(1).sub(6) (i.e. ss.sub(7))
        // is strictly in-bounds and initialized at ply == 0.
        // Trailing padding: 4 slots after MAX_PLY so ss.add(2) at MAX_PLY is in-bounds.
        const SS_OFFSET: usize = 7;
        const STACK_LEN: usize = MAX_PLY + SS_OFFSET + 5;
        const _: () = assert!(SS_OFFSET >= 7);
        const _: () = assert!(SS_OFFSET + MAX_PLY + 3 < STACK_LEN);

        let mut stack = [Stack::default(); STACK_LEN];
        let ss_idx = SS_OFFSET;

        let default_cch: *mut [[StatsEntry<CORRECTION_HISTORY_LIMIT>; SQUARE_NB]; PIECE_NB] =
            &mut self.continuation_correction_history[Piece::NoPiece as usize][0] as *mut _;

        for i in 1..=SS_OFFSET {
            stack[ss_idx - i].continuation_history =
                &shared_history.continuation_history[0][0][Piece::NoPiece as usize][0];
            stack[ss_idx - i].continuation_correction_history = default_cch;
            stack[ss_idx - i].static_eval = VALUE_NONE;
        }

        for i in 0..=(MAX_PLY + 3) {
            stack[ss_idx + i].ply = i;
        }

        stack[ss_idx].pv = &mut pv;

        self.low_ply_history.iter_mut().for_each(|row| {
            for entry in row.iter_mut() {
                entry.set(102);
            }
        });

        for c in 0..COLOR_NB {
            for i in 0..UINT_16_HISTORY_SIZE {
                let v = self.main_history[c][i].get() as i32 * 729 / 1024;
                self.main_history[c][i].set(v as i16);
            }
        }

        if self.is_mainthread() {
            let init_v = if self.best_previous_score == VALUE_INFINITE {
                VALUE_ZERO
            } else {
                self.best_previous_score
            };
            self.iter_value = [init_v; 4];
        }

        self.root_depth = 0;
        let multi_pv = 1.min(self.root_moves.len());

        let mut search_again_counter = 0;
        let mut fail_high_recovery = 0;
        let mut time_reduction = 1.0_f64;
        let mut tot_best_move_changes = 0.0_f64;
        let mut iter_idx = 0usize;
        let mut last_best_move_depth: Depth = 0;
        let mut last_best_move_pv: Vec<Move> = Vec::with_capacity(64);
        let mut last_best_move_score: Value = -VALUE_INFINITE;
        let mut best_value: Value;

        let mut root_pos = std::mem::replace(&mut self.root_pos, Position::empty_placeholder());
        let ss_root = unsafe { stack.as_mut_ptr().add(ss_idx) };

        while self.root_depth + 1 < MAX_PLY as i32
            && !self.shared_ctrl.stop.load(Ordering::Relaxed)
            && !(self.limits.depth != 0 && self.is_mainthread() && self.root_depth >= self.limits.depth)
        {
            self.root_depth += 1;
            let seek_mate_threshold = 750 + 220000 / (self.root_depth * self.root_depth);

            for i in 0..self.root_moves.len() {
                let rm = &mut self.root_moves[i];
                rm.previous_score = rm.score;
                rm.previous_pv.clone_from(&rm.pv);
                rm.previous_score_exact = i < multi_pv;
            }

            if !self.shared_ctrl.increase_depth.load(Ordering::Relaxed) {
                search_again_counter += 1;
            }

            self.pv_idx = 0;
            self.last_iteration_idx_pv.clone_from(&self.root_moves[self.pv_idx].previous_pv);
            self.sel_depth = 0;

            let mut delta = 5 + (self.thread_idx % 8) as i32 + (self.root_moves[self.pv_idx].mean_squared_score.abs() / 10193) as i32;
            let avg = self.root_moves[self.pv_idx].average_score;
            let mut alpha = (avg - delta).max(-VALUE_INFINITE);
            let mut beta = (avg + delta).min(VALUE_INFINITE);

            self.seek_mate = self.root_moves[self.pv_idx].previous_score != -VALUE_INFINITE
                && self.root_moves[self.pv_idx].previous_score.abs() >= seek_mate_threshold;

            let us = root_pos.side_to_move();
            self.optimism[us as usize] = 456 * avg / (avg.abs() + 85);
            self.optimism[(!us) as usize] = -self.optimism[us as usize];

            let mut failed_high_cnt = 0;
            fail_high_recovery = 0.max(fail_high_recovery - 2);

            loop {
                let adjusted_depth = 1.max(self.root_depth - failed_high_cnt - fail_high_recovery - 3 * (search_again_counter + 1) / 4);

                best_value = unsafe {
                    self.search::<true, true>(
                        &mut root_pos,
                        ss_root,
                        alpha,
                        beta,
                        adjusted_depth,
                        false,
                        tt,
                        shared_history,
                    )
                };

                self.root_moves.sort();

                if self.shared_ctrl.stop.load(Ordering::Relaxed) {
                    break;
                }

                if best_value <= alpha {
                    beta = alpha;
                    alpha = (best_value - delta).max(-VALUE_INFINITE);
                    failed_high_cnt = 0;
                } else if best_value >= beta {
                    alpha = (beta - delta).max(alpha);
                    beta = (best_value + delta).min(VALUE_INFINITE);
                    failed_high_cnt += 1;
                } else {
                    break;
                }

                delta += 47 * delta / 128;
            }

            if failed_high_cnt > 0 {
                fail_high_recovery = (failed_high_cnt + 1) / 2 + 2;
            }

            let stopped = self.shared_ctrl.stop.load(Ordering::Relaxed);
            if !stopped {
                if self.is_mainthread() {
                    self.output_pv(&root_pos, self.root_depth, tt);
                }
                if last_best_move_pv.is_empty() || last_best_move_pv[0] != self.root_moves[0].pv[0] {
                    last_best_move_depth = self.root_depth;
                }
                last_best_move_pv.clone_from(&self.root_moves[0].pv);
                last_best_move_score = self.root_moves[0].score;
            } else {
                if self.root_moves[0].score == -VALUE_INFINITE && !last_best_move_pv.is_empty() {
                    if let Some(idx) = self.root_moves.iter().position(|rm| rm.pv[0] == last_best_move_pv[0]) {
                        let rm = self.root_moves.remove(idx);
                        self.root_moves.insert(0, rm);
                    }
                    self.root_moves[0].score = last_best_move_score;
                    self.root_moves[0].uci_score = last_best_move_score;
                    self.root_moves[0].pv.clone_from(&last_best_move_pv);
                }
                if self.is_mainthread() && !self.root_moves.is_empty() && self.root_moves[0].score != -VALUE_INFINITE {
                    self.output_pv(&root_pos, (self.root_depth - 1).max(1), tt);
                }
                break;
            }

            if self.is_mainthread() {
                if self.limits.use_time_management() && !self.shared_ctrl.stop.load(Ordering::Relaxed) {
                    let nodes_effort = self.root_moves[0].effort * 100000 / self.nodes.load().max(1);
                    let falling_eval = ((11.48
                        + 2.30 * (self.best_previous_average_score - best_value) as f64
                        + 1.1 * (self.iter_value[iter_idx] - best_value) as f64)
                        / 100.0)
                        .clamp(0.576, 1.728);

                    let interp = |x: f64, x0: f64, x1: f64, y0: f64, y1: f64| -> f64 {
                        y0 + (y1 - y0) * (x - x0) / (x1 - x0)
                    };

                    time_reduction = interp((self.root_depth - last_best_move_depth) as f64, 4.96, 18.79, 0.639, 1.712)
                        .clamp(0.629, 1.544);
                    let reduction = (1.468 + self.previous_time_reduction) / (2.284 * time_reduction);

                    tot_best_move_changes /= 2.0;
                    tot_best_move_changes += self.shared_ctrl.take_best_move_changes() as f64;

                    let best_move_instability = 1.077 + 2.229 * tot_best_move_changes / self.shared_ctrl.threads_count as f64;
                    let high_best_move_effort = interp(nodes_effort as f64, 75800.0, 104510.0, 0.969, 0.714)
                        .clamp(0.693, 0.838);

                    let mut total_time = self.tm.optimum() as f64 * falling_eval * reduction
                        * best_move_instability * high_best_move_effort;

                    if self.root_moves.len() == 1 {
                        total_time = total_time.min(500.0 * self.limits.npmsec.max(1) as f64);
                    }

                    let elapsed_time = self.tm.elapsed(|| self.shared_ctrl.nodes_searched()) as f64;
                    if elapsed_time > total_time.min(self.tm.maximum() as f64)
                        || self.root_moves[0].score >= mate_in(3)
                        || self.root_moves[0].score == mated_in(2)
                    {
                        self.shared_ctrl.stop.store(true, Ordering::Relaxed);
                    } else {
                        self.shared_ctrl.increase_depth.store(elapsed_time <= total_time * 0.50, Ordering::Relaxed);
                    }
                } else {
                    let elapsed = self.tm.elapsed(|| self.shared_ctrl.nodes_searched());
                    if self.limits.movetime != 0 && elapsed >= self.limits.movetime {
                        self.shared_ctrl.stop.store(true, Ordering::Relaxed);
                    } else if self.limits.nodes != 0 && self.shared_ctrl.nodes_searched() >= self.limits.nodes {
                        self.shared_ctrl.stop.store(true, Ordering::Relaxed);
                    }
                }

                self.iter_value[iter_idx] = best_value;
                iter_idx = (iter_idx + 1) & 3;
            }
        }

        self.root_pos = root_pos;

        if self.is_mainthread() {
            self.previous_time_reduction = time_reduction;
            if !self.root_moves.is_empty() && self.root_moves[0].score != -VALUE_INFINITE {
                self.best_previous_score = self.root_moves[0].score;
                self.best_previous_average_score = self.root_moves[0].average_score;
            }
        }
    }

    pub fn output_pv(&self, root_pos: &Position, depth: Depth, tt: &TranspositionTable) {
        self.output_pv_for(self, root_pos, depth, tt);
    }

    pub fn output_pv_for(&self, src_worker: &Worker, root_pos: &Position, depth: Depth, tt: &TranspositionTable) {
        let total_nodes = self.shared_ctrl.nodes_searched();
        let total_tb_hits = self.shared_ctrl.tb_hits();
        let rm = &src_worker.root_moves[0];
        let elapsed = 1.max(self.tm.elapsed_time());
        let nps = total_nodes * 1000 / elapsed as u64;

        let score_str = if is_mate(rm.score) {
            format!("mate {}", (VALUE_MATE - rm.score + 1) / 2)
        } else if is_mated(rm.score) {
            format!("mate -{}", (VALUE_MATE + rm.score) / 2)
        } else {
            let cp = crate::evaluate::to_cp(rm.score, root_pos);
            format!("cp {}", cp)
        };

        let mut pv_str = String::new();
        for (i, &m) in rm.pv.iter().enumerate() {
            if i > 0 {
                pv_str.push(' ');
            }
            pv_str.push_str(&crate::uci::format_move(m, root_pos.is_chess960()));
        }

        println!(
            "info depth {} seldepth {} multipv 1 score {} nodes {} nps {} hashfull {} tbhits {} time {} pv {}",
            depth,
            rm.sel_depth,
            score_str,
            total_nodes,
            nps,
            tt.hashfull(0),
            total_tb_hits,
            elapsed,
            pv_str
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_root_moves_depth10() {
        let mut pool = crate::thread::ThreadPool::new(1, 16);
        let mut pos = Position::new();
        pos.set(START_FEN, false).unwrap();
        let limits = LimitsType { depth: 10, ..LimitsType::default() };
        pool.start_thinking(&pos, limits);
        assert_eq!(pool.main_worker().nodes.load(), 4543);
    }

    #[test]
    fn test_root_moves_depth1() {
        let mut pool = crate::thread::ThreadPool::new(1, 16);
        let mut pos = Position::new();
        pos.set(START_FEN, false).unwrap();

        let m_nc3 = Move::make(Square::SqB1, Square::SqC3, MoveType::Normal, PieceType::Knight);
        let limits = LimitsType { depth: 1, ..LimitsType::default() };
        pool.start_thinking(&pos, limits);
        assert_eq!(pool.main_worker().root_moves[0].pv[0], m_nc3);
        assert_eq!(pool.main_worker().root_moves[0].score, 25);
        assert_eq!(pool.main_worker().nodes.load(), 20);
    }

    #[test]
    fn test_position_cur_state_pointer_safety() {
        fn move_by_value(p: Position) -> Position {
            p
        }

        let mut pos = Position::new();
        pos.set(START_FEN, false).unwrap();
        let start_key = pos.key();

        // Move by value, clone, and copy_from
        let mut moved = move_by_value(pos);
        assert_eq!(moved.key(), start_key);

        let e2e4 = crate::uci::parse_move(&mut moved, "e2e4").unwrap();
        let e7e5 = {
            moved.do_move(e2e4, false, None);
            crate::uci::parse_move(&mut moved, "e7e5").unwrap()
        };
        moved.do_move(e7e5, false, None);
        let after_2_key = moved.key();

        // Clone after moves + move by value
        let cloned = move_by_value(moved.clone());
        assert_eq!(cloned.key(), after_2_key);

        // copy_from into an empty_placeholder and into an existing Position
        let mut copy1 = Position::empty_placeholder();
        copy1.copy_from(&cloned);
        assert_eq!(copy1.key(), after_2_key);

        // Force state_stack reallocation via ensure_capacity and verify cur_state stays valid
        copy1.ensure_capacity(2048);
        assert_eq!(copy1.key(), after_2_key);
        copy1.do_null_move();
        assert_ne!(copy1.key(), after_2_key);
        copy1.undo_null_move();
        assert_eq!(copy1.key(), after_2_key);
        copy1.undo_move(e7e5);
        copy1.undo_move(e2e4);
        assert_eq!(copy1.key(), start_key);
    }

    #[test]
    fn test_dynamic_uci_resize_and_smp() {
        let mut pool = crate::thread::ThreadPool::new(1, 16);
        let mut pos = Position::new();
        pos.set(START_FEN, false).unwrap();

        // Resize Hash: 16 -> 512 -> 128 MB and Threads: 1 -> 16 -> 4 -> 8 -> 1
        let steps: &[(usize, usize, i32)] = &[
            (16, 1, 8),
            (512, 16, 8),
            (128, 4, 8),
            (128, 8, 8),
            (16, 1, 10),
        ];

        for &(hash_mb, threads, depth) in steps {
            pool.set_hash(hash_mb);
            pool.set_threads(threads);
            pool.clear();
            let limits = LimitsType { depth, ..LimitsType::default() };
            pool.start_thinking(&pos, limits);
            let bm = pool.best_move();
            assert!(bm.is_ok(), "best move must be valid for Hash={} Threads={}", hash_mb, threads);
        }

        // Verify exact single-thread parity still holds after all dynamic resizes
        assert_eq!(pool.main_worker().nodes.load(), 4543);
    }
}
