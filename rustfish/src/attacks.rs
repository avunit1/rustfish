use crate::bitboard::*;
use crate::types::*;

#[inline(always)]
pub const fn safe_destination(s: Square, step: i8) -> Bitboard {
    let to_val = (s as i8).wrapping_add(step);
    if to_val >= 0 && to_val < 64 {
        let to = Square::from_u8(to_val as u8);
        let f1 = (s as i8) & 7;
        let f2 = (to as i8) & 7;
        let diff = if f1 > f2 { f1 - f2 } else { f2 - f1 };
        if diff <= 2 {
            square_bb(to)
        } else {
            0
        }
    } else {
        0
    }
}

pub const fn line_mask(sq: Square, d1: Direction, d2: Direction) -> Bitboard {
    let mut mask = 0u64;
    let dirs = [d1, d2];
    let mut i = 0;
    while i < 2 {
        let d = dirs[i];
        let mut s = sq;
        loop {
            let dest = safe_destination(s, d as i8);
            if dest == 0 {
                break;
            }
            mask |= dest;
            s = Square::from_u8(((s as i8) + (d as i8)) as u8);
        }
        i += 1;
    }
    mask
}

pub const fn sliding_attack(pt: PieceType, sq: Square, occupied: Bitboard) -> Bitboard {
    let mut attacks = 0u64;
    let rook_dirs = [
        Direction::North,
        Direction::South,
        Direction::East,
        Direction::West,
    ];
    let bishop_dirs = [
        Direction::NorthEast,
        Direction::SouthEast,
        Direction::SouthWest,
        Direction::NorthWest,
    ];

    let dirs = match pt {
        PieceType::Rook => rook_dirs,
        _ => bishop_dirs,
    };

    let mut i = 0;
    while i < 4 {
        let d = dirs[i];
        let mut s = sq;
        loop {
            let dest = safe_destination(s, d as i8);
            if dest == 0 {
                break;
            }
            attacks |= dest;
            if (occupied & dest) != 0 {
                break;
            }
            s = Square::from_u8(((s as i8) + (d as i8)) as u8);
        }
        i += 1;
    }
    attacks
}

pub const fn knight_attack(sq: Square) -> Bitboard {
    let mut b = 0u64;
    let steps: [i8; 8] = [-17, -15, -10, -6, 6, 10, 15, 17];
    let mut i = 0;
    while i < 8 {
        b |= safe_destination(sq, steps[i]);
        i += 1;
    }
    b
}

pub const fn king_attack(sq: Square) -> Bitboard {
    let mut b = 0u64;
    let steps: [i8; 8] = [-9, -8, -7, -1, 1, 7, 8, 9];
    let mut i = 0;
    while i < 8 {
        b |= safe_destination(sq, steps[i]);
        i += 1;
    }
    b
}

pub const fn pseudo_attacks(pt: PieceType, sq: Square) -> Bitboard {
    match pt {
        PieceType::Rook | PieceType::Bishop => sliding_attack(pt, sq, 0),
        PieceType::Queen => {
            sliding_attack(PieceType::Rook, sq, 0) | sliding_attack(PieceType::Bishop, sq, 0)
        }
        PieceType::Knight => knight_attack(sq),
        PieceType::King => king_attack(sq),
        _ => 0,
    }
}

pub const PSEUDO_ATTACKS: [[Bitboard; SQUARE_NB]; PIECE_TYPE_NB] = {
    let mut attacks = [[0u64; SQUARE_NB]; PIECE_TYPE_NB];
    let mut s1 = 0;
    while s1 < 64 {
        let sq = Square::from_u8(s1 as u8);
        attacks[Color::White as usize][s1] = pawn_attacks_bb(Color::White, square_bb(sq));
        attacks[Color::Black as usize][s1] = pawn_attacks_bb(Color::Black, square_bb(sq));
        attacks[PieceType::Knight as usize][s1] = pseudo_attacks(PieceType::Knight, sq);
        let bishop = pseudo_attacks(PieceType::Bishop, sq);
        let rook = pseudo_attacks(PieceType::Rook, sq);
        attacks[PieceType::Bishop as usize][s1] = bishop;
        attacks[PieceType::Rook as usize][s1] = rook;
        attacks[PieceType::Queen as usize][s1] = bishop | rook;
        attacks[PieceType::King as usize][s1] = pseudo_attacks(PieceType::King, sq);
        s1 += 1;
    }
    attacks
};

pub const RANK_ATTACKS: [[u8; 64]; FILE_NB] = {
    let mut table = [[0u8; 64]; FILE_NB];
    let mut file = 0;
    while file < 8 {
        let mut occ6 = 0;
        while occ6 < 64 {
            let occupied = (occ6 as u64) << 1;
            table[file][occ6] =
                sliding_attack(PieceType::Rook, Square::from_u8(file as u8), occupied) as u8;
            occ6 += 1;
        }
        file += 1;
    }
    table
};

#[repr(C, align(32))]
#[derive(Clone, Copy)]
pub struct DualMagic {
    pub mask_file: Bitboard,
    pub mask_diag: Bitboard,
    pub mask_antidiag: Bitboard,
    pub mask_none: Bitboard,
    pub r: Bitboard,
    pub rr: Bitboard,
    pub shift: i32,
    pub file: u8,
}

pub const DUAL_MAGICS: [DualMagic; SQUARE_NB] = {
    let mut magics = [DualMagic {
        mask_file: 0,
        mask_diag: 0,
        mask_antidiag: 0,
        mask_none: 0,
        r: 0,
        rr: 0,
        shift: 0,
        file: 0,
    }; SQUARE_NB];

    let mut s = 0;
    while s < 64 {
        let sq = Square::from_u8(s as u8);
        magics[s].mask_file = line_mask(sq, Direction::North, Direction::South);
        magics[s].mask_diag = line_mask(sq, Direction::NorthEast, Direction::SouthWest);
        magics[s].mask_none = 0;
        magics[s].mask_antidiag = line_mask(sq, Direction::NorthWest, Direction::SouthEast);
        magics[s].r = square_bb(sq).wrapping_mul(2);
        magics[s].rr = square_bb(Square::from_u8(63 - s as u8)).wrapping_mul(2);
        magics[s].shift = 8 * ((s >> 3) as i32);
        magics[s].file = (s & 7) as u8;
        s += 1;
    }
    magics
};

pub const LINE_BB: [[Bitboard; SQUARE_NB]; SQUARE_NB] = {
    let mut line = [[0u64; SQUARE_NB]; SQUARE_NB];
    let mut s1 = 0;
    while s1 < 64 {
        let sq1 = Square::from_u8(s1 as u8);
        let mut s2 = 0;
        while s2 < 64 {
            let sq2 = Square::from_u8(s2 as u8);
            let b_attacks = sliding_attack(PieceType::Bishop, sq1, 0);
            let r_attacks = sliding_attack(PieceType::Rook, sq1, 0);
            if (b_attacks & square_bb(sq2)) != 0 {
                line[s1][s2] = (sliding_attack(PieceType::Bishop, sq1, 0)
                    & sliding_attack(PieceType::Bishop, sq2, 0))
                    | square_bb(sq1)
                    | square_bb(sq2);
            } else if (r_attacks & square_bb(sq2)) != 0 {
                line[s1][s2] = (sliding_attack(PieceType::Rook, sq1, 0)
                    & sliding_attack(PieceType::Rook, sq2, 0))
                    | square_bb(sq1)
                    | square_bb(sq2);
            }
            s2 += 1;
        }
        s1 += 1;
    }
    line
};

pub const BETWEEN_BB: [[Bitboard; SQUARE_NB]; SQUARE_NB] = {
    let mut between = [[0u64; SQUARE_NB]; SQUARE_NB];
    let mut s1 = 0;
    while s1 < 64 {
        let sq1 = Square::from_u8(s1 as u8);
        let mut s2 = 0;
        while s2 < 64 {
            let sq2 = Square::from_u8(s2 as u8);
            let b_attacks = sliding_attack(PieceType::Bishop, sq1, 0);
            let r_attacks = sliding_attack(PieceType::Rook, sq1, 0);
            if (b_attacks & square_bb(sq2)) != 0 {
                between[s1][s2] = sliding_attack(PieceType::Bishop, sq1, square_bb(sq2))
                    & sliding_attack(PieceType::Bishop, sq2, square_bb(sq1));
            } else if (r_attacks & square_bb(sq2)) != 0 {
                between[s1][s2] = sliding_attack(PieceType::Rook, sq1, square_bb(sq2))
                    & sliding_attack(PieceType::Rook, sq2, square_bb(sq1));
            }
            between[s1][s2] |= square_bb(sq2);
            s2 += 1;
        }
        s1 += 1;
    }
    between
};

pub const INVERSE_RAY_PASS_BB: [[Bitboard; SQUARE_NB]; SQUARE_NB] = {
    let mut inv_ray = [[0u64; SQUARE_NB]; SQUARE_NB];
    let mut s1 = 0;
    while s1 < 64 {
        let sq1 = Square::from_u8(s1 as u8);
        let mut s2 = 0;
        while s2 < 64 {
            let sq2 = Square::from_u8(s2 as u8);
            let b_attacks = sliding_attack(PieceType::Bishop, sq1, 0);
            let r_attacks = sliding_attack(PieceType::Rook, sq1, 0);
            if (b_attacks & square_bb(sq2)) != 0 {
                inv_ray[s2][s1] = sliding_attack(PieceType::Bishop, sq1, 0)
                    & (sliding_attack(PieceType::Bishop, sq2, square_bb(sq1)) | square_bb(sq2));
            } else if (r_attacks & square_bb(sq2)) != 0 {
                inv_ray[s2][s1] = sliding_attack(PieceType::Rook, sq1, 0)
                    & (sliding_attack(PieceType::Rook, sq2, square_bb(sq1)) | square_bb(sq2));
            }
            s2 += 1;
        }
        s1 += 1;
    }
    inv_ray
};

pub const DIAG_BB: [Bitboard; SQUARE_NB] = {
    let mut diag = [0u64; SQUARE_NB];
    let mut s = 0;
    while s < 64 {
        let sq = Square::from_u8(s as u8);
        diag[s] = line_mask(sq, Direction::NorthEast, Direction::SouthWest) | square_bb(sq);
        s += 1;
    }
    diag
};

pub const ANTI_DIAG_BB: [Bitboard; SQUARE_NB] = {
    let mut antidiag = [0u64; SQUARE_NB];
    let mut s = 0;
    while s < 64 {
        let sq = Square::from_u8(s as u8);
        antidiag[s] = line_mask(sq, Direction::NorthWest, Direction::SouthEast) | square_bb(sq);
        s += 1;
    }
    antidiag
};

#[inline(always)]
pub const fn line_bb(s1: Square, s2: Square) -> Bitboard {
    LINE_BB[s1.idx()][s2.idx()]
}

#[inline(always)]
pub const fn between_bb(s1: Square, s2: Square) -> Bitboard {
    BETWEEN_BB[s1.idx()][s2.idx()]
}

#[inline(always)]
pub const fn ray_pass_bb(s1: Square, s2: Square) -> Bitboard {
    INVERSE_RAY_PASS_BB[s2.idx()][s1.idx()]
}

#[inline(always)]
pub const fn diag_bb(s: Square) -> Bitboard {
    DIAG_BB[s.idx()]
}

#[inline(always)]
pub const fn antidiag_bb(s: Square) -> Bitboard {
    ANTI_DIAG_BB[s.idx()]
}

#[inline(always)]
pub const fn attacks_bb_empty(pt: PieceType, s: Square, c: Color) -> Bitboard {
    if (pt as u8) == (PieceType::Pawn as u8) {
        PSEUDO_ATTACKS[c as usize][s.idx()]
    } else {
        PSEUDO_ATTACKS[pt as usize][s.idx()]
    }
}

#[inline(always)]
pub fn both_attacks_bb(s: Square, occupied: Bitboard) -> (Bitboard, Bitboard) {
    #[cfg(target_arch = "x86_64")]
    unsafe {
        use core::arch::x86_64::*;

        let m = DUAL_MAGICS.get_unchecked(s.idx());
        let mask = _mm256_load_si256(m as *const DualMagic as *const __m256i);
        let rs = _mm256_set1_epi64x(m.r as i64);
        let rrs = _mm256_set1_epi64x(m.rr as i64);

        let o = _mm256_and_si256(mask, _mm256_set1_epi64x(occupied as i64));
        let fwd = _mm256_sub_epi64(o, rs);

        let shuf = _mm256_set_epi8(
            0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15,
            0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15,
        );

        let bswap_o = _mm256_shuffle_epi8(o, shuf);
        let rev_sub = _mm256_sub_epi64(bswap_o, rrs);
        let rev = _mm256_shuffle_epi8(rev_sub, shuf);

        let result = _mm256_and_si256(_mm256_xor_si256(fwd, rev), mask);

        let rook_bishop = _mm256_or_si256(
            result,
            _mm256_permute4x64_epi64(result, 0b11_01_10_00), // _MM_SHUFFLE(3, 1, 2, 0)
        );

        let row_occ = *RANK_ATTACKS
            .get_unchecked(m.file as usize)
            .get_unchecked(((occupied >> (m.shift + 1)) & 0x3F) as usize);
        let rank_attacks = (row_occ as u64) << m.shift;

        let bishop = _mm_extract_epi64(_mm256_castsi256_si128(rook_bishop), 1) as u64;
        let rook_file = _mm_cvtsi128_si64(_mm256_castsi256_si128(result)) as u64;

        (bishop, rook_file.wrapping_add(rank_attacks))
    }

    #[cfg(not(target_arch = "x86_64"))]
    {
        (
            sliding_attack(PieceType::Bishop, s, occupied),
            sliding_attack(PieceType::Rook, s, occupied),
        )
    }
}

#[inline(always)]
pub fn bishop_attacks_bb(s: Square, occupied: Bitboard) -> Bitboard {
    both_attacks_bb(s, occupied).0
}

#[inline(always)]
pub fn rook_attacks_bb(s: Square, occupied: Bitboard) -> Bitboard {
    both_attacks_bb(s, occupied).1
}

#[inline(always)]
pub fn attacks_bb(pt: PieceType, s: Square, occupied: Bitboard) -> Bitboard {
    match pt {
        PieceType::Bishop => both_attacks_bb(s, occupied).0,
        PieceType::Rook => both_attacks_bb(s, occupied).1,
        PieceType::Queen => {
            let (b, r) = both_attacks_bb(s, occupied);
            b | r
        }
        _ => PSEUDO_ATTACKS[pt as usize][s.idx()],
    }
}
