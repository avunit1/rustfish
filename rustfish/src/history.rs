use std::sync::atomic::{AtomicI16, Ordering};
use crate::memory::{PageAlignedBox, PageAlignedSlice};
use crate::types::*;
use crate::position::Position;

pub const PAWN_HISTORY_BASE_SIZE: usize = 8192;
pub const UINT_16_HISTORY_SIZE: usize = 65536;
pub const CORRHIST_BASE_SIZE: usize = UINT_16_HISTORY_SIZE;
pub const CORRECTION_HISTORY_LIMIT: i32 = 1024;
pub const LOW_PLY_HISTORY_SIZE: usize = 5;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[repr(transparent)]
pub struct StatsEntry<const D: i32> {
    pub entry: i16,
}

impl<const D: i32> Default for StatsEntry<D> {
    #[inline(always)]
    fn default() -> Self {
        Self { entry: 0 }
    }
}

impl<const D: i32> StatsEntry<D> {
    #[inline(always)]
    pub const fn new(val: i16) -> Self {
        Self { entry: val }
    }

    #[inline(always)]
    pub fn get(&self) -> i16 {
        self.entry
    }

    #[inline(always)]
    pub fn set(&mut self, val: i16) {
        self.entry = val;
    }

    #[inline(always)]
    pub fn update(&mut self, bonus: i32) {
        let clamped_bonus = bonus.clamp(-D, D);
        let val = self.entry as i32;
        self.entry = (val + clamped_bonus - val * clamped_bonus.abs() / D) as i16;
        debug_assert!((self.entry as i32).abs() <= D);
    }
}

#[repr(transparent)]
pub struct AtomicStatsEntry<const D: i32> {
    pub entry: AtomicI16,
}

impl<const D: i32> AtomicStatsEntry<D> {
    #[inline(always)]
    pub fn new(val: i16) -> Self {
        Self {
            entry: AtomicI16::new(val),
        }
    }

    #[inline(always)]
    pub fn get(&self) -> i16 {
        self.entry.load(Ordering::Relaxed)
    }

    #[inline(always)]
    pub fn set(&self, val: i16) {
        self.entry.store(val, Ordering::Relaxed);
    }

    #[inline(always)]
    pub fn update(&self, bonus: i32) {
        let clamped_bonus = bonus.clamp(-D, D);
        let val = self.entry.load(Ordering::Relaxed) as i32;
        let new_val = (val + clamped_bonus - val * clamped_bonus.abs() / D) as i16;
        self.entry.store(new_val, Ordering::Relaxed);
    }
}

pub type ButterflyHistory = Box<[[StatsEntry<7183>; UINT_16_HISTORY_SIZE]; COLOR_NB]>;
pub type LowPlyHistory = Box<[[StatsEntry<7183>; UINT_16_HISTORY_SIZE]; LOW_PLY_HISTORY_SIZE]>;
pub type CapturePieceToHistory = [[[StatsEntry<10692>; PIECE_TYPE_NB]; SQUARE_NB]; PIECE_NB];
pub type PieceToHistory = [[AtomicStatsEntry<30000>; SQUARE_NB]; PIECE_NB];
pub type ContinuationHistoryBlock = [[[[PieceToHistory; SQUARE_NB]; PIECE_NB]; 2]; 2];
pub type ContinuationCorrectionHistory = Box<[[[[StatsEntry<CORRECTION_HISTORY_LIMIT>; SQUARE_NB]; PIECE_NB]; SQUARE_NB]; PIECE_NB]>;
pub type TTMoveHistory = StatsEntry<8192>;

#[repr(C)]
pub struct CorrectionBundle {
    pub pawn: AtomicStatsEntry<CORRECTION_HISTORY_LIMIT>,
    pub minor: AtomicStatsEntry<CORRECTION_HISTORY_LIMIT>,
    pub non_pawn_white: AtomicStatsEntry<CORRECTION_HISTORY_LIMIT>,
    pub non_pawn_black: AtomicStatsEntry<CORRECTION_HISTORY_LIMIT>,
}

impl CorrectionBundle {
    pub fn new(val: i16) -> Self {
        Self {
            pawn: AtomicStatsEntry::new(val),
            minor: AtomicStatsEntry::new(val),
            non_pawn_white: AtomicStatsEntry::new(val),
            non_pawn_black: AtomicStatsEntry::new(val),
        }
    }

    #[inline(always)]
    pub fn set(&self, val: i16) {
        self.pawn.set(val);
        self.minor.set(val);
        self.non_pawn_white.set(val);
        self.non_pawn_black.set(val);
    }
}

pub struct SharedHistories {
    pub correction_history: PageAlignedSlice<[CorrectionBundle; COLOR_NB]>,
    pub continuation_history: PageAlignedBox<ContinuationHistoryBlock>,
    pub pawn_history: PageAlignedSlice<[[AtomicStatsEntry<8192>; SQUARE_NB]; PIECE_NB]>,
    size_minus1: usize,
    pawn_hist_size_minus1: usize,
}

impl SharedHistories {
    /// Allocates page-aligned backing memory without serial initialization;
    /// callers must invoke `clear_range` across worker threads (or `clear`) before search.
    pub fn new_uninit(thread_count: usize) -> Self {
        let tc = if thread_count == 0 { 1 } else { thread_count.next_power_of_two() };
        let corr_size = CORRHIST_BASE_SIZE * tc;
        let pawn_size = PAWN_HISTORY_BASE_SIZE * tc;

        let correction_history = PageAlignedSlice::new_large_pages_zeroed(corr_size);
        let pawn_history = PageAlignedSlice::new_large_pages_zeroed(pawn_size);
        let continuation_history = PageAlignedBox::new_large_pages_zeroed();

        Self {
            correction_history,
            continuation_history,
            pawn_history,
            size_minus1: corr_size - 1,
            pawn_hist_size_minus1: pawn_size - 1,
        }
    }

    pub fn new(thread_count: usize) -> Self {
        let sh = Self::new_uninit(thread_count);
        sh.clear_range(0, 1);
        sh
    }

    #[inline(always)]
    pub fn pawn_entry(&self, pos: &Position) -> &[[AtomicStatsEntry<8192>; SQUARE_NB]; PIECE_NB] {
        let idx = (pos.pawn_key() as usize) & self.pawn_hist_size_minus1;
        unsafe { self.pawn_history.get_unchecked(idx) }
    }

    #[inline(always)]
    pub fn pawn_correction_entry(&self, pos: &Position) -> &[CorrectionBundle; COLOR_NB] {
        let idx = (pos.pawn_key() as usize) & self.size_minus1;
        unsafe { self.correction_history.get_unchecked(idx) }
    }

    #[inline(always)]
    pub fn minor_piece_correction_entry(&self, pos: &Position) -> &[CorrectionBundle; COLOR_NB] {
        let idx = (pos.minor_piece_key() as usize) & self.size_minus1;
        unsafe { self.correction_history.get_unchecked(idx) }
    }

    #[inline(always)]
    pub fn nonpawn_correction_entry(&self, c: Color, pos: &Position) -> &[CorrectionBundle; COLOR_NB] {
        let idx = (pos.non_pawn_key(c) as usize) & self.size_minus1;
        unsafe { self.correction_history.get_unchecked(idx) }
    }

    #[inline(always)]
    pub fn prefetch_keys(
        &self,
        pawn_key: Key,
        minor_key: Key,
        non_pawn_w: Key,
        non_pawn_b: Key,
        pc: Piece,
        to: Square,
    ) {
        #[cfg(target_arch = "x86_64")]
        unsafe {
            use core::arch::x86_64::{_mm_prefetch, _MM_HINT_T0};
            let p_pawn_hist = &self.pawn_history.get_unchecked((pawn_key as usize) & self.pawn_hist_size_minus1)[pc as usize][to.idx()] as *const _ as *const i8;
            let p_pawn_corr = self.correction_history.get_unchecked((pawn_key as usize) & self.size_minus1) as *const _ as *const i8;
            let p_minor_corr = self.correction_history.get_unchecked((minor_key as usize) & self.size_minus1) as *const _ as *const i8;
            let p_np_w = self.correction_history.get_unchecked((non_pawn_w as usize) & self.size_minus1) as *const _ as *const i8;
            let p_np_b = self.correction_history.get_unchecked((non_pawn_b as usize) & self.size_minus1) as *const _ as *const i8;
            _mm_prefetch(p_pawn_hist, _MM_HINT_T0);
            _mm_prefetch(p_pawn_corr, _MM_HINT_T0);
            _mm_prefetch(p_minor_corr, _MM_HINT_T0);
            _mm_prefetch(p_np_w, _MM_HINT_T0);
            _mm_prefetch(p_np_b, _MM_HINT_T0);
        }
    }

    /// Parallel slice clear matching C++ Stockfish's `Worker::clear()`!
    /// Each thread clears its own contiguous slice of `correction_history` and `pawn_history`;
    /// thread 0 also clears `continuation_history`.
    pub fn clear_range(&self, thread_idx: usize, thread_count: usize) {
        let corr_len = self.correction_history.len();
        let corr_start = (thread_idx as u64 * corr_len as u64 / thread_count as u64) as usize;
        let corr_end = if thread_idx + 1 == thread_count {
            corr_len
        } else {
            ((thread_idx + 1) as u64 * corr_len as u64 / thread_count as u64) as usize
        };
        let corr_slice = self.correction_history.as_slice();
        for entry in &corr_slice[corr_start..corr_end] {
            entry[0].set(-5);
            entry[1].set(-5);
        }

        let pawn_len = self.pawn_history.len();
        let pawn_start = (thread_idx as u64 * pawn_len as u64 / thread_count as u64) as usize;
        let pawn_end = if thread_idx + 1 == thread_count {
            pawn_len
        } else {
            ((thread_idx + 1) as u64 * pawn_len as u64 / thread_count as u64) as usize
        };
        let pawn_slice = self.pawn_history.as_slice();
        for entry in &pawn_slice[pawn_start..pawn_end] {
            for p in 0..PIECE_NB {
                for s in 0..SQUARE_NB {
                    entry[p][s].set(-1338);
                }
            }
        }

        if thread_idx == 0 {
            for in_check in 0..2 {
                for capture in 0..2 {
                    for prev_p in 0..PIECE_NB {
                        for prev_s in 0..SQUARE_NB {
                            for curr_p in 0..PIECE_NB {
                                for curr_s in 0..SQUARE_NB {
                                    self.continuation_history[in_check][capture][prev_p][prev_s][curr_p][curr_s].set(-586);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    pub fn clear(&self) {
        self.clear_range(0, 1);
    }
}

pub fn make_butterfly_history() -> ButterflyHistory {
    unsafe {
        let layout = std::alloc::Layout::new::<[[StatsEntry<7183>; UINT_16_HISTORY_SIZE]; COLOR_NB]>();
        let ptr = std::alloc::alloc(layout) as *mut [[StatsEntry<7183>; UINT_16_HISTORY_SIZE]; COLOR_NB];
        assert!(!ptr.is_null());
        for c in 0..COLOR_NB {
            for i in 0..UINT_16_HISTORY_SIZE {
                (*ptr)[c][i] = StatsEntry::new(-5);
            }
        }
        Box::from_raw(ptr)
    }
}

pub fn make_low_ply_history() -> LowPlyHistory {
    unsafe {
        let layout = std::alloc::Layout::new::<[[StatsEntry<7183>; UINT_16_HISTORY_SIZE]; LOW_PLY_HISTORY_SIZE]>();
        let ptr = std::alloc::alloc(layout) as *mut [[StatsEntry<7183>; UINT_16_HISTORY_SIZE]; LOW_PLY_HISTORY_SIZE];
        assert!(!ptr.is_null());
        for ply in 0..LOW_PLY_HISTORY_SIZE {
            for i in 0..UINT_16_HISTORY_SIZE {
                (*ptr)[ply][i] = StatsEntry::new(102);
            }
        }
        Box::from_raw(ptr)
    }
}

pub fn make_continuation_correction_history() -> ContinuationCorrectionHistory {
    unsafe {
        let layout = std::alloc::Layout::new::<[[[[StatsEntry<CORRECTION_HISTORY_LIMIT>; SQUARE_NB]; PIECE_NB]; SQUARE_NB]; PIECE_NB]>();
        let ptr = std::alloc::alloc(layout) as *mut [[[[StatsEntry<CORRECTION_HISTORY_LIMIT>; SQUARE_NB]; PIECE_NB]; SQUARE_NB]; PIECE_NB];
        assert!(!ptr.is_null());
        for p1 in 0..PIECE_NB {
            for s1 in 0..SQUARE_NB {
                for p2 in 0..PIECE_NB {
                    for s2 in 0..SQUARE_NB {
                        (*ptr)[p1][s1][p2][s2] = StatsEntry::new(5);
                    }
                }
            }
        }
        Box::from_raw(ptr)
    }
}
