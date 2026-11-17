use crate::bitboard::*;
use crate::position::Position;
use crate::types::*;
use super::super::common::{IndexType, ORIENT_TBL};

pub const HASH_VALUE: u32 = 0x5C80FA69;
pub const PAWN_IDS: usize = COLOR_NB * 48;
pub const DIMENSIONS: usize = PAWN_IDS * (PAWN_IDS - 1) / 2; // 4560
pub const INDEX_BASE: usize = 59808;

#[inline(always)]
const fn make_pawn_id(color: Color, square: Square) -> IndexType {
    48 * (color as usize) + (square as usize) - (Square::SqA2 as usize)
}

#[inline(always)]
const fn pp_idx(a: IndexType, b: IndexType) -> IndexType {
    let hi = if a > b { a } else { b };
    let lo = if a < b { a } else { b };
    hi * (hi - 1) / 2 + lo + INDEX_BASE
}

#[inline(always)]
pub fn pawn_pair_bb(s: Square) -> Bitboard {
    let f = file_of(s) as usize;
    let mut files = file_bb(File::from_usize(f));
    if f > 0 {
        files |= file_bb(File::from_usize(f - 1));
    }
    if f < 7 {
        files |= file_bb(File::from_usize(f + 1));
    }
    files & !(RANK_1_BB | RANK_8_BB) & !square_bb(s)
}

#[allow(non_camel_case_types)]
pub struct PP_3Wide;

impl PP_3Wide {
    pub const DIMENSIONS: usize = DIMENSIONS;

    #[inline(always)]
    pub fn make_index(
        perspective: Color,
        color: Color,
        from: Square,
        to: Square,
        paired_color: Color,
        ksq: Square,
    ) -> IndexType {
        let orientation = ORIENT_TBL[ksq as usize] ^ (56 * (perspective as u8));
        let from_oriented = Square::from_u8((from as u8) ^ orientation);
        let to_oriented = Square::from_u8((to as u8) ^ orientation);

        let color_oriented = Color::from_usize((color as usize) ^ (perspective as usize));
        let paired_color_oriented = Color::from_usize((paired_color as usize) ^ (perspective as usize));

        let id_a = make_pawn_id(color_oriented, from_oriented);
        let id_b = make_pawn_id(paired_color_oriented, to_oriented);
        pp_idx(id_a, id_b)
    }

    pub fn append_active_indices(perspective: Color, pos: &Position, active: &mut ValueList<u16, 256>) {
        let ksq = pos.square_color(PieceType::King, perspective);
        let pawns_w = pos.pieces_color(PieceType::Pawn, Color::White);
        let pawns_b = pos.pieces_color(PieceType::Pawn, Color::Black);
        let updated = pawns_w | pawns_b;

        let orientation = ORIENT_TBL[ksq as usize] ^ (56 * (perspective as u8));
        let unchanged = 0; // for active indices of whole board, all pawns are in updated

        let mut u = updated;
        while u != 0 {
            let a = u.pop_lsb();
            let partners = pawn_pair_bb(a) & (unchanged | u);
            if partners == 0 {
                continue;
            }

            let a_col = if (pawns_b & square_bb(a)) != 0 { Color::Black } else { Color::White };
            let a_id = make_pawn_id(
                Color::from_usize((a_col as usize) ^ (perspective as usize)),
                Square::from_u8((a as u8) ^ orientation),
            );

            let mut pb = pawns_b & partners;
            while pb != 0 {
                let to = pb.pop_lsb();
                let b_id = make_pawn_id(
                    Color::from_usize((Color::Black as usize) ^ (perspective as usize)),
                    Square::from_u8((to as u8) ^ orientation),
                );
                active.push(pp_idx(a_id, b_id) as u16);
            }

            let mut pw = pawns_w & partners;
            while pw != 0 {
                let to = pw.pop_lsb();
                let b_id = make_pawn_id(
                    Color::from_usize((Color::White as usize) ^ (perspective as usize)),
                    Square::from_u8((to as u8) ^ orientation),
                );
                active.push(pp_idx(a_id, b_id) as u16);
            }
        }
    }

    #[inline(always)]
    pub fn append_changed_indices(
        perspective: Color,
        ksq: Square,
        diff: &DirtyPawnPairs,
        removed: &mut ValueList<u16, 256>,
        added: &mut ValueList<u16, 256>,
        prefetch_base: *const i8,
    ) {
        let white_before = diff.before[Color::White as usize];
        let black_before = diff.before[Color::Black as usize];
        let white_after = diff.after[Color::White as usize];
        let black_after = diff.after[Color::Black as usize];

        if white_before == white_after && black_before == black_after {
            return;
        }

        Self::append_indices_single(
            perspective,
            ksq,
            (white_after & !white_before) | (black_after & !black_before),
            white_after,
            black_after,
            added,
            prefetch_base,
        );

        Self::append_indices_single(
            perspective,
            ksq,
            (white_before & !white_after) | (black_before & !black_after),
            white_before,
            black_before,
            removed,
            prefetch_base,
        );
    }

    #[inline(always)]
    pub fn append_changed_indices_both(
        white_ksq: Square,
        black_ksq: Square,
        diff: &DirtyPawnPairs,
        white_removed: &mut ValueList<u16, 256>,
        white_added: &mut ValueList<u16, 256>,
        black_removed: &mut ValueList<u16, 256>,
        black_added: &mut ValueList<u16, 256>,
        prefetch_base: *const i8,
    ) {
        let white_before = diff.before[Color::White as usize];
        let black_before = diff.before[Color::Black as usize];
        let white_after = diff.after[Color::White as usize];
        let black_after = diff.after[Color::Black as usize];

        if white_before == white_after && black_before == black_after {
            return;
        }

        Self::append_indices_pair(
            white_ksq,
            black_ksq,
            (white_after & !white_before) | (black_after & !black_before),
            white_after,
            black_after,
            white_added,
            black_added,
            prefetch_base,
        );

        Self::append_indices_pair(
            white_ksq,
            black_ksq,
            (white_before & !white_after) | (black_before & !black_after),
            white_before,
            black_before,
            white_removed,
            black_removed,
            prefetch_base,
        );
    }

    fn append_indices_single(
        perspective: Color,
        ksq: Square,
        updated: Bitboard,
        pawns_w: Bitboard,
        pawns_b: Bitboard,
        out: &mut ValueList<u16, 256>,
        prefetch_base: *const i8,
    ) {
        if updated == 0 {
            return;
        }

        let orientation = ORIENT_TBL[ksq as usize] ^ (56 * (perspective as u8));
        let unchanged = (pawns_w | pawns_b) & !updated;

        let mut u = updated;
        while u != 0 {
            let a = u.pop_lsb();
            let partners = pawn_pair_bb(a) & (unchanged | u);
            if partners == 0 {
                continue;
            }

            let a_col = if (pawns_b & square_bb(a)) != 0 { Color::Black } else { Color::White };
            let a_id = make_pawn_id(
                Color::from_usize((a_col as usize) ^ (perspective as usize)),
                Square::from_u8((a as u8) ^ orientation),
            );

            let mut pb = pawns_b & partners;
            while pb != 0 {
                let to = pb.pop_lsb();
                let b_id = make_pawn_id(
                    Color::from_usize((Color::Black as usize) ^ (perspective as usize)),
                    Square::from_u8((to as u8) ^ orientation),
                );
                let idx = pp_idx(a_id, b_id);
                #[cfg(target_arch = "x86_64")]
                if !prefetch_base.is_null() {
                    unsafe {
                        core::arch::x86_64::_mm_prefetch(
                            prefetch_base.add(idx * 1024),
                            core::arch::x86_64::_MM_HINT_T2,
                        );
                    }
                }
                out.push(idx as u16);
            }

            let mut pw = pawns_w & partners;
            while pw != 0 {
                let to = pw.pop_lsb();
                let b_id = make_pawn_id(
                    Color::from_usize((Color::White as usize) ^ (perspective as usize)),
                    Square::from_u8((to as u8) ^ orientation),
                );
                let idx = pp_idx(a_id, b_id);
                #[cfg(target_arch = "x86_64")]
                if !prefetch_base.is_null() {
                    unsafe {
                        core::arch::x86_64::_mm_prefetch(
                            prefetch_base.add(idx * 1024),
                            core::arch::x86_64::_MM_HINT_T2,
                        );
                    }
                }
                out.push(idx as u16);
            }
        }
    }

    fn append_indices_pair(
        white_ksq: Square,
        black_ksq: Square,
        updated: Bitboard,
        pawns_w: Bitboard,
        pawns_b: Bitboard,
        w_out: &mut ValueList<u16, 256>,
        b_out: &mut ValueList<u16, 256>,
        prefetch_base: *const i8,
    ) {
        if updated == 0 {
            return;
        }

        let w_orient = ORIENT_TBL[white_ksq as usize];
        let b_orient = ORIENT_TBL[black_ksq as usize] ^ 56;
        let unchanged = (pawns_w | pawns_b) & !updated;

        let mut u = updated;
        while u != 0 {
            let a = u.pop_lsb();
            let partners = pawn_pair_bb(a) & (unchanged | u);
            if partners == 0 {
                continue;
            }

            let a_col = if (pawns_b & square_bb(a)) != 0 { Color::Black } else { Color::White };
            let w_a_id = make_pawn_id(a_col, Square::from_u8((a as u8) ^ w_orient));
            let b_a_id = make_pawn_id(!a_col, Square::from_u8((a as u8) ^ b_orient));

            let mut pb = pawns_b & partners;
            while pb != 0 {
                let to = pb.pop_lsb();
                let w_b_id = make_pawn_id(Color::Black, Square::from_u8((to as u8) ^ w_orient));
                let b_b_id = make_pawn_id(Color::White, Square::from_u8((to as u8) ^ b_orient));
                let w_idx = pp_idx(w_a_id, w_b_id);
                let b_idx = pp_idx(b_a_id, b_b_id);
                #[cfg(target_arch = "x86_64")]
                if !prefetch_base.is_null() {
                    unsafe {
                        core::arch::x86_64::_mm_prefetch(
                            prefetch_base.add(w_idx * 1024),
                            core::arch::x86_64::_MM_HINT_T2,
                        );
                        core::arch::x86_64::_mm_prefetch(
                            prefetch_base.add(b_idx * 1024),
                            core::arch::x86_64::_MM_HINT_T2,
                        );
                    }
                }
                w_out.push(w_idx as u16);
                b_out.push(b_idx as u16);
            }

            let mut pw = pawns_w & partners;
            while pw != 0 {
                let to = pw.pop_lsb();
                let w_b_id = make_pawn_id(Color::White, Square::from_u8((to as u8) ^ w_orient));
                let b_b_id = make_pawn_id(Color::Black, Square::from_u8((to as u8) ^ b_orient));
                let w_idx = pp_idx(w_a_id, w_b_id);
                let b_idx = pp_idx(b_a_id, b_b_id);
                #[cfg(target_arch = "x86_64")]
                if !prefetch_base.is_null() {
                    unsafe {
                        core::arch::x86_64::_mm_prefetch(
                            prefetch_base.add(w_idx * 1024),
                            core::arch::x86_64::_MM_HINT_T2,
                        );
                        core::arch::x86_64::_mm_prefetch(
                            prefetch_base.add(b_idx * 1024),
                            core::arch::x86_64::_MM_HINT_T2,
                        );
                    }
                }
                w_out.push(w_idx as u16);
                b_out.push(b_idx as u16);
            }
        }
    }
}
