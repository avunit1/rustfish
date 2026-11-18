use crate::attacks::*;
use crate::bitboard::*;
use crate::position::Position;
use crate::types::*;
use super::super::common::{IndexType, ORIENT_TBL};

pub const HASH_VALUE: u32 = 0x2C41EAEA;
pub const DIMENSIONS: usize = 59808;

// Pawn diagonal threats only target knights and rooks: 4 valid targets.
const NUM_VALID_TARGETS: [usize; PIECE_NB] = [
    0, 4, 10, 8, 8, 10, 0, 0,
    0, 4, 10, 8, 8, 10, 0, 0,
];

#[rustfmt::skip]
const MAP: [[i32; 6]; 6] = [
    [-1,  0, -1,  1, -1, -1],
    [ 0,  1,  2,  3,  4, -1],
    [ 0,  1,  2,  3, -1, -1],
    [ 0,  1,  2,  3, -1, -1],
    [ 0,  1,  2,  3,  4, -1],
    [-1, -1, -1, -1, -1, -1],
];

const ALL_PIECES_LIST: [Piece; 12] = [
    Piece::WPawn, Piece::WKnight, Piece::WBishop, Piece::WRook, Piece::WQueen, Piece::WKing,
    Piece::BPawn, Piece::BKnight, Piece::BBishop, Piece::BRook, Piece::BQueen, Piece::BKing,
];

#[repr(C, align(64))]
pub struct ThreatLUTs {
    // [attacker][attacked][from < to]
    pub lut1: [[[u32; 2]; PIECE_NB]; PIECE_NB],
    // [pt][from][to]
    pub lut2: [[[u16; SQUARE_NB]; SQUARE_NB]; PIECE_TYPE_NB],
}

pub static THREAT_LUTS: ThreatLUTs = {
    let mut tables = ThreatLUTs {
        lut1: [[[0; 2]; PIECE_NB]; PIECE_NB],
        lut2: [[[0; SQUARE_NB]; SQUARE_NB]; PIECE_TYPE_NB],
    };

    let mut offsets = [0u16; PIECE_TYPE_NB];

    // Compute pseudo attacks and offsets
    let pts = [
        PieceType::Pawn,
        PieceType::Knight,
        PieceType::Bishop,
        PieceType::Rook,
        PieceType::Queen,
        PieceType::King,
    ];

    let mut pt_idx = 0;
    while pt_idx < pts.len() {
        let pt = pts[pt_idx];
        let mut offset = 0u16;
        let mut from = 0;
        while from < SQUARE_NB {
            let mut to = 0;
            while to < SQUARE_NB {
                let attacks = if (pt as u8) != (PieceType::Pawn as u8) {
                    PSEUDO_ATTACKS_DATA[pt as usize][from]
                } else if to < from {
                    PSEUDO_ATTACKS_DATA[Color::Black as usize][from]
                } else {
                    PSEUDO_ATTACKS_DATA[Color::White as usize][from]
                };
                let mask = (1u64 << to).wrapping_sub(1) & attacks;
                tables.lut2[pt as usize][from][to] = offset + mask.count_ones() as u16;
                to += 1;
            }

            if (pt as u8) != (PieceType::Pawn as u8) {
                offset += PSEUDO_ATTACKS_DATA[pt as usize][from].count_ones() as u16;
            } else if from >= Square::SqA2 as usize && from <= Square::SqH7 as usize {
                offset += PSEUDO_ATTACKS_DATA[Color::White as usize][from].count_ones() as u16;
            }
            from += 1;
        }
        offsets[pt as usize] = offset;
        pt_idx += 1;
    }

    let mut cumulative_offset = 0u32;
    let mut attkr_idx = 0;
    while attkr_idx < ALL_PIECES_LIST.len() {
        let attacker = ALL_PIECES_LIST[attkr_idx];
        let attacker_type = attacker.type_of();
        let cumulative_piece_offset = offsets[attacker_type as usize] as u32;
        let valid_targets = NUM_VALID_TARGETS[attacker as usize] as u32;

        let mut attkd_idx = 0;
        while attkd_idx < ALL_PIECES_LIST.len() {
            let attacked = ALL_PIECES_LIST[attkd_idx];
            let enemy = ((attacker as usize) ^ (attacked as usize)) == 8;
            let attacked_type = attacked.type_of();

            let map_val = MAP[attacker_type as usize - 1][attacked_type as usize - 1];
            let semi_excluded = (attacker_type as u8) == (attacked_type as u8)
                && (enemy || (attacker_type as u8) != (PieceType::Pawn as u8));
            let excluded = map_val < 0;
            let feature = if excluded {
                0
            } else {
                cumulative_offset
                    + ((attacked.color_of() as usize as u32) * (valid_targets / 2) + map_val as u32)
                        * cumulative_piece_offset
            };

            tables.lut1[attacker as usize][attacked as usize][0] =
                if excluded { DIMENSIONS as u32 } else { feature };
            tables.lut1[attacker as usize][attacked as usize][1] =
                if excluded || semi_excluded { DIMENSIONS as u32 } else { feature };

            attkd_idx += 1;
        }

        cumulative_offset += valid_targets * cumulative_piece_offset;
        attkr_idx += 1;
    }

    tables
};

pub struct FullThreats;

impl FullThreats {
    pub const HASH_VALUE: u32 = HASH_VALUE;
    pub const DIMENSIONS: usize = DIMENSIONS;

    #[inline(always)]
    pub fn make_index(
        perspective: Color,
        attacker: Piece,
        from: Square,
        to: Square,
        attacked: Piece,
        ksq: Square,
    ) -> IndexType {
        let orientation = ORIENT_TBL[ksq.idx()] ^ (56 * (perspective as u8));
        let from_oriented = (from as u8 ^ orientation) as usize;
        let to_oriented = (to as u8 ^ orientation) as usize;

        let swap = 8 * (perspective as usize);
        let attacker_oriented = (attacker as usize) ^ swap;
        let attacked_oriented = (attacked as usize) ^ swap;

        let lt = (from_oriented < to_oriented) as usize;
        unsafe {
            let base = *THREAT_LUTS
                .lut1
                .get_unchecked(attacker_oriented)
                .get_unchecked(attacked_oriented)
                .get_unchecked(lt) as usize;
            let offset = *THREAT_LUTS
                .lut2
                .get_unchecked(attacker.type_of() as usize)
                .get_unchecked(from_oriented)
                .get_unchecked(to_oriented) as usize;
            base + offset
        }
    }

    pub fn append_active_indices(perspective: Color, pos: &Position, active: &mut ValueList<u16, 256>) {
        let ksq = pos.square_color(PieceType::King, perspective);
        let occupied = pos.pieces();
        let pawn_targets = pos.pieces_2(PieceType::Knight, PieceType::Rook);
        let minor_slider_targets = pos.pieces_4(PieceType::Pawn, PieceType::Knight, PieceType::Bishop, PieceType::Rook);
        let queen_targets = pos.pieces_5(PieceType::Pawn, PieceType::Knight, PieceType::Bishop, PieceType::Rook, PieceType::Queen);

        let mut process_pawn_attacks = |c: Color, attk_dir: Direction| {
            let c_pawns = pos.pieces_color(PieceType::Pawn, c);
            let mut attacks = shift(c_pawns, attk_dir) & pawn_targets;

            while attacks != 0 {
                let to = attacks.pop_lsb();
                let from = Square::from_usize(((to as i8) - (attk_dir as i8)) as usize);
                let attacked = pos.piece_on(to);
                let attacker = Piece::make(c, PieceType::Pawn);
                let index = Self::make_index(perspective, attacker, from, to, attacked, ksq);
                if index < DIMENSIONS {
                    active.push(index as u16);
                }
            }
        };

        // White pawn attacks: NORTH_EAST (+9), NORTH_WEST (+7)
        process_pawn_attacks(Color::White, Direction::NorthEast);
        process_pawn_attacks(Color::White, Direction::NorthWest);
        // Black pawn attacks: SOUTH_WEST (-9), SOUTH_EAST (-7)
        process_pawn_attacks(Color::Black, Direction::SouthWest);
        process_pawn_attacks(Color::Black, Direction::SouthEast);

        for c in [Color::White, Color::Black] {
            let pts = [PieceType::Knight, PieceType::Bishop, PieceType::Rook, PieceType::Queen];
            for pt in pts {
                let attacker = Piece::make(c, pt);
                let mut bb = pos.pieces_color(pt, c);
                let targets = if pt == PieceType::Knight || pt == PieceType::Queen {
                    queen_targets
                } else {
                    minor_slider_targets
                };

                while bb != 0 {
                    let from = bb.pop_lsb();
                    let mut attacks = attacks_bb(pt, from, occupied) & targets;
                    while attacks != 0 {
                        let to = attacks.pop_lsb();
                        let attacked = pos.piece_on(to);
                        let index = Self::make_index(perspective, attacker, from, to, attacked, ksq);
                        if index < DIMENSIONS {
                            active.push(index as u16);
                        }
                    }
                }
            }
        }
    }

    #[inline(always)]
    pub fn append_changed_indices(
        perspective: Color,
        ksq: Square,
        diff: &DirtyThreats,
        removed: &mut ValueList<u16, 256>,
        added: &mut ValueList<u16, 256>,
        prefetch_base: *const i8,
    ) {
        let orient = ORIENT_TBL[ksq.idx()] ^ (56 * (perspective as u8));
        let pair_swap = 0x88 * (perspective as usize);
        let len = diff.list.len();

        unsafe {
            for i in 0..len {
                let raw = diff.list.as_slice().get_unchecked(i).raw();
                let from = ((raw as u8) ^ orient) as usize;
                let to = (((raw >> 8) as u8) ^ orient) as usize;
                let pair = (raw >> 16) as u8 as usize;
                let pt = (pair >> 4) & 7;

                let lt = (from < to) as usize;
                let p_swapped = pair ^ pair_swap;
                let attacker = p_swapped >> 4;
                let attacked = p_swapped & 0x0F;
                let base = *THREAT_LUTS.lut1.get_unchecked(attacker).get_unchecked(attacked).get_unchecked(lt) as usize;
                let offset = *THREAT_LUTS.lut2.get_unchecked(pt).get_unchecked(from).get_unchecked(to) as usize;
                let index = base + offset;

                #[cfg(target_arch = "x86_64")]
                if !prefetch_base.is_null() {
                    core::arch::x86_64::_mm_prefetch(
                        prefetch_base.add(index * 1024),
                        core::arch::x86_64::_MM_HINT_T2,
                    );
                }

                if (raw >> 31) != 0 {
                    added.push_if_lt(index, DIMENSIONS);
                } else {
                    removed.push_if_lt(index, DIMENSIONS);
                }
            }
        }
    }

    #[inline(always)]
    pub fn append_changed_indices_both(
        white_ksq: Square,
        black_ksq: Square,
        diff: &DirtyThreats,
        white_removed: &mut ValueList<u16, 256>,
        white_added: &mut ValueList<u16, 256>,
        black_removed: &mut ValueList<u16, 256>,
        black_added: &mut ValueList<u16, 256>,
        prefetch_base: *const i8,
    ) {
        let w_orient = ORIENT_TBL[white_ksq.idx()];
        let b_orient = ORIENT_TBL[black_ksq.idx()] ^ 56;
        let len = diff.list.len();

        unsafe {
            for i in 0..len {
                let raw = diff.list.as_slice().get_unchecked(i).raw();
                let from = raw as u8;
                let to = (raw >> 8) as u8;
                let pair = (raw >> 16) as u8 as usize;
                let pt = (pair >> 4) & 7;

                let w_from = (from ^ w_orient) as usize;
                let w_to = (to ^ w_orient) as usize;
                let w_lt = (w_from < w_to) as usize;
                let w_attkr = pair >> 4;
                let w_attkd = pair & 0x0F;
                let w_index = (*THREAT_LUTS.lut1.get_unchecked(w_attkr).get_unchecked(w_attkd).get_unchecked(w_lt) as usize)
                    + (*THREAT_LUTS.lut2.get_unchecked(pt).get_unchecked(w_from).get_unchecked(w_to) as usize);

                let b_from = (from ^ b_orient) as usize;
                let b_to = (to ^ b_orient) as usize;
                let b_lt = (b_from < b_to) as usize;
                let b_pair = pair ^ 0x88;
                let b_attkr = b_pair >> 4;
                let b_attkd = b_pair & 0x0F;
                let b_index = (*THREAT_LUTS.lut1.get_unchecked(b_attkr).get_unchecked(b_attkd).get_unchecked(b_lt) as usize)
                    + (*THREAT_LUTS.lut2.get_unchecked(pt).get_unchecked(b_from).get_unchecked(b_to) as usize);

                #[cfg(target_arch = "x86_64")]
                if !prefetch_base.is_null() {
                    core::arch::x86_64::_mm_prefetch(
                        prefetch_base.add(w_index * 1024),
                        core::arch::x86_64::_MM_HINT_T2,
                    );
                    core::arch::x86_64::_mm_prefetch(
                        prefetch_base.add(b_index * 1024),
                        core::arch::x86_64::_MM_HINT_T2,
                    );
                }

                if (raw >> 31) != 0 {
                    white_added.push_if_lt(w_index, DIMENSIONS);
                    black_added.push_if_lt(b_index, DIMENSIONS);
                } else {
                    white_removed.push_if_lt(w_index, DIMENSIONS);
                    black_removed.push_if_lt(b_index, DIMENSIONS);
                }
            }
        }
    }
}
