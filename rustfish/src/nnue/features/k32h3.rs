use crate::position::Position;
use crate::types::*;
use super::super::common::{IndexType, ORIENT_TBL};

pub const HASH_VALUE: u32 = 0xAB005CEF;
pub const NUM_BLOCKS: usize = 3;
pub const BLOCK_DIMENSIONS: usize = (SQUARE_NB / 2) * (11 * SQUARE_NB); // 32 * 704 = 22528
pub const DIMENSIONS: usize = NUM_BLOCKS * BLOCK_DIMENSIONS; // 67584

const PS_NONE: u16 = 0;
const PS_W_PAWN: u16 = 0;
const PS_B_PAWN: u16 = 1 * SQUARE_NB as u16;
const PS_W_KNIGHT: u16 = 2 * SQUARE_NB as u16;
const PS_B_KNIGHT: u16 = 3 * SQUARE_NB as u16;
const PS_W_BISHOP: u16 = 4 * SQUARE_NB as u16;
const PS_B_BISHOP: u16 = 5 * SQUARE_NB as u16;
const PS_W_ROOK: u16 = 6 * SQUARE_NB as u16;
const PS_B_ROOK: u16 = 7 * SQUARE_NB as u16;
const PS_W_QUEEN: u16 = 8 * SQUARE_NB as u16;
const PS_B_QUEEN: u16 = 9 * SQUARE_NB as u16;
const PS_KING: u16 = 10 * SQUARE_NB as u16;
const PS_NB: u16 = 11 * SQUARE_NB as u16;

const PIECE_SQUARE_INDEX: [[u16; PIECE_NB]; COLOR_NB] = [
    [
        PS_NONE, PS_W_PAWN, PS_W_KNIGHT, PS_W_BISHOP, PS_W_ROOK, PS_W_QUEEN, PS_KING, PS_NONE,
        PS_NONE, PS_B_PAWN, PS_B_KNIGHT, PS_B_BISHOP, PS_B_ROOK, PS_B_QUEEN, PS_KING, PS_NONE,
    ],
    [
        PS_NONE, PS_B_PAWN, PS_B_KNIGHT, PS_B_BISHOP, PS_B_ROOK, PS_B_QUEEN, PS_KING, PS_NONE,
        PS_NONE, PS_W_PAWN, PS_W_KNIGHT, PS_W_BISHOP, PS_W_ROOK, PS_W_QUEEN, PS_KING, PS_NONE,
    ],
];

const fn b(v: u16) -> u16 {
    v * PS_NB
}

#[rustfmt::skip]
const KING_BUCKETS: [u16; SQUARE_NB] = [
    b(28), b(29), b(30), b(31), b(31), b(30), b(29), b(28),
    b(24), b(25), b(26), b(27), b(27), b(26), b(25), b(24),
    b(20), b(21), b(22), b(23), b(23), b(22), b(21), b(20),
    b(16), b(17), b(18), b(19), b(19), b(18), b(17), b(16),
    b(12), b(13), b(14), b(15), b(15), b(14), b(13), b(12),
    b( 8), b( 9), b(10), b(11), b(11), b(10), b( 9), b( 8),
    b( 4), b( 5), b( 6), b( 7), b( 7), b( 6), b( 5), b( 4),
    b( 0), b( 1), b( 2), b( 3), b( 3), b( 2), b( 1), b( 0),
];

const OFFSETS: [[u16; PIECE_NB]; COLOR_NB * SQUARE_NB] = {
    let mut table = [[0u16; PIECE_NB]; COLOR_NB * SQUARE_NB];
    let mut c = 0;
    while c < COLOR_NB {
        let mut sq = 0;
        while sq < SQUARE_NB {
            let flip = (56 * c) as u16;
            let orient = (ORIENT_TBL[sq] as u16) ^ flip;
            let king_bucket = KING_BUCKETS[sq ^ (flip as usize)];
            let mut piece_index = 0;
            while piece_index < PIECE_NB {
                table[c * SQUARE_NB + sq][piece_index] =
                    PIECE_SQUARE_INDEX[c][piece_index] + king_bucket + orient;
                piece_index += 1;
            }
            sq += 1;
        }
        c += 1;
    }
    table
};

pub struct K32H3;

impl K32H3 {
    pub const NUM_BLOCKS: usize = NUM_BLOCKS;
    pub const DIMENSIONS: usize = DIMENSIONS;

    #[inline(always)]
    pub const fn block_offset(block: usize) -> IndexType {
        block * BLOCK_DIMENSIONS
    }

    #[inline(always)]
    pub fn block(pos: &Position, perspective: Color) -> usize {
        let them = !perspective;
        if pos.count_color(them, PieceType::Queen) > 0 {
            2
        } else if pos.pieces_2(PieceType::Rook, PieceType::Queen) != 0 {
            1
        } else {
            0
        }
    }

    #[inline(always)]
    pub fn block_state(pos: &Position) -> [u8; COLOR_NB] {
        [
            Self::block(pos, Color::White) as u8,
            Self::block(pos, Color::Black) as u8,
        ]
    }

    #[inline(always)]
    pub fn make_index(perspective: Color, s: Square, pc: Piece, ksq: Square) -> IndexType {
        let idx = (perspective as usize) * SQUARE_NB + ksq.idx();
        s.idx() ^ unsafe { *OFFSETS.get_unchecked(idx).get_unchecked(pc as usize) as usize }
    }

    #[inline(always)]
    pub fn append_changed_indices(
        perspective: Color,
        ksq: Square,
        diff: &DirtyPiece,
        removed: &mut ValueList<u16, 32>,
        added: &mut ValueList<u16, 32>,
    ) {
        removed.push(Self::make_index(perspective, diff.from, diff.pc, ksq) as u16);
        if diff.to != Square::SqNone {
            added.push(Self::make_index(perspective, diff.to, diff.pc, ksq) as u16);
        }
        if diff.remove_sq != Square::SqNone {
            removed.push(Self::make_index(perspective, diff.remove_sq, diff.remove_pc, ksq) as u16);
        }
        if diff.add_sq != Square::SqNone {
            added.push(Self::make_index(perspective, diff.add_sq, diff.add_pc, ksq) as u16);
        }
    }

    #[inline(always)]
    pub fn requires_refresh(
        diff: &DirtyPiece,
        perspective: Color,
        block_before: usize,
        block_after: usize,
    ) -> bool {
        diff.pc == Piece::make(perspective, PieceType::King) || block_before != block_after
    }
}
