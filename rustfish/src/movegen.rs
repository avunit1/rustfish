use core::mem::MaybeUninit;
use crate::attacks::*;
use crate::bitboard::*;
use crate::position::*;
use crate::types::*;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GenType {
    Captures = 0,
    Quiets = 1,
    Evasions = 2,
    All = 3,
}

pub const GEN_CAPTURES: u8 = 0;
pub const GEN_QUIETS: u8 = 1;
pub const GEN_EVASIONS: u8 = 2;

const US_WHITE: u8 = 0;
const US_BLACK: u8 = 1;

#[inline(always)]
unsafe fn push_move(move_list: &mut [MaybeUninit<Move>], cur: &mut usize, m: Move) {
    move_list.get_unchecked_mut(*cur).write(m);
    *cur += 1;
}

#[inline(always)]
fn splat_pawn_moves(
    move_list: &mut [MaybeUninit<Move>],
    mut cur: usize,
    mut to_bb: Bitboard,
    offset: Direction,
) -> usize {
    let rev = match offset {
        Direction::North => Direction::South,
        Direction::South => Direction::North,
        Direction::NorthEast => Direction::SouthWest,
        Direction::SouthEast => Direction::NorthWest,
        Direction::SouthWest => Direction::NorthEast,
        Direction::NorthWest => Direction::SouthEast,
        _ => Direction::South,
    };
    while to_bb != 0 {
        let to = pop_lsb(&mut to_bb);
        let from = unsafe { to.add_dir_unchecked(rev) };
        unsafe { push_move(move_list, &mut cur, Move::new(from, to)) };
    }
    cur
}

#[inline(always)]
fn splat_pawn_moves_double<const US: u8>(
    move_list: &mut [MaybeUninit<Move>],
    mut cur: usize,
    mut to_bb: Bitboard,
) -> usize {
    while to_bb != 0 {
        let to = pop_lsb(&mut to_bb);
        let from = unsafe {
            if US == US_WHITE {
                Square::from_u8_unchecked((to as u8).wrapping_sub(16))
            } else {
                Square::from_u8_unchecked((to as u8).wrapping_add(16))
            }
        };
        unsafe { push_move(move_list, &mut cur, Move::new(from, to)) };
    }
    cur
}

#[inline(always)]
fn splat_moves(
    move_list: &mut [MaybeUninit<Move>],
    mut cur: usize,
    from: Square,
    mut to_bb: Bitboard,
) -> usize {
    while to_bb != 0 {
        let to = pop_lsb(&mut to_bb);
        unsafe { push_move(move_list, &mut cur, Move::new(from, to)) };
    }
    cur
}

#[inline(always)]
fn make_promotions<const GEN: u8, const ENEMY: bool>(
    move_list: &mut [MaybeUninit<Move>],
    mut cur: usize,
    to: Square,
    d: Direction,
) -> usize {
    let all = GEN == GEN_EVASIONS;
    let from = unsafe {
        to.add_dir_unchecked(match d {
            Direction::North => Direction::South,
            Direction::South => Direction::North,
            Direction::NorthEast => Direction::SouthWest,
            Direction::SouthEast => Direction::NorthWest,
            Direction::SouthWest => Direction::NorthEast,
            Direction::NorthWest => Direction::SouthEast,
            _ => Direction::South,
        })
    };

    if GEN == GEN_CAPTURES || all {
        unsafe {
            push_move(
                move_list,
                &mut cur,
                Move::make(from, to, MoveType::Promotion, PieceType::Queen),
            );
        }
    }

    if (GEN == GEN_CAPTURES && ENEMY) || (GEN == GEN_QUIETS && !ENEMY) || all {
        unsafe {
            push_move(
                move_list,
                &mut cur,
                Move::make(from, to, MoveType::Promotion, PieceType::Rook),
            );
            push_move(
                move_list,
                &mut cur,
                Move::make(from, to, MoveType::Promotion, PieceType::Bishop),
            );
            push_move(
                move_list,
                &mut cur,
                Move::make(from, to, MoveType::Promotion, PieceType::Knight),
            );
        }
    }

    cur
}

#[inline(always)]
fn generate_pawn_moves<const US: u8, const GEN: u8>(
    pos: &Position,
    move_list: &mut [MaybeUninit<Move>],
    mut cur: usize,
    target: Bitboard,
) -> usize {
    let us = if US == US_WHITE { Color::White } else { Color::Black };
    let them = !us;
    let trank7_bb = if US == US_WHITE { RANK_7_BB } else { RANK_2_BB };
    let trank3_bb = if US == US_WHITE { RANK_3_BB } else { RANK_6_BB };
    let up = if US == US_WHITE { Direction::North } else { Direction::South };
    let up_right = if US == US_WHITE { Direction::NorthEast } else { Direction::SouthWest };
    let up_left = if US == US_WHITE { Direction::NorthWest } else { Direction::SouthEast };

    let empty_squares = !pos.pieces();
    let enemies = if GEN == GEN_EVASIONS {
        pos.checkers()
    } else {
        pos.pieces_c(them)
    };
    let pinned = pos.blockers_for_king(us);
    let ksq = pos.square(us, PieceType::King);

    let pawns_on_7 = pos.pieces_c_pt(us, PieceType::Pawn) & trank7_bb;
    let pawns_not_on_7 = pos.pieces_c_pt(us, PieceType::Pawn) & !trank7_bb;
    let pushable = !pinned | file_bb_of_sq(ksq);
    let can_capture_left = !pinned | antidiag_bb(ksq);
    let can_capture_right = !pinned | diag_bb(ksq);

    // Single and double pawn pushes
    if GEN != GEN_CAPTURES {
        let mut b1 = shift(pawns_not_on_7 & pushable, up) & empty_squares;
        let mut b2 = shift(b1 & trank3_bb, up) & empty_squares;

        if GEN == GEN_EVASIONS {
            b1 &= target;
            b2 &= target;
        }

        cur = splat_pawn_moves(move_list, cur, b1, up);
        cur = splat_pawn_moves_double::<US>(move_list, cur, b2);
    }

    // Promotions and underpromotions
    if pawns_on_7 != 0 {
        let mut b1 = shift(pawns_on_7 & can_capture_right, up_right) & enemies;
        let mut b2 = shift(pawns_on_7 & can_capture_left, up_left) & enemies;
        let mut b3 = shift(pawns_on_7 & pushable, up) & empty_squares;

        if GEN == GEN_EVASIONS {
            b3 &= target;
        }

        while b1 != 0 {
            let to = pop_lsb(&mut b1);
            cur = make_promotions::<GEN, true>(move_list, cur, to, up_right);
        }

        while b2 != 0 {
            let to = pop_lsb(&mut b2);
            cur = make_promotions::<GEN, true>(move_list, cur, to, up_left);
        }

        while b3 != 0 {
            let to = pop_lsb(&mut b3);
            cur = make_promotions::<GEN, false>(move_list, cur, to, up);
        }
    }

    // Standard and en passant captures
    if GEN == GEN_CAPTURES || GEN == GEN_EVASIONS {
        let b1 = shift(pawns_not_on_7 & can_capture_right, up_right) & enemies;
        let b2 = shift(pawns_not_on_7 & can_capture_left, up_left) & enemies;

        cur = splat_pawn_moves(move_list, cur, b1, up_right);
        cur = splat_pawn_moves(move_list, cur, b2, up_left);

        let ep_sq = pos.ep_square();
        if ep_sq != Square::SqNone {
            let target_blocks_ep =
                GEN == GEN_EVASIONS && (target & square_bb(ep_sq.add_dir(up))) != 0;
            if !target_blocks_ep {
                let down_right = if US == US_WHITE { Direction::SouthWest } else { Direction::NorthEast };
                let down_left = if US == US_WHITE { Direction::SouthEast } else { Direction::NorthWest };

                let mut ep_pawns = pawns_not_on_7
                    & ((can_capture_right & shift(square_bb(ep_sq), down_right))
                        | (can_capture_left & shift(square_bb(ep_sq), down_left)));

                while ep_pawns != 0 {
                    let from = pop_lsb(&mut ep_pawns);
                    unsafe {
                        push_move(
                            move_list,
                            &mut cur,
                            Move::make(from, ep_sq, MoveType::EnPassant, PieceType::Knight),
                        );
                    }
                }
            }
        }
    }

    cur
}

#[inline(always)]
fn generate_moves_pt<const US: u8, const PT: u8>(
    pos: &Position,
    move_list: &mut [MaybeUninit<Move>],
    mut cur: usize,
    target: Bitboard,
) -> usize {
    let us = if US == US_WHITE { Color::White } else { Color::Black };
    let pt = match PT {
        2 => PieceType::Knight,
        3 => PieceType::Bishop,
        4 => PieceType::Rook,
        _ => PieceType::Queen,
    };
    let ksq = pos.square(us, PieceType::King);
    let pinned = pos.blockers_for_king(us);
    let occ = pos.pieces();

    let mut bb = pos.pieces_c_pt(us, pt) & !pinned;
    while bb != 0 {
        let from = pop_lsb(&mut bb);
        let b = match PT {
            2 => unsafe { *PSEUDO_ATTACKS[PieceType::Knight as usize].get_unchecked(from.idx()) },
            3 => bishop_attacks_bb(from, occ),
            4 => rook_attacks_bb(from, occ),
            _ => bishop_attacks_bb(from, occ) | rook_attacks_bb(from, occ),
        } & target;
        cur = splat_moves(move_list, cur, from, b);
    }

    if PT != 2 {
        let mut bb = pos.pieces_c_pt(us, pt) & pinned;
        while bb != 0 {
            let from = pop_lsb(&mut bb);
            let b = (match PT {
                3 => bishop_attacks_bb(from, occ),
                4 => rook_attacks_bb(from, occ),
                _ => bishop_attacks_bb(from, occ) | rook_attacks_bb(from, occ),
            }) & target
                & line_bb(ksq, from);
            cur = splat_moves(move_list, cur, from, b);
        }
    }

    cur
}

#[inline(always)]
fn generate_moves_typed<const US: u8, const GEN: u8>(
    pos: &Position,
    move_list: &mut [MaybeUninit<Move>],
    mut cur: usize,
) -> usize {
    let us = if US == US_WHITE { Color::White } else { Color::Black };
    let ksq = pos.square(us, PieceType::King);
    let checkers = pos.checkers();

    if GEN != GEN_EVASIONS || !more_than_one(checkers) {
        let target = if GEN == GEN_EVASIONS {
            between_bb(ksq, lsb(checkers))
        } else if GEN == GEN_CAPTURES {
            pos.pieces_c(!us)
        } else {
            !pos.pieces()
        };

        cur = generate_pawn_moves::<US, GEN>(pos, move_list, cur, target);
        cur = generate_moves_pt::<US, 2>(pos, move_list, cur, target);
        cur = generate_moves_pt::<US, 3>(pos, move_list, cur, target);
        cur = generate_moves_pt::<US, 4>(pos, move_list, cur, target);
        cur = generate_moves_pt::<US, 5>(pos, move_list, cur, target);
    }

    let target = if GEN == GEN_EVASIONS {
        !pos.pieces_c(us)
    } else if GEN == GEN_CAPTURES {
        pos.pieces_c(!us)
    } else {
        !pos.pieces()
    };

    let b = unsafe { *PSEUDO_ATTACKS[PieceType::King as usize].get_unchecked(ksq.idx()) } & target;
    if b != 0 {
        let threats = pos.threats_by(PieceType::AllPieces);
        cur = splat_moves(move_list, cur, ksq, b & !threats);
    }

    if GEN == GEN_QUIETS {
        let can_castle_any = if US == US_WHITE {
            pos.can_castle(CastlingRights::WhiteCastling)
        } else {
            pos.can_castle(CastlingRights::BlackCastling)
        };

        if can_castle_any {
            let rights = if US == US_WHITE {
                [CastlingRights::WhiteOO, CastlingRights::WhiteOOO]
            } else {
                [CastlingRights::BlackOO, CastlingRights::BlackOOO]
            };

            for cr in rights {
                if !pos.can_castle(cr) || pos.castling_impeded(cr) {
                    continue;
                }

                let rook_sq = pos.castling_rook_square(cr);
                let king_side = (cr == CastlingRights::WhiteOO) || (cr == CastlingRights::BlackOO);
                let to = relative_square(us, if king_side { Square::SqG1 } else { Square::SqC1 });

                let threats = pos.threats_by(PieceType::AllPieces);
                if (between_bb(ksq, to) & threats) == 0
                    && (pos.blockers_for_king(us) & square_bb(rook_sq)) == 0
                {
                    unsafe {
                        push_move(
                            move_list,
                            &mut cur,
                            Move::make(ksq, rook_sq, MoveType::Castling, PieceType::Knight),
                        );
                    }
                }
            }
        }
    }

    cur
}

#[inline(always)]
pub fn generate_moves_for<const GEN: u8>(
    pos: &Position,
    move_list: &mut [MaybeUninit<Move>],
    cur: usize,
) -> usize {
    debug_assert!(move_list.len() >= MAX_MOVES);
    if pos.side_to_move() == Color::White {
        generate_moves_typed::<US_WHITE, GEN>(pos, move_list, cur)
    } else {
        generate_moves_typed::<US_BLACK, GEN>(pos, move_list, cur)
    }
}

#[inline(never)]
pub fn generate_moves_uninit(
    pos: &Position,
    move_list: &mut [MaybeUninit<Move>],
    cur: usize,
    gen_type: GenType,
) -> usize {
    debug_assert!(move_list.len() >= MAX_MOVES);
    match (pos.side_to_move(), gen_type) {
        (Color::White, GenType::Captures) => generate_moves_typed::<US_WHITE, GEN_CAPTURES>(pos, move_list, cur),
        (Color::Black, GenType::Captures) => generate_moves_typed::<US_BLACK, GEN_CAPTURES>(pos, move_list, cur),
        (Color::White, GenType::Quiets) => generate_moves_typed::<US_WHITE, GEN_QUIETS>(pos, move_list, cur),
        (Color::Black, GenType::Quiets) => generate_moves_typed::<US_BLACK, GEN_QUIETS>(pos, move_list, cur),
        (Color::White, GenType::Evasions) => generate_moves_typed::<US_WHITE, GEN_EVASIONS>(pos, move_list, cur),
        (Color::Black, GenType::Evasions) => generate_moves_typed::<US_BLACK, GEN_EVASIONS>(pos, move_list, cur),
        (_, GenType::All) => {
            if pos.checkers() != 0 {
                generate_moves_uninit(pos, move_list, cur, GenType::Evasions)
            } else {
                let after_captures = generate_moves_uninit(pos, move_list, cur, GenType::Captures);
                generate_moves_uninit(pos, move_list, after_captures, GenType::Quiets)
            }
        }
    }
}

pub fn generate_moves(
    pos: &Position,
    move_list: &mut [Move],
    cur: usize,
    gen_type: GenType,
) -> usize {
    let uninit = unsafe {
        core::slice::from_raw_parts_mut(
            move_list.as_mut_ptr() as *mut MaybeUninit<Move>,
            move_list.len(),
        )
    };
    generate_moves_uninit(pos, uninit, cur, gen_type)
}

pub fn generate_moves_all(pos: &Position, move_list: &mut [Move], cur: usize) -> usize {
    generate_moves(pos, move_list, cur, GenType::All)
}

pub struct MoveList {
    pub moves: [Move; MAX_MOVES],
    pub count: usize,
}

impl MoveList {
    pub fn new_all(pos: &Position) -> Self {
        let mut ml = MoveList {
            moves: [Move::none(); MAX_MOVES],
            count: 0,
        };
        ml.count = generate_moves_all(pos, &mut ml.moves, 0);
        ml
    }
}
