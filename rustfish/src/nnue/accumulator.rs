use core::arch::x86_64::*;
use crate::bitboard::*;
use crate::position::Position;
use crate::types::*;
use super::common::L1;
use super::feature_transformer::FeatureTransformer;
use super::features::{FullThreats, K32H3, PP_3Wide};

const DIMENSIONS: usize = L1; // 1024
const TILE_HEIGHT: usize = 128; // 8 registers * 16 i16 = 128
type Tile = [__m256i; 8];

#[inline(always)]
unsafe fn load_tile(data: *const i16) -> Tile {
    let p = data as *const __m256i;
    [
        _mm256_load_si256(p.add(0)),
        _mm256_load_si256(p.add(1)),
        _mm256_load_si256(p.add(2)),
        _mm256_load_si256(p.add(3)),
        _mm256_load_si256(p.add(4)),
        _mm256_load_si256(p.add(5)),
        _mm256_load_si256(p.add(6)),
        _mm256_load_si256(p.add(7)),
    ]
}

#[inline(always)]
unsafe fn store_tile(dest: *mut i16, acc: Tile) {
    let p = dest as *mut __m256i;
    _mm256_store_si256(p.add(0), acc[0]);
    _mm256_store_si256(p.add(1), acc[1]);
    _mm256_store_si256(p.add(2), acc[2]);
    _mm256_store_si256(p.add(3), acc[3]);
    _mm256_store_si256(p.add(4), acc[4]);
    _mm256_store_si256(p.add(5), acc[5]);
    _mm256_store_si256(p.add(6), acc[6]);
    _mm256_store_si256(p.add(7), acc[7]);
}

#[inline(always)]
unsafe fn apply_tile<const SIGN: i32>(acc: &mut Tile, data: *const i16) {
    let p = data as *const __m256i;
    macro_rules! step {
        ($k:expr) => {
            let col = _mm256_load_si256(p.add($k));
            acc[$k] = if SIGN == 1 {
                _mm256_add_epi16(acc[$k], col)
            } else {
                _mm256_sub_epi16(acc[$k], col)
            };
        };
    }
    step!(0); step!(1); step!(2); step!(3);
    step!(4); step!(5); step!(6); step!(7);
}

#[inline(always)]
unsafe fn prefetch_next_tile(data: *const i8) {
    _mm_prefetch(data.add(TILE_HEIGHT), _MM_HINT_T0);
    _mm_prefetch(data.add(TILE_HEIGHT + 64), _MM_HINT_T0);
}

#[inline(always)]
unsafe fn apply_feature_single<const SIGN: i32, const PREFETCH: bool>(acc: &mut Tile, data: *const i8) {
    if PREFETCH {
        prefetch_next_tile(data);
    }
    let zero = _mm256_setzero_si256();
    let col = data as *const __m256i;
    macro_rules! step {
        ($k:expr, $c:expr) => {
            let weight = _mm256_load_si256(col.add($c));
            let lo = _mm256_srai_epi16(_mm256_unpacklo_epi8(zero, weight), 8);
            let hi = _mm256_srai_epi16(_mm256_unpackhi_epi8(zero, weight), 8);
            if SIGN == 1 {
                acc[$k] = _mm256_add_epi16(acc[$k], lo);
                acc[$k + 1] = _mm256_add_epi16(acc[$k + 1], hi);
            } else {
                acc[$k] = _mm256_sub_epi16(acc[$k], lo);
                acc[$k + 1] = _mm256_sub_epi16(acc[$k + 1], hi);
            }
        };
    }
    step!(0, 0); step!(2, 1); step!(4, 2); step!(6, 3);
}

#[inline(always)]
unsafe fn apply_feature_pair<const SIGN: i32, const PREFETCH: bool>(acc: &mut Tile, data0: *const i8, data1: *const i8) {
    if PREFETCH {
        prefetch_next_tile(data0);
        prefetch_next_tile(data1);
    }
    let ones = _mm256_set1_epi8(1);
    let col0 = data0 as *const __m256i;
    let col1 = data1 as *const __m256i;
    macro_rules! step {
        ($k:expr, $c:expr) => {
            let w0 = _mm256_load_si256(col0.add($c));
            let w1 = _mm256_load_si256(col1.add($c));
            let lo = _mm256_maddubs_epi16(ones, _mm256_unpacklo_epi8(w0, w1));
            let hi = _mm256_maddubs_epi16(ones, _mm256_unpackhi_epi8(w0, w1));
            if SIGN == 1 {
                acc[$k] = _mm256_add_epi16(acc[$k], lo);
                acc[$k + 1] = _mm256_add_epi16(acc[$k + 1], hi);
            } else {
                acc[$k] = _mm256_sub_epi16(acc[$k], lo);
                acc[$k + 1] = _mm256_sub_epi16(acc[$k + 1], hi);
            }
        };
    }
    step!(0, 0); step!(2, 1); step!(4, 2); step!(6, 3);
}

#[inline(always)]
unsafe fn apply_feature_diff<const PREFETCH: bool>(acc: &mut Tile, add: *const i8, sub: *const i8) {
    if PREFETCH {
        prefetch_next_tile(add);
        prefetch_next_tile(sub);
    }
    let bias = _mm256_set1_epi8(-128);
    let signs = _mm256_set1_epi16(-255);
    let col_add = add as *const __m256i;
    let col_sub = sub as *const __m256i;
    macro_rules! step {
        ($k:expr, $c:expr) => {
            let ua = _mm256_xor_si256(_mm256_load_si256(col_add.add($c)), bias);
            let us = _mm256_xor_si256(_mm256_load_si256(col_sub.add($c)), bias);
            acc[$k] = _mm256_add_epi16(acc[$k], _mm256_maddubs_epi16(_mm256_unpacklo_epi8(ua, us), signs));
            acc[$k + 1] = _mm256_add_epi16(acc[$k + 1], _mm256_maddubs_epi16(_mm256_unpackhi_epi8(ua, us), signs));
        };
    }
    step!(0, 0); step!(2, 1); step!(4, 2); step!(6, 3);
}

#[inline(always)]
unsafe fn apply_features<const SIGN: i32>(
    acc: &mut Tile,
    indices: &[u16],
    weights: *const i8,
) {
    let count = indices.len();
    let mut i = 0;
    while i + 1 < count {
        apply_feature_pair::<SIGN, true>(
            acc,
            weights.add((*indices.get_unchecked(i) as usize) * DIMENSIONS),
            weights.add((*indices.get_unchecked(i + 1) as usize) * DIMENSIONS),
        );
        i += 2;
    }
    if i < count {
        apply_feature_single::<SIGN, true>(
            acc,
            weights.add((*indices.get_unchecked(i) as usize) * DIMENSIONS),
        );
    }
}

#[inline(always)]
unsafe fn apply_features_but_odd<const SIGN: i32>(
    acc: &mut Tile,
    psq: &[u16],
    mut thr: &[u16],
    ft: &FeatureTransformer,
    psq_base: *const i8,
    j: usize,
    odd: &mut Option<*const i8>,
) {
    let thr_weights = ft.threat_and_pp_weights.as_ptr().add(j);
    let psq_count = psq.len();
    *odd = None;

    let mut i = 0;
    while i + 1 < psq_count {
        apply_feature_pair::<SIGN, true>(
            acc,
            psq_base.add((*psq.get_unchecked(i) as usize) * DIMENSIONS + j),
            psq_base.add((*psq.get_unchecked(i + 1) as usize) * DIMENSIONS + j),
        );
        i += 2;
    }

    if i < psq_count {
        if thr.is_empty() {
            *odd = Some(psq_base.add((*psq.get_unchecked(i) as usize) * DIMENSIONS + j));
            return;
        }
        apply_feature_pair::<SIGN, true>(
            acc,
            psq_base.add((*psq.get_unchecked(i) as usize) * DIMENSIONS + j),
            thr_weights.add((*thr.get_unchecked(0) as usize) * DIMENSIONS),
        );
        thr = thr.get_unchecked(1..);
    }

    let mut thr_count = thr.len();
    if thr_count % 2 != 0 {
        thr_count -= 1;
        *odd = Some(thr_weights.add((*thr.get_unchecked(thr_count) as usize) * DIMENSIONS));
    }

    apply_features::<SIGN>(acc, thr.get_unchecked(..thr_count), thr_weights);
}

#[inline(always)]
unsafe fn apply_odd_features(
    acc: &mut Tile,
    removed: Option<*const i8>,
    added: Option<*const i8>,
) {
    if let (Some(rem), Some(add)) = (removed, added) {
        apply_feature_diff::<true>(acc, add, rem);
        return;
    }
    if let Some(rem) = removed {
        apply_feature_single::<-1, true>(acc, rem);
    }
    if let Some(add) = added {
        apply_feature_single::<1, true>(acc, add);
    }
}

#[inline(always)]
unsafe fn apply_psq_features(
    acc: &mut Tile,
    removed: &[u16],
    added: &[u16],
    ft: &FeatureTransformer,
    psq_weights: *const i8,
    j: usize,
) {
    let mut odd_removed = None;
    let mut odd_added = None;
    apply_features_but_odd::<-1>(acc, removed, &[], ft, psq_weights, j, &mut odd_removed);
    apply_features_but_odd::<1>(acc, added, &[], ft, psq_weights, j, &mut odd_added);
    apply_odd_features(acc, odd_removed, odd_added);
}

#[inline(always)]
unsafe fn apply_threat_features<const SIGN: i32>(
    acc: &mut Tile,
    list: &[u16],
    ft: &FeatureTransformer,
    j: usize,
) {
    apply_features::<SIGN>(acc, list, ft.threat_and_pp_weights.as_ptr().add(j));
}

#[inline(always)]
unsafe fn apply_combined_features(
    acc: &mut Tile,
    psq_removed: &[u16],
    thr_removed: &[u16],
    psq_added: &[u16],
    thr_added: &[u16],
    ft: &FeatureTransformer,
    psq_base: *const i8,
    j: usize,
) {
    let mut odd_removed = None;
    let mut odd_added = None;
    apply_features_but_odd::<-1>(acc, psq_removed, thr_removed, ft, psq_base, j, &mut odd_removed);
    apply_features_but_odd::<1>(acc, psq_added, thr_added, ft, psq_base, j, &mut odd_added);
    apply_odd_features(acc, odd_removed, odd_added);
}

#[inline(always)]
unsafe fn get_changed_pieces(old_pieces: &[Piece; SQUARE_NB], new_pieces: &[Piece; SQUARE_NB]) -> Bitboard {
    let mut same_bb = 0u64;
    for i in (0..64).step_by(32) {
        let old_v = _mm256_loadu_si256((old_pieces.as_ptr() as *const __m256i).add(i / 32));
        let new_v = _mm256_loadu_si256((new_pieces.as_ptr() as *const __m256i).add(i / 32));
        let cmp_equal = _mm256_cmpeq_epi8(old_v, new_v);
        let equal_mask = _mm256_movemask_epi8(cmp_equal) as u32;
        same_bb |= (equal_mask as u64) << i;
    }
    !same_bb
}

#[inline(always)]
unsafe fn append_changed_psq_indices(
    perspective: Color,
    ksq: Square,
    old_pieces: &[Piece; SQUARE_NB],
    old_bb: Bitboard,
    new_pieces: &[Piece; SQUARE_NB],
    new_bb: Bitboard,
    removed: &mut ValueList<u16, 32>,
    added: &mut ValueList<u16, 32>,
) {
    let changed_bb = get_changed_pieces(old_pieces, new_pieces);
    let mut removed_bb = changed_bb & old_bb;
    let mut added_bb = changed_bb & new_bb;

    while removed_bb != 0 {
        let sq = removed_bb.pop_lsb();
        removed.push(K32H3::make_index(perspective, sq, old_pieces[sq as usize], ksq) as u16);
    }
    while added_bb != 0 {
        let sq = added_bb.pop_lsb();
        added.push(K32H3::make_index(perspective, sq, new_pieces[sq as usize], ksq) as u16);
    }
}

// -------------------------------------------------------------
// Accumulator Caches (Finny Tables)
// -------------------------------------------------------------

#[repr(C, align(64))]
#[derive(Clone)]
pub struct CacheEntry {
    pub accumulation: [i16; L1],
    pub pieces: [Piece; SQUARE_NB],
    pub piece_bb: Bitboard,
}

impl CacheEntry {
    pub fn new() -> Self {
        Self {
            accumulation: [0; L1],
            pieces: [Piece::NoPiece; SQUARE_NB],
            piece_bb: 0,
        }
    }

    pub fn clear(&mut self, biases: &[i16; L1]) {
        self.accumulation.copy_from_slice(biases);
        self.pieces = [Piece::NoPiece; SQUARE_NB];
        self.piece_bb = 0;
    }
}

pub struct AccumulatorCaches {
    pub entries: Vec<CacheEntry>,
}

impl AccumulatorCaches {
    #[inline(always)]
    fn idx(block: usize, sq: usize, c: usize) -> usize {
        (block * SQUARE_NB + sq) * COLOR_NB + c
    }

    pub fn new(biases: &[i16; L1]) -> Self {
        let total = K32H3::NUM_BLOCKS * SQUARE_NB * COLOR_NB;
        let mut caches = Self {
            entries: vec![CacheEntry::new(); total],
        };
        caches.clear(biases);
        caches
    }

    #[inline(always)]
    pub fn entry(&self, block: usize, sq: usize, c: usize) -> &CacheEntry {
        &self.entries[Self::idx(block, sq, c)]
    }

    #[inline(always)]
    pub fn entry_mut(&mut self, block: usize, sq: usize, c: usize) -> &mut CacheEntry {
        let idx = Self::idx(block, sq, c);
        &mut self.entries[idx]
    }

    pub fn clear(&mut self, biases: &[i16; L1]) {
        for entry in &mut self.entries {
            entry.clear(biases);
        }
    }
}

// -------------------------------------------------------------
// Accumulator State & Stack
// -------------------------------------------------------------

#[repr(C, align(64))]
#[derive(Clone)]
pub struct AccumulatorState {
    pub accumulation: [[i16; L1]; COLOR_NB],
    pub computed: [bool; COLOR_NB],
    pub block_before: [u8; COLOR_NB],
    pub dirties: Dirties,
}

impl AccumulatorState {
    pub fn new() -> Self {
        Self {
            accumulation: [[0; L1]; COLOR_NB],
            computed: [false; COLOR_NB],
            block_before: [0; COLOR_NB],
            dirties: Dirties::default(),
        }
    }
}

pub struct AccumulatorStack {
    pub accumulators: Vec<AccumulatorState>,
    pub size: usize,
}

impl AccumulatorStack {
    pub fn new() -> Self {
        Self {
            accumulators: vec![AccumulatorState::new(); MAX_PLY + 16],
            size: 1,
        }
    }

    #[inline(always)]
    pub fn latest(&self) -> &AccumulatorState {
        unsafe { self.accumulators.get_unchecked(self.size - 1) }
    }

    #[inline(always)]
    pub fn mut_latest(&mut self) -> &mut AccumulatorState {
        unsafe { self.accumulators.get_unchecked_mut(self.size - 1) }
    }

    pub fn reset(&mut self) {
        self.accumulators[0].dirties = Dirties::default();
        self.accumulators[0].computed = [false; COLOR_NB];
        self.size = 1;
    }

    #[inline(always)]
    pub fn push(&mut self, pos: &Position) -> &mut AccumulatorState {
        debug_assert!(self.size < MAX_PLY + 1);
        let idx = self.size;
        let acc = unsafe { self.accumulators.get_unchecked_mut(idx) };
        acc.computed = [false; COLOR_NB];
        acc.block_before = K32H3::block_state(pos);
        acc.dirties.dirty_threats.list.clear();
        self.size += 1;
        acc
    }

    #[inline(always)]
    pub fn pop(&mut self) {
        debug_assert!(self.size > 1);
        self.size -= 1;
    }

    pub unsafe fn evaluate(
        &mut self,
        pos: &Position,
        ft: &FeatureTransformer,
        cache: &mut AccumulatorCaches,
    ) {
        let last_white = self.find_last_usable_accumulator(Color::White, pos);
        let last_black = self.find_last_usable_accumulator(Color::Black, pos);

        if self.accumulators.get_unchecked(last_white).computed[Color::White as usize]
            && self.accumulators.get_unchecked(last_black).computed[Color::Black as usize]
        {
            self.forward_update_incremental_both(pos, ft, last_white, last_black);
        } else {
            self.evaluate_side(Color::White, pos, ft, cache, last_white);
            self.evaluate_side(Color::Black, pos, ft, cache, last_black);
        }
    }

    #[cold]
    #[inline(never)]
    unsafe fn evaluate_side(
        &mut self,
        perspective: Color,
        pos: &Position,
        ft: &FeatureTransformer,
        cache: &mut AccumulatorCaches,
        last_usable_accum: usize,
    ) {
        const MIN_PC_COUNT_HYBRID: usize = 15;
        let c_idx = perspective as usize;

        if self.accumulators.get_unchecked(last_usable_accum).computed[c_idx] {
            self.forward_update_incremental(perspective, pos, ft, last_usable_accum);
        } else {
            let dirty_piece = self.latest().dirties.dirty_piece;
            let current_block = K32H3::block(pos, perspective);

            if dirty_piece.pc == Piece::make(perspective, PieceType::King)
                && self.latest().block_before[c_idx] == current_block as u8
                && self.size >= 2
                && self.accumulators.get_unchecked(self.size - 2).computed[c_idx]
                && pos.count_all() >= MIN_PC_COUNT_HYBRID
                && ((dirty_piece.from as usize & 0b100) == (dirty_piece.to as usize & 0b100))
                && dirty_piece.add_sq == Square::SqNone
            {
                let base = self.accumulators.as_mut_ptr();
                update_accumulator_hybrid(
                    perspective,
                    pos,
                    ft,
                    &mut *base.add(self.size - 1),
                    &*base.add(self.size - 2),
                    cache,
                );
                return;
            }

            update_accumulator_refresh_cache(perspective, ft, pos, self.mut_latest(), cache);
            self.backward_update_incremental(perspective, pos, ft, last_usable_accum);
        }
    }

    #[inline(always)]
    fn find_last_usable_accumulator(&self, perspective: Color, pos: &Position) -> usize {
        let psq_block = K32H3::block(pos, perspective);
        let c_idx = perspective as usize;

        let mut curr_idx = self.size - 1;
        while curr_idx > 0 {
            let st = unsafe { self.accumulators.get_unchecked(curr_idx) };
            if st.computed[c_idx] {
                return curr_idx;
            }
            let dirty_piece = st.dirties.dirty_piece;
            let before = st.block_before[c_idx] as usize;
            if K32H3::requires_refresh(&dirty_piece, perspective, before, psq_block) {
                return curr_idx;
            }
            curr_idx -= 1;
        }
        0
    }

    unsafe fn forward_update_incremental(
        &mut self,
        perspective: Color,
        pos: &Position,
        ft: &FeatureTransformer,
        begin: usize,
    ) {
        let ksq = pos.square_color(PieceType::King, perspective);
        let psq_block = K32H3::block(pos, perspective);
        let base = self.accumulators.as_mut_ptr();

        for next in (begin + 1)..self.size {
            update_accumulator_incremental::<true>(
                perspective,
                ft,
                ksq,
                psq_block,
                &mut *base.add(next),
                &*base.add(next - 1),
            );
        }
    }

    unsafe fn backward_update_incremental(
        &mut self,
        perspective: Color,
        pos: &Position,
        ft: &FeatureTransformer,
        end: usize,
    ) {
        let ksq = pos.square_color(PieceType::King, perspective);
        let psq_block = K32H3::block(pos, perspective);
        let base = self.accumulators.as_mut_ptr();

        let mut next = (self.size as isize) - 2;
        while next >= end as isize {
            let u_next = next as usize;
            update_accumulator_incremental::<false>(
                perspective,
                ft,
                ksq,
                psq_block,
                &mut *base.add(u_next),
                &*base.add(u_next + 1),
            );
            next -= 1;
        }
    }

    unsafe fn forward_update_incremental_both(
        &mut self,
        pos: &Position,
        ft: &FeatureTransformer,
        white_begin: usize,
        black_begin: usize,
    ) {
        let white_ksq = pos.square_color(PieceType::King, Color::White);
        let black_ksq = pos.square_color(PieceType::King, Color::Black);
        let white_block = K32H3::block(pos, Color::White);
        let black_block = K32H3::block(pos, Color::Black);
        let shared_begin = white_begin.max(black_begin);
        let base = self.accumulators.as_mut_ptr();

        for next in (white_begin + 1)..=shared_begin {
            update_accumulator_incremental::<true>(
                Color::White,
                ft,
                white_ksq,
                white_block,
                &mut *base.add(next),
                &*base.add(next - 1),
            );
        }

        for next in (black_begin + 1)..=shared_begin {
            update_accumulator_incremental::<true>(
                Color::Black,
                ft,
                black_ksq,
                black_block,
                &mut *base.add(next),
                &*base.add(next - 1),
            );
        }

        for next in (shared_begin + 1)..self.size {
            update_accumulator_incremental_both(
                ft,
                white_ksq,
                black_ksq,
                white_block,
                black_block,
                &mut *base.add(next),
                &*base.add(next - 1),
            );
        }
    }
}

// -------------------------------------------------------------
// Accumulator Update Implementations
// -------------------------------------------------------------

#[cold]
#[inline(never)]
unsafe fn update_accumulator_refresh_cache(
    perspective: Color,
    ft: &FeatureTransformer,
    pos: &Position,
    accumulator: &mut AccumulatorState,
    cache: &mut AccumulatorCaches,
) {
    let psq_block = K32H3::block(pos, perspective);
    let ksq = pos.square_color(PieceType::King, perspective);
    let c_idx = perspective as usize;
    let entry = cache.entry_mut(psq_block, ksq as usize, c_idx);
    let psq_weights = ft.psq_weights(psq_block);

    let mut removed = ValueList::<u16, 32>::new();
    let mut added = ValueList::<u16, 32>::new();
    append_changed_psq_indices(
        perspective,
        ksq,
        &entry.pieces,
        entry.piece_bb,
        pos.piece_array(),
        pos.pieces(),
        &mut removed,
        &mut added,
    );

    entry.piece_bb = pos.pieces();
    entry.pieces = *pos.piece_array();

    let mut active = ValueList::<u16, 256>::new();
    FullThreats::append_active_indices(perspective, pos, &mut active);
    PP_3Wide::append_active_indices(perspective, pos, &mut active);

    accumulator.computed[c_idx] = true;

    let mut j = 0;
    while j < DIMENSIONS {
        let mut acc = load_tile(entry.accumulation.as_ptr().add(j));

        apply_psq_features(&mut acc, removed.as_slice(), added.as_slice(), ft, psq_weights, j);
        store_tile(entry.accumulation.as_mut_ptr().add(j), acc);

        apply_threat_features::<1>(&mut acc, active.as_slice(), ft, j);
        store_tile(accumulator.accumulation[c_idx].as_mut_ptr().add(j), acc);

        j += TILE_HEIGHT;
    }
}

#[inline(always)]
unsafe fn apply_combined_tile<const PREFETCH: bool>(
    from_acc: *const i16,
    to_acc: *mut i16,
    j: usize,
    rem_ptrs: &[*const i8],
    add_ptrs: &[*const i8],
    odd_rem: *const i8,
    odd_add: *const i8,
) {
    let mut acc = load_tile(from_acc.add(j));

    let mut i = 0;
    let rem_len = rem_ptrs.len();
    while i < rem_len {
        apply_feature_pair::<-1, PREFETCH>(
            &mut acc,
            (*rem_ptrs.get_unchecked(i)).add(j),
            (*rem_ptrs.get_unchecked(i + 1)).add(j),
        );
        i += 2;
    }

    let mut i = 0;
    let add_len = add_ptrs.len();
    while i < add_len {
        apply_feature_pair::<1, PREFETCH>(
            &mut acc,
            (*add_ptrs.get_unchecked(i)).add(j),
            (*add_ptrs.get_unchecked(i + 1)).add(j),
        );
        i += 2;
    }

    if !odd_rem.is_null() && !odd_add.is_null() {
        apply_feature_diff::<PREFETCH>(&mut acc, odd_add.add(j), odd_rem.add(j));
    } else if !odd_rem.is_null() {
        apply_feature_single::<-1, PREFETCH>(&mut acc, odd_rem.add(j));
    } else if !odd_add.is_null() {
        apply_feature_single::<1, PREFETCH>(&mut acc, odd_add.add(j));
    }

    store_tile(to_acc.add(j), acc);
}

#[inline(never)]
unsafe fn apply_combined_dispatch(
    perspective: Color,
    ft: &FeatureTransformer,
    from: &AccumulatorState,
    to: &mut AccumulatorState,
    psq_removed: &[u16],
    thr_removed: &[u16],
    psq_added: &[u16],
    thr_added: &[u16],
    psq_weights: *const i8,
) {
    let thr_weights = ft.threat_and_pp_weights.as_ptr();
    let mut rem_buf = [const { core::mem::MaybeUninit::<*const i8>::uninit() }; 288];
    let mut add_buf = [const { core::mem::MaybeUninit::<*const i8>::uninit() }; 288];

    let mut n_rem = 0usize;
    for &idx in psq_removed {
        rem_buf.get_unchecked_mut(n_rem).write(psq_weights.add((idx as usize) * DIMENSIONS));
        n_rem += 1;
    }
    for &idx in thr_removed {
        rem_buf.get_unchecked_mut(n_rem).write(thr_weights.add((idx as usize) * DIMENSIONS));
        n_rem += 1;
    }

    let mut n_add = 0usize;
    for &idx in psq_added {
        add_buf.get_unchecked_mut(n_add).write(psq_weights.add((idx as usize) * DIMENSIONS));
        n_add += 1;
    }
    for &idx in thr_added {
        add_buf.get_unchecked_mut(n_add).write(thr_weights.add((idx as usize) * DIMENSIONS));
        n_add += 1;
    }

    let odd_rem = if (n_rem & 1) != 0 {
        n_rem -= 1;
        rem_buf.get_unchecked(n_rem).assume_init()
    } else {
        core::ptr::null()
    };
    let odd_add = if (n_add & 1) != 0 {
        n_add -= 1;
        add_buf.get_unchecked(n_add).assume_init()
    } else {
        core::ptr::null()
    };

    let rem_pairs = core::slice::from_raw_parts(rem_buf.as_ptr() as *const *const i8, n_rem);
    let add_pairs = core::slice::from_raw_parts(add_buf.as_ptr() as *const *const i8, n_add);

    let c_idx = perspective as usize;
    let from_acc = from.accumulation[c_idx].as_ptr();
    let to_acc = to.accumulation[c_idx].as_mut_ptr();

    let mut j = 0;
    while j < DIMENSIONS - TILE_HEIGHT {
        apply_combined_tile::<true>(from_acc, to_acc, j, rem_pairs, add_pairs, odd_rem, odd_add);
        j += TILE_HEIGHT;
    }
    apply_combined_tile::<false>(from_acc, to_acc, DIMENSIONS - TILE_HEIGHT, rem_pairs, add_pairs, odd_rem, odd_add);
}

unsafe fn update_accumulator_incremental<const FORWARD: bool>(
    perspective: Color,
    ft: &FeatureTransformer,
    ksq: Square,
    psq_block: usize,
    target_state: &mut AccumulatorState,
    computed: &AccumulatorState,
) {
    let c_idx = perspective as usize;
    let mut psq_removed = ValueList::<u16, 32>::new();
    let mut psq_added = ValueList::<u16, 32>::new();
    let mut thr_removed = ValueList::<u16, 256>::new();
    let mut thr_added = ValueList::<u16, 256>::new();

    let dirty_piece = if FORWARD { &target_state.dirties.dirty_piece } else { &computed.dirties.dirty_piece };
    let dirty_threats = if FORWARD { &target_state.dirties.dirty_threats } else { &computed.dirties.dirty_threats };
    let dirty_pawn_pairs = if FORWARD { &target_state.dirties.dirty_pawn_pairs } else { &computed.dirties.dirty_pawn_pairs };
    let threat_pp_base = ft.threat_and_pp_weights.as_ptr();

    if FORWARD {
        FullThreats::append_changed_indices(perspective, ksq, dirty_threats, &mut thr_removed, &mut thr_added, threat_pp_base);
        PP_3Wide::append_changed_indices(perspective, ksq, dirty_pawn_pairs, &mut thr_removed, &mut thr_added, threat_pp_base);
        K32H3::append_changed_indices(perspective, ksq, dirty_piece, &mut psq_removed, &mut psq_added);
    } else {
        FullThreats::append_changed_indices(perspective, ksq, dirty_threats, &mut thr_added, &mut thr_removed, threat_pp_base);
        PP_3Wide::append_changed_indices(perspective, ksq, dirty_pawn_pairs, &mut thr_added, &mut thr_removed, threat_pp_base);
        K32H3::append_changed_indices(perspective, ksq, dirty_piece, &mut psq_added, &mut psq_removed);
    }

    let psq_weights = ft.psq_weights(psq_block);
    apply_combined_dispatch(
        perspective,
        ft,
        computed,
        target_state,
        psq_removed.as_slice(),
        thr_removed.as_slice(),
        psq_added.as_slice(),
        thr_added.as_slice(),
        psq_weights,
    );

    target_state.computed[c_idx] = true;
}

unsafe fn update_accumulator_incremental_both(
    ft: &FeatureTransformer,
    white_ksq: Square,
    black_ksq: Square,
    white_block: usize,
    black_block: usize,
    target_state: &mut AccumulatorState,
    computed: &AccumulatorState,
) {
    let mut psq_removed_w = ValueList::<u16, 32>::new();
    let mut psq_added_w = ValueList::<u16, 32>::new();
    let mut psq_removed_b = ValueList::<u16, 32>::new();
    let mut psq_added_b = ValueList::<u16, 32>::new();

    let mut thr_removed_w = ValueList::<u16, 256>::new();
    let mut thr_added_w = ValueList::<u16, 256>::new();
    let mut thr_removed_b = ValueList::<u16, 256>::new();
    let mut thr_added_b = ValueList::<u16, 256>::new();

    let threat_pp_base = ft.threat_and_pp_weights.as_ptr();

    FullThreats::append_changed_indices_both(
        white_ksq,
        black_ksq,
        &target_state.dirties.dirty_threats,
        &mut thr_removed_w,
        &mut thr_added_w,
        &mut thr_removed_b,
        &mut thr_added_b,
        threat_pp_base,
    );
    PP_3Wide::append_changed_indices_both(
        white_ksq,
        black_ksq,
        &target_state.dirties.dirty_pawn_pairs,
        &mut thr_removed_w,
        &mut thr_added_w,
        &mut thr_removed_b,
        &mut thr_added_b,
        threat_pp_base,
    );
    K32H3::append_changed_indices(
        Color::White,
        white_ksq,
        &target_state.dirties.dirty_piece,
        &mut psq_removed_w,
        &mut psq_added_w,
    );
    K32H3::append_changed_indices(
        Color::Black,
        black_ksq,
        &target_state.dirties.dirty_piece,
        &mut psq_removed_b,
        &mut psq_added_b,
    );

    apply_combined_dispatch(
        Color::White,
        ft,
        computed,
        target_state,
        psq_removed_w.as_slice(),
        thr_removed_w.as_slice(),
        psq_added_w.as_slice(),
        thr_added_w.as_slice(),
        ft.psq_weights(white_block),
    );
    apply_combined_dispatch(
        Color::Black,
        ft,
        computed,
        target_state,
        psq_removed_b.as_slice(),
        thr_removed_b.as_slice(),
        psq_added_b.as_slice(),
        thr_added_b.as_slice(),
        ft.psq_weights(black_block),
    );

    target_state.computed[Color::White as usize] = true;
    target_state.computed[Color::Black as usize] = true;
}

#[cold]
#[inline(never)]
unsafe fn update_accumulator_hybrid(
    perspective: Color,
    pos: &Position,
    ft: &FeatureTransformer,
    target: &mut AccumulatorState,
    computed: &AccumulatorState,
    cache: &mut AccumulatorCaches,
) {
    let dirty_piece = target.dirties.dirty_piece;
    let old_ksq = dirty_piece.from;
    let new_ksq = dirty_piece.to;
    let c_idx = perspective as usize;

    let current_pieces = *pos.piece_array();
    let mut previous_pieces = current_pieces;
    let mut previous_piece_bb = pos.pieces();

    if dirty_piece.remove_sq != Square::SqNone {
        previous_pieces[new_ksq as usize] = dirty_piece.remove_pc;
    } else {
        previous_pieces[new_ksq as usize] = Piece::NoPiece;
        previous_piece_bb &= !square_bb(new_ksq);
    }

    previous_pieces[old_ksq as usize] = Piece::make(perspective, PieceType::King);
    previous_piece_bb |= square_bb(old_ksq);

    let psq_block = K32H3::block(pos, perspective);
    let psq_weights = ft.psq_weights(psq_block);

    let old_idx = AccumulatorCaches::idx(psq_block, old_ksq as usize, c_idx);
    let new_idx = AccumulatorCaches::idx(psq_block, new_ksq as usize, c_idx);
    debug_assert_ne!(old_idx, new_idx);
    let base_ptr = cache.entries.as_mut_ptr();
    let old_entry = &*base_ptr.add(old_idx);
    let new_entry = &mut *base_ptr.add(new_idx);

    let mut old_remove = ValueList::<u16, 32>::new();
    let mut old_add = ValueList::<u16, 32>::new();
    let mut new_remove = ValueList::<u16, 32>::new();
    let mut new_add = ValueList::<u16, 32>::new();

    append_changed_psq_indices(
        perspective,
        old_ksq,
        &old_entry.pieces,
        old_entry.piece_bb,
        &previous_pieces,
        previous_piece_bb,
        &mut old_remove,
        &mut old_add,
    );
    append_changed_psq_indices(
        perspective,
        new_ksq,
        &new_entry.pieces,
        new_entry.piece_bb,
        &current_pieces,
        pos.pieces(),
        &mut new_remove,
        &mut new_add,
    );

    let threat_pp_base = ft.threat_and_pp_weights.as_ptr();
    let mut thr_removed = ValueList::<u16, 256>::new();
    let mut thr_added = ValueList::<u16, 256>::new();
    FullThreats::append_changed_indices(perspective, new_ksq, &target.dirties.dirty_threats, &mut thr_removed, &mut thr_added, threat_pp_base);
    PP_3Wide::append_changed_indices(perspective, new_ksq, &target.dirties.dirty_pawn_pairs, &mut thr_removed, &mut thr_added, threat_pp_base);

    let from_acc = computed.accumulation[c_idx].as_ptr();
    let to_acc = target.accumulation[c_idx].as_mut_ptr();

    let mut j = 0;
    while j < DIMENSIONS {
        let mut acc = load_tile(new_entry.accumulation.as_ptr().add(j));
        apply_psq_features(&mut acc, new_remove.as_slice(), new_add.as_slice(), ft, psq_weights, j);
        store_tile(new_entry.accumulation.as_mut_ptr().add(j), acc);

        apply_tile::<1>(&mut acc, from_acc.add(j));
        apply_tile::<-1>(&mut acc, old_entry.accumulation.as_ptr().add(j));

        apply_combined_features(
            &mut acc,
            old_add.as_slice(),
            thr_removed.as_slice(),
            old_remove.as_slice(),
            thr_added.as_slice(),
            ft,
            psq_weights,
            j,
        );

        store_tile(to_acc.add(j), acc);
        j += TILE_HEIGHT;
    }

    new_entry.pieces = current_pieces;
    new_entry.piece_bb = pos.pieces();
    target.computed[c_idx] = true;
}
