use crate::types::*;

pub const FILE_A_BB: Bitboard = 0x0101010101010101;
pub const FILE_B_BB: Bitboard = FILE_A_BB << 1;
pub const FILE_C_BB: Bitboard = FILE_A_BB << 2;
pub const FILE_D_BB: Bitboard = FILE_A_BB << 3;
pub const FILE_E_BB: Bitboard = FILE_A_BB << 4;
pub const FILE_F_BB: Bitboard = FILE_A_BB << 5;
pub const FILE_G_BB: Bitboard = FILE_A_BB << 6;
pub const FILE_H_BB: Bitboard = FILE_A_BB << 7;

pub const RANK_1_BB: Bitboard = 0xFF;
pub const RANK_2_BB: Bitboard = RANK_1_BB << (8 * 1);
pub const RANK_3_BB: Bitboard = RANK_1_BB << (8 * 2);
pub const RANK_4_BB: Bitboard = RANK_1_BB << (8 * 3);
pub const RANK_5_BB: Bitboard = RANK_1_BB << (8 * 4);
pub const RANK_6_BB: Bitboard = RANK_1_BB << (8 * 5);
pub const RANK_7_BB: Bitboard = RANK_1_BB << (8 * 6);
pub const RANK_8_BB: Bitboard = RANK_1_BB << (8 * 7);

#[inline(always)]
pub const fn square_bb(s: Square) -> Bitboard {
    1u64 << (s.idx())
}

#[inline(always)]
pub const fn rank_bb(r: Rank) -> Bitboard {
    RANK_1_BB << (8 * (r as usize))
}

#[inline(always)]
pub const fn rank_bb_of_sq(s: Square) -> Bitboard {
    rank_bb(rank_of(s))
}

#[inline(always)]
pub const fn file_bb(f: File) -> Bitboard {
    FILE_A_BB << (f as usize)
}

#[inline(always)]
pub const fn file_bb_of_sq(s: Square) -> Bitboard {
    file_bb(file_of(s))
}

#[inline(always)]
pub const fn shift(b: Bitboard, dir: Direction) -> Bitboard {
    match dir {
        Direction::North => b << 8,
        Direction::South => b >> 8,
        Direction::East => (b & !FILE_H_BB) << 1,
        Direction::West => (b & !FILE_A_BB) >> 1,
        Direction::NorthEast => (b & !FILE_H_BB) << 9,
        Direction::NorthWest => (b & !FILE_A_BB) << 7,
        Direction::SouthEast => (b & !FILE_H_BB) >> 7,
        Direction::SouthWest => (b & !FILE_A_BB) >> 9,
    }
}

#[inline(always)]
pub const fn shift_double_push(b: Bitboard, c: Color) -> Bitboard {
    match c {
        Color::White => b << 16,
        Color::Black => b >> 16,
    }
}

#[inline(always)]
pub const fn pawn_attacks_bb(c: Color, b: Bitboard) -> Bitboard {
    match c {
        Color::White => shift(b, Direction::NorthWest) | shift(b, Direction::NorthEast),
        Color::Black => shift(b, Direction::SouthWest) | shift(b, Direction::SouthEast),
    }
}

#[inline(always)]
pub const fn pawn_single_push_bb(c: Color, b: Bitboard) -> Bitboard {
    match c {
        Color::White => b << 8,
        Color::Black => b >> 8,
    }
}

const PAWN_PAIR_BB: [Bitboard; SQUARE_NB] = {
    let mut result = [0u64; SQUARE_NB];
    let mut sq = 0;
    while sq < 64 {
        let f = sq & 7;
        let file = FILE_A_BB << f;
        let left = (file & !FILE_A_BB) >> 1;
        let right = (file & !FILE_H_BB) << 1;
        let files = file | left | right;
        result[sq] = files & !(RANK_1_BB | RANK_8_BB) & !(1u64 << sq);
        sq += 1;
    }
    result
};

#[inline(always)]
pub const fn pawn_pair_bb(s: Square) -> Bitboard {
    PAWN_PAIR_BB[s.idx()]
}

#[inline(always)]
pub const fn popcount(b: Bitboard) -> u32 {
    b.count_ones()
}

#[inline(always)]
pub const fn lsb(b: Bitboard) -> Square {
    debug_assert!(b != 0);
    unsafe { Square::from_u8_unchecked(b.trailing_zeros() as u8) }
}

#[inline(always)]
pub const fn msb(b: Bitboard) -> Square {
    debug_assert!(b != 0);
    unsafe { Square::from_u8_unchecked(63 ^ (b.leading_zeros() as u8)) }
}

#[inline(always)]
pub fn pop_lsb(b: &mut Bitboard) -> Square {
    debug_assert!(*b != 0);
    let s = lsb(*b);
    *b &= b.wrapping_sub(1);
    s
}

pub trait BitboardExt {
    fn pop_lsb(&mut self) -> Square;
    fn lsb(self) -> Square;
    fn msb(self) -> Square;
}

impl BitboardExt for Bitboard {
    #[inline(always)]
    fn pop_lsb(&mut self) -> Square {
        pop_lsb(self)
    }

    #[inline(always)]
    fn lsb(self) -> Square {
        lsb(self)
    }

    #[inline(always)]
    fn msb(self) -> Square {
        msb(self)
    }
}

#[inline(always)]
pub const fn least_significant_square_bb(b: Bitboard) -> Bitboard {
    debug_assert!(b != 0);
    b & b.wrapping_neg()
}

#[inline(always)]
pub const fn more_than_one(b: Bitboard) -> bool {
    (b & b.wrapping_sub(1)) != 0
}

#[inline(always)]
pub const fn edge_distance(f: File) -> usize {
    let f_val = f as usize;
    if f_val < (7 - f_val) {
        f_val
    } else {
        7 - f_val
    }
}
