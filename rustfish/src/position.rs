use crate::attacks::*;
use crate::bitboard::*;
use crate::types::*;

pub struct ZobristKeys {
    pub psq: [[Key; SQUARE_NB]; PIECE_NB],
    pub enpassant: [Key; FILE_NB],
    pub castling: [Key; CASTLING_RIGHT_NB],
    pub side: Key,
    pub no_pawns: Key,
}

pub const ZOBRIST: ZobristKeys = {
    let mut psq = [[0u64; SQUARE_NB]; PIECE_NB];
    let mut enpassant = [0u64; FILE_NB];
    let mut castling = [0u64; CASTLING_RIGHT_NB];

    let mut s = 1070372u64;

    // We do the PRNG inline in const fn:
    // s ^= s >> 12; s ^= s << 25; s ^= s >> 27; s * 2685821657736338717
    macro_rules! next_key {
        () => {{
            s ^= s >> 12;
            s ^= s << 25;
            s ^= s >> 27;
            s.wrapping_mul(2685821657736338717)
        }};
    }

    let pieces = [
        Piece::WPawn,
        Piece::WKnight,
        Piece::WBishop,
        Piece::WRook,
        Piece::WQueen,
        Piece::WKing,
        Piece::BPawn,
        Piece::BKnight,
        Piece::BBishop,
        Piece::BRook,
        Piece::BQueen,
        Piece::BKing,
    ];

    let mut p_idx = 0;
    while p_idx < pieces.len() {
        let pc = pieces[p_idx] as usize;
        let mut sq = 0;
        while sq < 64 {
            psq[pc][sq] = next_key!();
            sq += 1;
        }
        p_idx += 1;
    }

    let mut sq = Square::SqA8 as usize;
    while sq <= Square::SqH8 as usize {
        psq[Piece::WPawn as usize][sq] = 0;
        sq += 1;
    }

    let mut sq = Square::SqA1 as usize;
    while sq <= Square::SqH1 as usize {
        psq[Piece::BPawn as usize][sq] = 0;
        sq += 1;
    }

    let mut f = 0;
    while f < FILE_NB {
        enpassant[f] = next_key!();
        f += 1;
    }

    let mut c = 0;
    while c < CASTLING_RIGHT_NB {
        castling[c] = next_key!();
        c += 1;
    }

    let side = next_key!();
    let no_pawns = next_key!();

    ZobristKeys {
        psq,
        enpassant,
        castling,
        side,
        no_pawns,
    }
};

#[inline(always)]
pub fn pack_pieces_into(board: &[Piece; SQUARE_NB], out: &mut [u8; 32]) {
    #[cfg(target_feature = "avx2")]
    unsafe {
        use core::arch::x86_64::*;
        let ptr = board.as_ptr() as *const __m256i;
        let lo = _mm256_loadu_si256(ptr);
        let hi = _mm256_loadu_si256(ptr.add(1));
        let packed = _mm256_or_si256(lo, _mm256_slli_epi16(hi, 4));
        _mm256_storeu_si256(out.as_mut_ptr() as *mut __m256i, packed);
    }
    #[cfg(not(target_feature = "avx2"))]
    {
        for k in (0..32).step_by(8) {
            let mut a = 0u64;
            let mut b = 0u64;
            for i in 0..8 {
                a |= (board[k + i] as u64) << (i * 8);
                b |= (board[k + 32 + i] as u64) << (i * 8);
            }
            a |= b << 4;
            out[k..k + 8].copy_from_slice(&a.to_le_bytes());
        }
    }
}

#[inline(always)]
pub fn pack_pieces(board: &[Piece; SQUARE_NB]) -> [u8; 32] {
    let mut p = [0u8; 32];
    pack_pieces_into(board, &mut p);
    p
}

use std::cell::UnsafeCell;

pub struct StateInfo {
    pub material_key: Key,
    pub pawn_key: Key,
    pub minor_piece_key: Key,
    pub non_pawn_key: [Key; COLOR_NB],
    pub non_pawn_material: [Value; COLOR_NB],
    pub castling_rights: u8,
    pub rule50: i32,
    pub plies_from_null: i32,
    pub ep_square: Square,

    pub key: Key,
    pub checkers_bb: Bitboard,
    pub blockers_for_king: [Bitboard; COLOR_NB],
    pub pinners: [Bitboard; COLOR_NB],
    pub check_squares: [Bitboard; PIECE_TYPE_NB],
    pub threats: UnsafeCell<[Bitboard; PIECE_TYPE_NB]>,
    pub captured_piece: Piece,
    pub repetition: i32,
    pub piece_placement: [u8; 32],
}

impl StateInfo {
    pub const fn new() -> Self {
        StateInfo {
            material_key: 0,
            pawn_key: 0,
            minor_piece_key: 0,
            non_pawn_key: [0; COLOR_NB],
            non_pawn_material: [0; COLOR_NB],
            castling_rights: 0,
            rule50: 0,
            plies_from_null: 0,
            ep_square: Square::SqNone,
            key: 0,
            checkers_bb: 0,
            blockers_for_king: [0; COLOR_NB],
            pinners: [0; COLOR_NB],
            check_squares: [0; PIECE_TYPE_NB],
            threats: UnsafeCell::new([0; PIECE_TYPE_NB]),
            captured_piece: Piece::NoPiece,
            repetition: 0,
            piece_placement: [0; 32],
        }
    }
}

impl Default for StateInfo {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for StateInfo {
    fn clone(&self) -> Self {
        StateInfo {
            material_key: self.material_key,
            pawn_key: self.pawn_key,
            minor_piece_key: self.minor_piece_key,
            non_pawn_key: self.non_pawn_key,
            non_pawn_material: self.non_pawn_material,
            castling_rights: self.castling_rights,
            rule50: self.rule50,
            plies_from_null: self.plies_from_null,
            ep_square: self.ep_square,
            key: self.key,
            checkers_bb: self.checkers_bb,
            blockers_for_king: self.blockers_for_king,
            pinners: self.pinners,
            check_squares: self.check_squares,
            threats: UnsafeCell::new(unsafe { *self.threats.get() }),
            captured_piece: self.captured_piece,
            repetition: self.repetition,
            piece_placement: self.piece_placement,
        }
    }
}

pub const PIECES_LIST: [Piece; 12] = [
    Piece::WPawn,
    Piece::WKnight,
    Piece::WBishop,
    Piece::WRook,
    Piece::WQueen,
    Piece::WKing,
    Piece::BPawn,
    Piece::BKnight,
    Piece::BBishop,
    Piece::BRook,
    Piece::BQueen,
    Piece::BKing,
];

pub struct Position {
    board: [Piece; SQUARE_NB],
    by_type_bb: [Bitboard; PIECE_TYPE_NB],
    by_color_bb: [Bitboard; COLOR_NB],
    piece_count: [i32; PIECE_NB],
    castling_rights_mask: [u8; SQUARE_NB],
    castling_rook_square: [Square; CASTLING_RIGHT_NB],
    castling_path: [Bitboard; CASTLING_RIGHT_NB],
    side_to_move: Color,
    game_ply: i32,
    chess960: bool,
    pub state_stack: Vec<StateInfo>,
    pub state_idx: usize,
}

impl Clone for Position {
    fn clone(&self) -> Self {
        let needed = (self.state_idx + MAX_PLY + 64).max(512);
        let mut state_stack = Vec::with_capacity(needed);
        for i in 0..=self.state_idx {
            state_stack.push(self.state_stack[i].clone());
        }
        state_stack.resize_with(needed, StateInfo::new);
        Self {
            board: self.board,
            by_type_bb: self.by_type_bb,
            by_color_bb: self.by_color_bb,
            piece_count: self.piece_count,
            castling_rights_mask: self.castling_rights_mask,
            castling_rook_square: self.castling_rook_square,
            castling_path: self.castling_path,
            side_to_move: self.side_to_move,
            game_ply: self.game_ply,
            chess960: self.chess960,
            state_stack,
            state_idx: self.state_idx,
        }
    }
}

impl Position {
    pub fn new() -> Self {
        Position {
            board: [Piece::NoPiece; SQUARE_NB],
            by_type_bb: [0; PIECE_TYPE_NB],
            by_color_bb: [0; COLOR_NB],
            piece_count: [0; PIECE_NB],
            castling_rights_mask: [0; SQUARE_NB],
            castling_rook_square: [Square::SqNone; CASTLING_RIGHT_NB],
            castling_path: [0; CASTLING_RIGHT_NB],
            side_to_move: Color::White,
            game_ply: 0,
            chess960: false,
            state_stack: vec![StateInfo::new(); 1024],
            state_idx: 0,
        }
    }

    pub fn ensure_capacity(&mut self, extra: usize) {
        let needed = self.state_idx + extra;
        if needed >= self.state_stack.len() {
            let new_len = (needed + 256).max(self.state_stack.len() * 2);
            self.state_stack.resize_with(new_len, StateInfo::new);
        }
    }

    #[inline(always)]
    pub fn state(&self) -> &StateInfo {
        unsafe { self.state_stack.get_unchecked(self.state_idx) }
    }

    #[inline(always)]
    pub fn state_mut(&mut self) -> &mut StateInfo {
        unsafe { self.state_stack.get_unchecked_mut(self.state_idx) }
    }

    #[inline(always)]
    pub fn side_to_move(&self) -> Color {
        self.side_to_move
    }

    #[inline(always)]
    pub fn game_ply(&self) -> i32 {
        self.game_ply
    }

    #[inline(always)]
    pub fn is_chess960(&self) -> bool {
        self.chess960
    }

    #[inline(always)]
    pub fn piece_on(&self, s: Square) -> Piece {
        debug_assert!(is_ok(s));
        self.board[s.idx()]
    }

    #[inline(always)]
    pub fn piece_array(&self) -> &[Piece; SQUARE_NB] {
        &self.board
    }

    #[inline(always)]
    pub fn empty(&self, s: Square) -> bool {
        self.piece_on(s) == Piece::NoPiece
    }

    #[inline(always)]
    pub fn moved_piece(&self, m: Move) -> Piece {
        self.piece_on(m.from_sq())
    }

    #[inline(always)]
    pub fn captured_piece(&self) -> Piece {
        self.state().captured_piece
    }

    #[inline(always)]
    pub fn pieces(&self) -> Bitboard {
        self.by_type_bb[PieceType::AllPieces as usize]
    }

    #[inline(always)]
    pub fn pieces_pt(&self, pt: PieceType) -> Bitboard {
        self.by_type_bb[pt as usize]
    }

    #[inline(always)]
    pub fn pieces_c(&self, c: Color) -> Bitboard {
        self.by_color_bb[c as usize]
    }

    #[inline(always)]
    pub fn pieces_c_pt(&self, c: Color, pt: PieceType) -> Bitboard {
        self.pieces_c(c) & self.pieces_pt(pt)
    }

    #[inline(always)]
    pub fn pieces_c_2pt(&self, c: Color, pt1: PieceType, pt2: PieceType) -> Bitboard {
        self.pieces_c(c) & (self.pieces_pt(pt1) | self.pieces_pt(pt2))
    }

    #[inline(always)]
    pub fn pieces_2pt(&self, pt1: PieceType, pt2: PieceType) -> Bitboard {
        self.pieces_pt(pt1) | self.pieces_pt(pt2)
    }

    #[inline(always)]
    pub fn count_pc(&self, pc: Piece) -> i32 {
        self.piece_count[pc as usize]
    }

    #[inline(always)]
    pub fn count_c_pt(&self, c: Color, pt: PieceType) -> i32 {
        self.piece_count[make_piece(c, pt) as usize]
    }

    #[inline(always)]
    pub fn count_pt(&self, pt: PieceType) -> i32 {
        self.count_c_pt(Color::White, pt) + self.count_c_pt(Color::Black, pt)
    }

    #[inline(always)]
    pub fn count_piece(&self, pt: PieceType) -> usize {
        self.count_pt(pt) as usize
    }

    #[inline(always)]
    pub fn count_color(&self, c: Color, pt: PieceType) -> usize {
        self.count_c_pt(c, pt) as usize
    }

    #[inline(always)]
    pub fn count_all(&self) -> usize {
        self.pieces().count_ones() as usize
    }

    #[inline(always)]
    pub fn square(&self, c: Color, pt: PieceType) -> Square {
        debug_assert!(self.count_c_pt(c, pt) == 1);
        lsb(self.pieces_c_pt(c, pt))
    }

    #[inline(always)]
    pub fn square_color(&self, pt: PieceType, c: Color) -> Square {
        self.square(c, pt)
    }

    #[inline(always)]
    pub fn pieces_type(&self, pt: PieceType) -> Bitboard {
        self.pieces_pt(pt)
    }

    #[inline(always)]
    pub fn pieces_color(&self, pt: PieceType, c: Color) -> Bitboard {
        self.pieces_c_pt(c, pt)
    }

    #[inline(always)]
    pub fn pieces_2(&self, pt1: PieceType, pt2: PieceType) -> Bitboard {
        self.pieces_pt(pt1) | self.pieces_pt(pt2)
    }

    #[inline(always)]
    pub fn pieces_4(&self, pt1: PieceType, pt2: PieceType, pt3: PieceType, pt4: PieceType) -> Bitboard {
        self.pieces_pt(pt1) | self.pieces_pt(pt2) | self.pieces_pt(pt3) | self.pieces_pt(pt4)
    }

    #[inline(always)]
    pub fn pieces_5(&self, pt1: PieceType, pt2: PieceType, pt3: PieceType, pt4: PieceType, pt5: PieceType) -> Bitboard {
        self.pieces_pt(pt1) | self.pieces_pt(pt2) | self.pieces_pt(pt3) | self.pieces_pt(pt4) | self.pieces_pt(pt5)
    }

    #[inline(always)]
    pub fn ep_square(&self) -> Square {
        self.state().ep_square
    }

    #[inline(always)]
    pub fn can_castle(&self, cr: CastlingRights) -> bool {
        (self.state().castling_rights & (cr as u8)) != 0
    }

    #[inline(always)]
    pub fn castling_impeded(&self, cr: CastlingRights) -> bool {
        (self.pieces() & self.castling_path[cr as usize]) != 0
    }

    #[inline(always)]
    pub fn castling_rook_square(&self, cr: CastlingRights) -> Square {
        self.castling_rook_square[cr as usize]
    }

    #[inline(always)]
    pub fn checkers(&self) -> Bitboard {
        self.state().checkers_bb
    }

    #[inline(always)]
    pub fn blockers_for_king(&self, c: Color) -> Bitboard {
        self.state().blockers_for_king[c as usize]
    }

    #[inline(always)]
    pub fn pinners(&self, c: Color) -> Bitboard {
        self.state().pinners[c as usize]
    }

    #[inline(always)]
    pub fn check_squares(&self, pt: PieceType) -> Bitboard {
        self.state().check_squares[pt as usize]
    }

    #[inline(always)]
    pub fn threats_by(&self, pt: PieceType) -> Bitboard {
        unsafe {
            let threats_ptr = self.state().threats.get();
            if (*threats_ptr)[PieceType::AllPieces as usize] == 0 {
                self.compute_threats();
            }
            (*threats_ptr)[pt as usize]
        }
    }

    #[inline(always)]
    pub fn key(&self) -> Key {
        self.adjust_key50::<false>(self.state().key)
    }

    #[inline(always)]
    pub fn adjust_key50<const AFTER_MOVE: bool>(&self, k: Key) -> Key {
        let after = if AFTER_MOVE { 1 } else { 0 };
        if self.state().rule50 < (14 - after) {
            k
        } else {
            k ^ make_key(((self.state().rule50 - (14 - after)) / 8) as u64)
        }
    }

    #[inline(always)]
    pub fn prefetch_key(&self, m: Move) -> Key {
        let from = m.from_sq();
        let to = m.to_sq();
        let pc = self.piece_on(from);
        let captured = self.piece_on(to);
        let mut k = self.state().key ^ ZOBRIST.side;
        k ^= ZOBRIST.psq[captured as usize][to.idx()]
            ^ ZOBRIST.psq[pc as usize][to.idx()]
            ^ ZOBRIST.psq[pc as usize][from.idx()];
        if captured != Piece::NoPiece || type_of(pc) == PieceType::Pawn {
            return k;
        }
        self.adjust_key50::<true>(k)
    }

    #[inline(always)]
    pub fn pawn_key(&self) -> Key {
        self.state().pawn_key
    }

    #[inline(always)]
    pub fn material_key(&self) -> Key {
        self.state().material_key
    }

    #[inline(always)]
    pub fn minor_piece_key(&self) -> Key {
        self.state().minor_piece_key
    }

    #[inline(always)]
    pub fn non_pawn_key(&self, c: Color) -> Key {
        self.state().non_pawn_key[c as usize]
    }

    #[inline(always)]
    pub fn non_pawn_material_c(&self, c: Color) -> Value {
        self.state().non_pawn_material[c as usize]
    }

    #[inline(always)]
    pub fn non_pawn_material(&self) -> Value {
        self.non_pawn_material_c(Color::White) + self.non_pawn_material_c(Color::Black)
    }

    #[inline(always)]
    pub fn rule50_count(&self) -> i32 {
        self.state().rule50
    }

    #[inline(always)]
    pub fn capture(&self, m: Move) -> bool {
        debug_assert!(m.is_ok());
        let mt = m.move_type();
        if mt == MoveType::Normal || mt == MoveType::Promotion {
            !self.empty(m.to_sq())
        } else {
            mt == MoveType::EnPassant
        }
    }

    #[inline(always)]
    pub fn capture_stage(&self, m: Move) -> bool {
        debug_assert!(m.is_ok());
        let mt = m.move_type();
        if mt == MoveType::Normal {
            !self.empty(m.to_sq())
        } else if mt == MoveType::Promotion {
            !self.empty(m.to_sq()) || m.promotion_type() == PieceType::Queen
        } else {
            mt == MoveType::EnPassant
        }
    }

    #[inline(always)]
    pub fn put_piece(&mut self, pc: Piece, s: Square, dts: Option<&mut DirtyThreats>) {
        self.board[s.idx()] = pc;
        let s_bb = square_bb(s);
        let pt = type_of(pc);
        let c = color_of(pc);
        self.by_type_bb[PieceType::AllPieces as usize] |= s_bb;
        self.by_type_bb[pt as usize] |= s_bb;
        self.by_color_bb[c as usize] |= s_bb;
        self.piece_count[pc as usize] += 1;
        self.piece_count[make_piece(c, PieceType::AllPieces) as usize] += 1;

        if let Some(dts) = dts {
            self.update_piece_threats::<true>(pc, true, s, dts, !0);
        }
    }

    #[inline(always)]
    pub fn remove_piece(&mut self, s: Square, dts: Option<&mut DirtyThreats>) {
        let pc = self.board[s.idx()];
        if let Some(dts) = dts {
            self.update_piece_threats::<true>(pc, false, s, dts, !0);
        }
        let s_bb = square_bb(s);
        let pt = type_of(pc);
        let c = color_of(pc);
        self.by_type_bb[PieceType::AllPieces as usize] ^= s_bb;
        self.by_type_bb[pt as usize] ^= s_bb;
        self.by_color_bb[c as usize] ^= s_bb;
        self.board[s.idx()] = Piece::NoPiece;
        self.piece_count[pc as usize] -= 1;
        self.piece_count[make_piece(c, PieceType::AllPieces) as usize] -= 1;
    }

    #[inline(always)]
    pub fn move_piece(&mut self, from: Square, to: Square, mut dts: Option<&mut DirtyThreats>) {
        let pc = self.board[from.idx()];
        let from_to = square_bb(from) | square_bb(to);

        if let Some(ref mut d) = dts {
            self.update_piece_threats::<true>(pc, false, from, *d, from_to);
        }

        let pt = type_of(pc);
        let c = color_of(pc);
        self.by_type_bb[PieceType::AllPieces as usize] ^= from_to;
        self.by_type_bb[pt as usize] ^= from_to;
        self.by_color_bb[c as usize] ^= from_to;
        self.board[from.idx()] = Piece::NoPiece;
        self.board[to.idx()] = pc;

        if let Some(d) = dts {
            self.update_piece_threats::<true>(pc, true, to, d, from_to);
        }
    }

    #[inline(always)]
    pub fn swap_piece(&mut self, s: Square, pc: Piece, mut dts: Option<&mut DirtyThreats>) {
        let old = self.board[s.idx()];
        self.remove_piece(s, None);
        if let Some(ref mut d) = dts {
            self.update_piece_threats::<false>(old, false, s, *d, !0);
        }
        self.put_piece(pc, s, None);
        if let Some(d) = dts {
            self.update_piece_threats::<false>(pc, true, s, d, !0);
        }
    }

    #[inline(never)]
    pub fn update_piece_threats<const COMPUTE_RAY: bool>(
        &self,
        pc: Piece,
        put_piece: bool,
        s: Square,
        dts: &mut DirtyThreats,
        no_rays_containing: Bitboard,
    ) {
        let occupied = self.pieces();
        let (b_attacks, r_attacks) = both_attacks_bb(s, occupied);
        let slider_attacks = b_attacks | r_attacks;
        let occupied_no_k = occupied ^ self.pieces_pt(PieceType::King);
        let pt = type_of(pc);
        let sliders = (self.pieces_2pt(PieceType::Bishop, PieceType::Queen) & b_attacks)
            | (self.pieces_2pt(PieceType::Rook, PieceType::Queen) & r_attacks);

        let can_slider_threat = |threatened: Piece, slider: Piece| -> bool {
            type_of(threatened) != PieceType::Queen || type_of(slider) == PieceType::Queen
        };

        let process_sliders = |add_direct: bool, dts: &mut DirtyThreats| {
            let mut b = sliders;
            while b != 0 {
                let slider_sq = pop_lsb(&mut b);
                let slider = self.piece_on(slider_sq);

                let ray = ray_pass_bb(slider_sq, s);
                let discovered = ray & slider_attacks & occupied_no_k;

                debug_assert!(!more_than_one(discovered));
                if discovered != 0 && (ray & no_rays_containing) != no_rays_containing {
                    let threatened_sq = lsb(discovered);
                    let threatened_pc = self.piece_on(threatened_sq);
                    if can_slider_threat(threatened_pc, slider) {
                        dts.list.push(DirtyThreat::new(
                            slider,
                            threatened_pc,
                            slider_sq,
                            threatened_sq,
                            !put_piece,
                        ));
                    }
                }

                if add_direct && can_slider_threat(pc, slider) {
                    dts.list.push(DirtyThreat::new(slider, pc, slider_sq, s, put_piece));
                }
            }
        };

        if pt == PieceType::King {
            if COMPUTE_RAY {
                process_sliders(false, dts);
            }
            return;
        }

        let threat_targets = match pt {
            PieceType::Pawn => self.pieces_2pt(PieceType::Knight, PieceType::Rook),
            PieceType::Bishop | PieceType::Rook => {
                self.pieces_2pt(PieceType::Pawn, PieceType::Knight)
                    | self.pieces_2pt(PieceType::Bishop, PieceType::Rook)
            }
            _ => occupied_no_k,
        };

        let mut threatened = (match pt {
            PieceType::Bishop => b_attacks,
            PieceType::Rook => r_attacks,
            PieceType::Queen => slider_attacks,
            PieceType::Pawn => PSEUDO_ATTACKS[color_of(pc) as usize][s.idx()],
            _ => PSEUDO_ATTACKS[pt as usize][s.idx()],
        }) & threat_targets;

        let mut incoming_threats = PSEUDO_ATTACKS[PieceType::Knight as usize][s.idx()]
            & self.pieces_pt(PieceType::Knight);

        if pt == PieceType::Knight || pt == PieceType::Rook {
            incoming_threats |= (attacks_bb_empty(PieceType::Pawn, s, Color::White)
                & self.pieces_c_pt(Color::Black, PieceType::Pawn))
                | (attacks_bb_empty(PieceType::Pawn, s, Color::Black)
                    & self.pieces_c_pt(Color::White, PieceType::Pawn));
        }

        while threatened != 0 {
            let threatened_sq = pop_lsb(&mut threatened);
            let threatened_pc = self.piece_on(threatened_sq);
            debug_assert!(threatened_sq != s);
            debug_assert!(threatened_pc != Piece::NoPiece);

            dts.list.push(DirtyThreat::new(pc, threatened_pc, s, threatened_sq, put_piece));
        }

        if COMPUTE_RAY {
            process_sliders(true, dts);
        } else {
            incoming_threats |= if pt == PieceType::Queen {
                sliders & self.pieces_pt(PieceType::Queen)
            } else {
                sliders
            };
        }

        while incoming_threats != 0 {
            let src_sq = pop_lsb(&mut incoming_threats);
            let src_pc = self.piece_on(src_sq);
            debug_assert!(src_sq != s);
            debug_assert!(src_pc != Piece::NoPiece);

            dts.list.push(DirtyThreat::new(src_pc, pc, src_sq, s, put_piece));
        }
    }

    pub fn slider_blockers(&self, c: Color) -> (Bitboard, Bitboard) {
        let ksq = self.square(c, PieceType::King);

        let mut blockers = 0u64;
        let mut pinners = 0u64;

        let snipers = ((attacks_bb(PieceType::Rook, ksq, 0)
            & self.pieces_2pt(PieceType::Queen, PieceType::Rook))
            | (attacks_bb(PieceType::Bishop, ksq, 0)
                & self.pieces_2pt(PieceType::Queen, PieceType::Bishop)))
            & self.pieces_c(!c);

        let occupancy = self.pieces() ^ snipers;
        let mut snipers_b = snipers;

        while snipers_b != 0 {
            let sniper_sq = pop_lsb(&mut snipers_b);
            let b = between_bb(ksq, sniper_sq) & occupancy;

            if b != 0 && !more_than_one(b) {
                blockers |= b;
                if (b & self.pieces_c(c)) != 0 {
                    pinners |= square_bb(sniper_sq);
                }
            }
        }

        (blockers, pinners)
    }

    #[inline(always)]
    pub fn attackers_to(&self, s: Square, occupied: Bitboard) -> Bitboard {
        let (bishop_attacks, rook_attacks) = both_attacks_bb(s, occupied);

        (rook_attacks & self.pieces_2pt(PieceType::Rook, PieceType::Queen))
            | (bishop_attacks & self.pieces_2pt(PieceType::Bishop, PieceType::Queen))
            | (pawn_attacks_bb(Color::Black, square_bb(s))
                & self.pieces_c_pt(Color::White, PieceType::Pawn))
            | (pawn_attacks_bb(Color::White, square_bb(s))
                & self.pieces_c_pt(Color::Black, PieceType::Pawn))
            | (PSEUDO_ATTACKS[PieceType::Knight as usize][s.idx()]
                & self.pieces_pt(PieceType::Knight))
            | (PSEUDO_ATTACKS[PieceType::King as usize][s.idx()]
                & self.pieces_pt(PieceType::King))
    }

    #[inline(never)]
    pub fn compute_threats(&self) {
        let occ = self.pieces() ^ square_bb(self.square(self.side_to_move, PieceType::King));
        let them = !self.side_to_move;

        let pawns = if them == Color::White {
            pawn_attacks_bb(Color::White, self.pieces_c_pt(Color::White, PieceType::Pawn))
        } else {
            pawn_attacks_bb(Color::Black, self.pieces_c_pt(Color::Black, PieceType::Pawn))
        };

        let mut knights = 0u64;
        let mut bb = self.pieces_c_pt(them, PieceType::Knight);
        while bb != 0 {
            knights |= PSEUDO_ATTACKS[PieceType::Knight as usize][pop_lsb(&mut bb).idx()];
        }

        let mut bishops = 0u64;
        let mut bb = self.pieces_c_pt(them, PieceType::Bishop);
        while bb != 0 {
            bishops |= bishop_attacks_bb(pop_lsb(&mut bb), occ);
        }

        let mut rooks = 0u64;
        let mut bb = self.pieces_c_pt(them, PieceType::Rook);
        while bb != 0 {
            rooks |= rook_attacks_bb(pop_lsb(&mut bb), occ);
        }

        let mut queens = 0u64;
        let mut bb = self.pieces_c_pt(them, PieceType::Queen);
        while bb != 0 {
            let (b, r) = both_attacks_bb(pop_lsb(&mut bb), occ);
            queens |= b | r;
        }

        let kings = PSEUDO_ATTACKS[PieceType::King as usize][self.square(them, PieceType::King).idx()];

        let all = pawns | knights | bishops | rooks | queens | kings;

        unsafe {
            let threats = &mut *self.state().threats.get();
            threats[PieceType::AllPieces as usize] = all;
            threats[PieceType::Pawn as usize] = pawns;
            threats[PieceType::Knight as usize] = knights;
            threats[PieceType::Bishop as usize] = bishops;
            threats[PieceType::Rook as usize] = rooks;
            threats[PieceType::Queen as usize] = queens;
            threats[PieceType::King as usize] = kings;
        }
    }

    #[inline(never)]
    pub fn set_extra_bitboards(&mut self) {
        let (w_blockers, b_pinners) = self.slider_blockers(Color::White);
        let (b_blockers, w_pinners) = self.slider_blockers(Color::Black);

        let ksq = self.square(!self.side_to_move, PieceType::King);
        let (bishop_attacks, rook_attacks) = both_attacks_bb(ksq, self.pieces());
        let pawn_checks = pawn_attacks_bb(!self.side_to_move, square_bb(ksq));
        let knight_checks = PSEUDO_ATTACKS[PieceType::Knight as usize][ksq.idx()];

        let st = self.state_mut();
        st.blockers_for_king[Color::White as usize] = w_blockers;
        st.pinners[Color::Black as usize] = b_pinners;
        st.blockers_for_king[Color::Black as usize] = b_blockers;
        st.pinners[Color::White as usize] = w_pinners;

        st.check_squares[PieceType::Pawn as usize] = pawn_checks;
        st.check_squares[PieceType::Knight as usize] = knight_checks;
        st.check_squares[PieceType::Bishop as usize] = bishop_attacks;
        st.check_squares[PieceType::Rook as usize] = rook_attacks;
        st.check_squares[PieceType::Queen as usize] = bishop_attacks | rook_attacks;
        st.check_squares[PieceType::King as usize] = 0;
        unsafe {
            (*st.threats.get())[PieceType::AllPieces as usize] = 0;
        }
    }

    pub fn compute_material_key(&self) -> Key {
        let mut k = 0u64;
        for pc in PIECES_LIST {
            let count = self.piece_count[pc as usize];
            let mut cnt = 0;
            while cnt < count {
                k ^= ZOBRIST.psq[pc as usize][8 + cnt as usize];
                cnt += 1;
            }
        }
        k
    }

    pub fn set_state(&mut self) {
        let us = self.side_to_move;
        let them = !us;
        let king_sq = self.square(us, PieceType::King);
        let checkers = self.attackers_to(king_sq, self.pieces()) & self.pieces_c(them);
        let material_key = self.compute_material_key();
        let placement = pack_pieces(&self.board);

        self.set_extra_bitboards();

        let mut key = 0u64;
        let mut minor_piece_key = 0u64;
        let mut non_pawn_key = [0u64; COLOR_NB];
        let mut pawn_key = ZOBRIST.no_pawns;
        let mut non_pawn_material = [VALUE_ZERO; COLOR_NB];

        let mut b = self.pieces();
        while b != 0 {
            let s = pop_lsb(&mut b);
            let pc = self.piece_on(s);
            key ^= ZOBRIST.psq[pc as usize][s as usize];

            if type_of(pc) == PieceType::Pawn {
                pawn_key ^= ZOBRIST.psq[pc as usize][s as usize];
            } else {
                let c = color_of(pc) as usize;
                non_pawn_key[c] ^= ZOBRIST.psq[pc as usize][s as usize];
                non_pawn_material[c] += PIECE_VALUE[pc as usize];

                if type_of(pc) <= PieceType::Bishop {
                    minor_piece_key ^= ZOBRIST.psq[pc as usize][s as usize];
                }
            }
        }

        let st = self.state_mut();
        st.checkers_bb = checkers;
        st.key = key;
        st.minor_piece_key = minor_piece_key;
        st.non_pawn_key = non_pawn_key;
        st.pawn_key = pawn_key;
        st.non_pawn_material = non_pawn_material;

        if st.ep_square != Square::SqNone {
            st.key ^= ZOBRIST.enpassant[file_of(st.ep_square) as usize];
        }

        if us == Color::Black {
            st.key ^= ZOBRIST.side;
        }

        st.key ^= ZOBRIST.castling[st.castling_rights as usize];
        st.material_key = material_key;
        st.piece_placement = placement;
    }

    pub fn set_castling_right(&mut self, c: Color, rfrom: Square) {
        let kfrom = self.square(c, PieceType::King);
        let cr = match c {
            Color::White => {
                if (kfrom as u8) < (rfrom as u8) {
                    CastlingRights::WhiteOO
                } else {
                    CastlingRights::WhiteOOO
                }
            }
            Color::Black => {
                if (kfrom as u8) < (rfrom as u8) {
                    CastlingRights::BlackOO
                } else {
                    CastlingRights::BlackOOO
                }
            }
        };

        let cr_u8 = cr as u8;
        self.state_mut().castling_rights |= cr_u8;
        self.castling_rights_mask[kfrom as usize] |= cr_u8;
        self.castling_rights_mask[rfrom as usize] |= cr_u8;
        self.castling_rook_square[cr as usize] = rfrom;

        let king_side = (cr == CastlingRights::WhiteOO) || (cr == CastlingRights::BlackOO);
        let kto = relative_square(c, if king_side { Square::SqG1 } else { Square::SqC1 });
        let rto = relative_square(c, if king_side { Square::SqF1 } else { Square::SqD1 });

        self.castling_path[cr as usize] =
            (between_bb(rfrom, rto) | between_bb(kfrom, kto)) & !(square_bb(kfrom) | square_bb(rfrom));
    }

    pub fn set(&mut self, fen: &str, is_chess960: bool) -> Result<(), String> {
        self.board = [Piece::NoPiece; SQUARE_NB];
        self.by_type_bb = [0; PIECE_TYPE_NB];
        self.by_color_bb = [0; COLOR_NB];
        self.piece_count = [0; PIECE_NB];
        self.castling_rights_mask = [0; SQUARE_NB];
        self.castling_rook_square = [Square::SqNone; CASTLING_RIGHT_NB];
        self.castling_path = [0; CASTLING_RIGHT_NB];
        self.state_idx = 0;
        self.state_stack[0] = StateInfo::default();

        let mut tokens = fen.split_whitespace();
        let piece_placement = tokens.next().ok_or("Empty FEN")?;

        let mut sq = Square::SqA8 as i8;
        for c in piece_placement.chars() {
            if c.is_ascii_digit() {
                sq += c.to_digit(10).unwrap() as i8;
            } else if c == '/' {
                sq -= 16;
            } else {
                let pc = match c {
                    'P' => Piece::WPawn,
                    'N' => Piece::WKnight,
                    'B' => Piece::WBishop,
                    'R' => Piece::WRook,
                    'Q' => Piece::WQueen,
                    'K' => Piece::WKing,
                    'p' => Piece::BPawn,
                    'n' => Piece::BKnight,
                    'b' => Piece::BBishop,
                    'r' => Piece::BRook,
                    'q' => Piece::BQueen,
                    'k' => Piece::BKing,
                    _ => return Err(format!("Unknown piece char: {}", c)),
                };
                self.put_piece(pc, Square::from_u8(sq as u8), None);
                sq += 1;
            }
        }

        let stm = tokens.next().unwrap_or("w");
        self.side_to_move = if stm == "b" { Color::Black } else { Color::White };

        let castling = tokens.next().unwrap_or("-");
        for c in castling.chars() {
            if c == '-' {
                break;
            }
            let color = if c.is_ascii_uppercase() {
                Color::White
            } else {
                Color::Black
            };
            let c_upper = c.to_ascii_uppercase();
            let mut rsq = Square::SqNone;
            let mut ksq = Square::SqNone;
            let rook = make_piece(color, PieceType::Rook);
            let king = make_piece(color, PieceType::King);

            if c_upper == 'K' || c_upper == 'Q' {
                let dir: i8 = if c_upper == 'K' { -1 } else { 1 };
                let mut sq = relative_square(color, if c_upper == 'K' { Square::SqH1 } else { Square::SqA1 });
                for _ in 0..7 {
                    let pc = self.piece_on(sq);
                    if pc == king {
                        ksq = sq;
                        break;
                    } else if pc == rook && rsq == Square::SqNone {
                        rsq = sq;
                    }
                    sq = Square::from_u8((sq as i8 + dir) as u8);
                }
            } else if c_upper >= 'A' && c_upper <= 'H' {
                let rsq_candidate = make_square(
                    File::from_u8((c_upper as u8) - b'A'),
                    relative_rank(color, Rank::Rank1),
                );
                if self.piece_on(rsq_candidate) == rook {
                    rsq = rsq_candidate;
                }
                let sq = self.square(color, PieceType::King);
                if relative_rank_of_sq(color, sq) == Rank::Rank1
                    && file_of(sq) >= File::FileB
                    && file_of(sq) <= File::FileG
                {
                    ksq = sq;
                }
            } else {
                return Err(format!("Invalid castling character: {}", c));
            }

            if ksq != Square::SqNone && rsq != Square::SqNone {
                self.set_castling_right(color, rsq);
            }
        }

        let ep = tokens.next().unwrap_or("-");
        let mut legal_ep = false;
        if ep != "-" && ep.len() == 2 {
            let f = File::from_u8(ep.as_bytes()[0] - b'a');
            let r = Rank::from_u8(ep.as_bytes()[1] - b'1');
            let ep_sq = make_square(f, r);
            let st = self.state_mut();
            st.ep_square = ep_sq;

            if relative_rank_of_sq(self.side_to_move, ep_sq) == Rank::Rank6 {
                let pawns = attacks_bb_empty(PieceType::Pawn, ep_sq, !self.side_to_move)
                    & self.pieces_c_pt(self.side_to_move, PieceType::Pawn);
                let target = attacks_bb_empty(PieceType::Pawn, ep_sq, self.side_to_move)
                    & self.pieces_c_pt(!self.side_to_move, PieceType::Pawn)
                    & file_bb(f);
                let capsq = ep_sq.add_dir(pawn_push(!self.side_to_move));
                let occ = self.pieces() ^ square_bb(capsq);

                let enpassant = pawns != 0
                    && target != 0
                    && (self.pieces()
                        & (square_bb(ep_sq) | square_bb(ep_sq.add_dir(pawn_push(self.side_to_move)))))
                        == 0;

                let mut p_copy = pawns;
                while enpassant && p_copy != 0 && !legal_ep {
                    let p_sq = pop_lsb(&mut p_copy);
                    let ksq = self.square(self.side_to_move, PieceType::King);
                    let leaving_check = (self.attackers_to(ksq, occ ^ square_bb(p_sq))
                        & self.pieces_c(!self.side_to_move)
                        & !target)
                        != 0;
                    if !leaving_check {
                        legal_ep = true;
                    }
                }
            }
        }

        if !legal_ep {
            self.state_mut().ep_square = Square::SqNone;
        }

        let rule50 = tokens.next().unwrap_or("0").parse::<i32>().unwrap_or(0);
        let mut fullmove = tokens.next().unwrap_or("1").parse::<i32>().unwrap_or(1);
        fullmove = std::cmp::max(2 * (fullmove - 1), 0) + (if self.side_to_move == Color::Black { 1 } else { 0 });

        self.state_mut().rule50 = rule50;
        self.game_ply = fullmove;
        self.chess960 = is_chess960;

        self.set_state();

        let enemy_king = self.square(!self.side_to_move, PieceType::King);
        if (self.attackers_to(enemy_king, self.pieces()) & self.pieces_c(self.side_to_move)) != 0 {
            return Err("King can be captured in FEN".to_string());
        }

        Ok(())
    }

    pub fn fen(&self) -> String {
        let mut s = String::new();
        let mut r = 7i32;
        while r >= 0 {
            let mut empty_cnt = 0;
            let mut f = 0;
            while f < 8 {
                let sq = make_square(File::from_u8(f as u8), Rank::from_u8(r as u8));
                let pc = self.piece_on(sq);
                if pc == Piece::NoPiece {
                    empty_cnt += 1;
                } else {
                    if empty_cnt > 0 {
                        s.push_str(&empty_cnt.to_string());
                        empty_cnt = 0;
                    }
                    let ch = match pc {
                        Piece::WPawn => 'P',
                        Piece::WKnight => 'N',
                        Piece::WBishop => 'B',
                        Piece::WRook => 'R',
                        Piece::WQueen => 'Q',
                        Piece::WKing => 'K',
                        Piece::BPawn => 'p',
                        Piece::BKnight => 'n',
                        Piece::BBishop => 'b',
                        Piece::BRook => 'r',
                        Piece::BQueen => 'q',
                        Piece::BKing => 'k',
                        _ => ' ',
                    };
                    s.push(ch);
                }
                f += 1;
            }
            if empty_cnt > 0 {
                s.push_str(&empty_cnt.to_string());
            }
            if r > 0 {
                s.push('/');
            }
            r -= 1;
        }

        s.push_str(if self.side_to_move == Color::White { " w " } else { " b " });

        let mut castling = String::new();
        if self.can_castle(CastlingRights::WhiteOO) {
            castling.push(if self.chess960 {
                (b'A' + file_of(self.castling_rook_square(CastlingRights::WhiteOO)) as u8) as char
            } else {
                'K'
            });
        }
        if self.can_castle(CastlingRights::WhiteOOO) {
            castling.push(if self.chess960 {
                (b'A' + file_of(self.castling_rook_square(CastlingRights::WhiteOOO)) as u8) as char
            } else {
                'Q'
            });
        }
        if self.can_castle(CastlingRights::BlackOO) {
            castling.push(if self.chess960 {
                (b'a' + file_of(self.castling_rook_square(CastlingRights::BlackOO)) as u8) as char
            } else {
                'k'
            });
        }
        if self.can_castle(CastlingRights::BlackOOO) {
            castling.push(if self.chess960 {
                (b'a' + file_of(self.castling_rook_square(CastlingRights::BlackOOO)) as u8) as char
            } else {
                'q'
            });
        }
        if castling.is_empty() {
            castling.push('-');
        }
        s.push_str(&castling);

        s.push(' ');
        if self.ep_square() == Square::SqNone {
            s.push('-');
        } else {
            let f = (b'a' + file_of(self.ep_square()) as u8) as char;
            let r = (b'1' + rank_of(self.ep_square()) as u8) as char;
            s.push(f);
            s.push(r);
        }

        s.push_str(&format!(
            " {} {}",
            self.state().rule50,
            1 + (self.game_ply - if self.side_to_move == Color::Black { 1 } else { 0 }) / 2
        ));

        s
    }

    #[inline(never)]
    pub fn legal(&self, m: Move) -> bool {
        let us = self.side_to_move;
        let from = m.from_sq();
        let to = m.to_sq();
        let pc = self.moved_piece(m);

        if m.move_type() != MoveType::Normal {
            use crate::movegen::*;
            let mut list = [Move::none(); MAX_MOVES];
            let count = generate_moves_all(self, &mut list, 0);
            return list[..count].contains(&m);
        }

        if pc == Piece::NoPiece || color_of(pc) != us {
            return false;
        }

        if (self.pieces_c(us) & square_bb(to)) != 0 {
            return false;
        }

        if type_of(pc) == PieceType::Pawn {
            if ((RANK_8_BB | RANK_1_BB) & square_bb(to)) != 0 {
                return false;
            }

            let is_capture = (pawn_attacks_bb(us, square_bb(from)) & self.pieces_c(!us) & square_bb(to)) != 0;
            let is_single_push = (from.add_dir(pawn_push(us)) == to) && self.empty(to);
            let is_double_push = (from.add_dir(pawn_push(us)).add_dir(pawn_push(us)) == to)
                && (relative_rank_of_sq(us, from) == Rank::Rank2)
                && self.empty(to)
                && self.empty(to.add_dir(pawn_push(!us)));

            if !(is_capture || is_single_push || is_double_push) {
                return false;
            }
        } else if (attacks_bb(type_of(pc), from, self.pieces()) & square_bb(to)) == 0 {
            return false;
        }

        if type_of(pc) != PieceType::King {
            if self.checkers() != 0 {
                if more_than_one(self.checkers()) {
                    return false;
                }
                let checker_sq = lsb(self.checkers());
                if ((between_bb(self.square(us, PieceType::King), checker_sq) | square_bb(checker_sq))
                    & square_bb(to))
                    == 0
                {
                    return false;
                }
            }

            if (self.blockers_for_king(us) & square_bb(from)) != 0 {
                return (line_bb(self.square(us, PieceType::King), from) & square_bb(to)) != 0;
            }

            return true;
        }

        (square_bb(to) & self.threats_by(PieceType::AllPieces)) == 0
    }

    #[inline(always)]
    pub fn gives_check(&self, m: Move) -> bool {
        debug_assert!(m.is_ok());
        let from = m.from_sq();
        let to = m.to_sq();
        let pc = self.moved_piece(m);
        let pt = type_of(pc);
        let us = self.side_to_move;

        if (self.check_squares(pt) & square_bb(to)) != 0 {
            return true;
        }

        if (self.blockers_for_king(!us) & square_bb(from)) != 0 {
            if (line_bb(from, to) & self.pieces_c_pt(!us, PieceType::King)) == 0
                || m.move_type() == MoveType::Castling
            {
                return true;
            }
        }

        if m.move_type() == MoveType::Normal {
            false
        } else {
            self.gives_check_special(m)
        }
    }

    #[cold]
    #[inline(never)]
    fn gives_check_special(&self, m: Move) -> bool {
        let from = m.from_sq();
        let to = m.to_sq();
        let us = self.side_to_move;
        match m.move_type() {
            MoveType::Normal => false,
            MoveType::Promotion => {
                let promo = m.promotion_type();
                (attacks_bb(promo, to, self.pieces() ^ square_bb(from))
                    & self.pieces_c_pt(!us, PieceType::King))
                    != 0
            }
            MoveType::EnPassant => {
                let capsq = make_square(file_of(to), rank_of(from));
                let b = (self.pieces() ^ square_bb(from) ^ square_bb(capsq)) | square_bb(to);
                let (bishop_attacks, rook_attacks) = both_attacks_bb(self.square(!us, PieceType::King), b);
                ((rook_attacks & self.pieces_c_2pt(us, PieceType::Queen, PieceType::Rook))
                    | (bishop_attacks & self.pieces_c_2pt(us, PieceType::Queen, PieceType::Bishop)))
                    != 0
            }
            MoveType::Castling => {
                let rto = relative_square(us, if to > from { Square::SqF1 } else { Square::SqD1 });
                (self.check_squares(PieceType::Rook) & square_bb(rto)) != 0
            }
        }
    }

    pub fn do_castling<const DO: bool>(
        &mut self,
        us: Color,
        from: Square,
        to: &mut Square,
        rfrom: &mut Square,
        rto: &mut Square,
        mut dts: Option<&mut DirtyThreats>,
        dp: Option<&mut DirtyPiece>,
    ) {
        let king_side = (*to as u8) > (from as u8);
        *rfrom = *to;
        *rto = relative_square(us, if king_side { Square::SqF1 } else { Square::SqD1 });
        *to = relative_square(us, if king_side { Square::SqG1 } else { Square::SqC1 });

        if DO {
            if let Some(dp) = dp {
                dp.to = *to;
                dp.remove_pc = make_piece(us, PieceType::Rook);
                dp.add_pc = make_piece(us, PieceType::Rook);
                dp.remove_sq = *rfrom;
                dp.add_sq = *rto;
            }
        }

        if DO {
            self.remove_piece(from, dts.as_deref_mut());
            self.remove_piece(*rfrom, dts.as_deref_mut());
            self.put_piece(make_piece(us, PieceType::King), *to, dts.as_deref_mut());
            self.put_piece(make_piece(us, PieceType::Rook), *rto, dts);
        } else {
            self.remove_piece(*to, dts.as_deref_mut());
            self.remove_piece(*rto, dts.as_deref_mut());
            self.put_piece(make_piece(us, PieceType::King), from, dts.as_deref_mut());
            self.put_piece(make_piece(us, PieceType::Rook), *rfrom, dts);
        }
    }

    pub fn do_move(
        &mut self,
        m: Move,
        gives_check: bool,
        dirties: Option<&mut Dirties>,
    ) {
        self.do_move_inner(m, gives_check, dirties, None);
    }

    #[inline(never)]
    pub fn do_move_search(
        &mut self,
        m: Move,
        gives_check: bool,
        dirties: &mut Dirties,
        history: &crate::history::SharedHistories,
    ) {
        self.do_move_inner(m, gives_check, Some(dirties), Some(history));
    }

    #[inline(always)]
    fn do_move_inner(
        &mut self,
        m: Move,
        gives_check: bool,
        mut dirties: Option<&mut Dirties>,
        history: Option<&crate::history::SharedHistories>,
    ) {
        debug_assert!(m.is_ok());
        let prev_idx = self.state_idx;
        self.state_idx += 1;

        struct StatePrefix {
            material_key: Key,
            pawn_key: Key,
            minor_piece_key: Key,
            non_pawn_key: [Key; COLOR_NB],
            non_pawn_material: [Value; COLOR_NB],
            castling_rights: u8,
            rule50: i32,
            plies_from_null: i32,
            ep_square: Square,
            key: Key,
            captured_piece: Piece,
            checkers_bb: Bitboard,
        }

        let prev = unsafe { self.state_stack.get_unchecked(prev_idx) };
        let mut new_st = StatePrefix {
            material_key: prev.material_key,
            pawn_key: prev.pawn_key,
            minor_piece_key: prev.minor_piece_key,
            non_pawn_key: prev.non_pawn_key,
            non_pawn_material: prev.non_pawn_material,
            castling_rights: prev.castling_rights,
            rule50: prev.rule50 + 1,
            plies_from_null: prev.plies_from_null + 1,
            ep_square: prev.ep_square,
            key: prev.key,
            captured_piece: Piece::NoPiece,
            checkers_bb: 0,
        };

        let mut k = new_st.key ^ ZOBRIST.side;
        self.game_ply += 1;

        let us = self.side_to_move;
        let them = !us;
        let from = m.from_sq();
        let mut to = m.to_sq();
        let pc = self.piece_on(from);
        let mut captured = if m.move_type() == MoveType::EnPassant {
            make_piece(them, PieceType::Pawn)
        } else {
            self.piece_on(to)
        };

        if let Some(ref mut d) = dirties {
            d.dirty_pawn_pairs.before[Color::White as usize] = self.pieces_c_pt(Color::White, PieceType::Pawn);
            d.dirty_pawn_pairs.before[Color::Black as usize] = self.pieces_c_pt(Color::Black, PieceType::Pawn);
            d.dirty_piece.pc = pc;
            d.dirty_piece.from = from;
            d.dirty_piece.to = to;
            d.dirty_piece.add_sq = Square::SqNone;
        }

        if m.move_type() == MoveType::Castling {
            let mut rfrom = Square::SqNone;
            let mut rto = Square::SqNone;
            let (dts, dp) = match dirties {
                Some(ref mut d) => (Some(&mut d.dirty_threats), Some(&mut d.dirty_piece)),
                None => (None, None),
            };

            self.do_castling::<true>(us, from, &mut to, &mut rfrom, &mut rto, dts, dp);
            k ^= ZOBRIST.psq[captured as usize][rfrom.idx()] ^ ZOBRIST.psq[captured as usize][rto.idx()];
            new_st.non_pawn_key[us as usize] ^=
                ZOBRIST.psq[captured as usize][rfrom.idx()] ^ ZOBRIST.psq[captured as usize][rto.idx()];
            captured = Piece::NoPiece;
        } else if captured != Piece::NoPiece {
            let mut capsq = to;
            if type_of(captured) == PieceType::Pawn {
                if m.move_type() == MoveType::EnPassant {
                    capsq = unsafe { capsq.add_dir_unchecked(pawn_push(!us)) };
                    let dts = dirties.as_mut().map(|d| &mut d.dirty_threats);
                    self.remove_piece(capsq, dts);
                }
                new_st.pawn_key ^= ZOBRIST.psq[captured as usize][capsq.idx()];
            } else {
                new_st.non_pawn_material[them as usize] -= PIECE_VALUE[captured as usize];
                new_st.non_pawn_key[them as usize] ^= ZOBRIST.psq[captured as usize][capsq.idx()];

                if type_of(captured) <= PieceType::Bishop {
                    new_st.minor_piece_key ^= ZOBRIST.psq[captured as usize][capsq.idx()];
                }
            }

            if let Some(ref mut d) = dirties {
                d.dirty_piece.remove_pc = captured;
                d.dirty_piece.remove_sq = capsq;
            }

            k ^= ZOBRIST.psq[captured as usize][capsq.idx()];
            new_st.material_key ^= ZOBRIST.psq[captured as usize]
                [(8 + self.piece_count[captured as usize] as usize - (m.move_type() != MoveType::EnPassant) as usize) & 63];
            new_st.rule50 = 0;
        } else {
            if let Some(ref mut d) = dirties {
                d.dirty_piece.remove_sq = Square::SqNone;
            }
        }

        let psq_from_to = ZOBRIST.psq[pc as usize][from.idx()] ^ ZOBRIST.psq[pc as usize][to.idx()];
        k ^= psq_from_to;

        if new_st.ep_square != Square::SqNone {
            k ^= ZOBRIST.enpassant[file_of(new_st.ep_square) as usize];
            new_st.ep_square = Square::SqNone;
        }

        k ^= ZOBRIST.castling[new_st.castling_rights as usize & 15];
        new_st.castling_rights &= !(self.castling_rights_mask[from.idx()] | self.castling_rights_mask[to.idx()]);
        k ^= ZOBRIST.castling[new_st.castling_rights as usize & 15];

        if type_of(pc) == PieceType::Pawn {
            if ((to as i32) ^ (from as i32)) == 16 {
                let ep_square = unsafe { to.add_dir_unchecked(pawn_push(!us)) };
                let pawns = pawn_attacks_bb(us, square_bb(ep_square)) & self.pieces_c_pt(them, PieceType::Pawn);
                if pawns != 0 {
                    let ksq = self.square(them, PieceType::King);
                    let not_blockers = !unsafe { self.state_stack.get_unchecked(prev_idx) }.blockers_for_king[them as usize];
                    let no_discovery = (square_bb(from) & not_blockers) != 0 || file_of(from) == file_of(ksq);
                    if no_discovery && (pawns & (not_blockers | line_bb(ep_square, ksq))) != 0 {
                        new_st.ep_square = ep_square;
                        k ^= ZOBRIST.enpassant[file_of(ep_square) as usize];
                    }
                }
            } else if m.move_type() == MoveType::Promotion {
                let pt = m.promotion_type();
                let promotion = make_piece(us, pt);

                if let Some(ref mut d) = dirties {
                    d.dirty_piece.add_pc = promotion;
                    d.dirty_piece.add_sq = to;
                    d.dirty_piece.to = Square::SqNone;
                }

                k ^= ZOBRIST.psq[promotion as usize][to.idx()];
                new_st.material_key ^= ZOBRIST.psq[promotion as usize]
                    [(8 + self.piece_count[promotion as usize] as usize) & 63]
                    ^ ZOBRIST.psq[pc as usize][(8 + self.piece_count[pc as usize] as usize - 1) & 63];
                new_st.non_pawn_key[us as usize] ^= ZOBRIST.psq[promotion as usize][to.idx()];

                if pt <= PieceType::Bishop {
                    new_st.minor_piece_key ^= ZOBRIST.psq[promotion as usize][to.idx()];
                }

                new_st.non_pawn_material[us as usize] += PIECE_VALUE[promotion as usize];
            }

            new_st.pawn_key ^= psq_from_to;
            new_st.rule50 = 0;
        } else {
            new_st.non_pawn_key[us as usize] ^= psq_from_to;
            if type_of(pc) <= PieceType::Bishop {
                new_st.minor_piece_key ^= psq_from_to;
            }
        }

        new_st.key = k;

        if let Some(h) = history {
            h.prefetch_keys(
                new_st.pawn_key,
                new_st.minor_piece_key,
                new_st.non_pawn_key[0],
                new_st.non_pawn_key[1],
                pc,
                to,
            );
        }

        if m.move_type() != MoveType::Castling {
            let to_pc = if m.move_type() == MoveType::Promotion {
                make_piece(us, m.promotion_type())
            } else {
                pc
            };

            let mut dts = dirties.as_mut().map(|d| &mut d.dirty_threats);
            if captured != Piece::NoPiece && m.move_type() != MoveType::EnPassant {
                self.remove_piece(from, dts.as_deref_mut());
                self.swap_piece(to, to_pc, dts);
            } else if pc == to_pc {
                self.move_piece(from, to, dts);
            } else {
                self.remove_piece(from, dts.as_deref_mut());
                self.put_piece(to_pc, to, dts);
            }
        }

        new_st.captured_piece = captured;
        new_st.checkers_bb = if gives_check {
            self.attackers_to(self.square(them, PieceType::King), self.pieces()) & self.pieces_c(us)
        } else {
            0
        };

        if let Some(ref mut d) = dirties {
            d.dirty_pawn_pairs.after[Color::White as usize] = self.pieces_c_pt(Color::White, PieceType::Pawn);
            d.dirty_pawn_pairs.after[Color::Black as usize] = self.pieces_c_pt(Color::Black, PieceType::Pawn);
        }

        self.side_to_move = !self.side_to_move;
        {
            let dst = unsafe { self.state_stack.get_unchecked_mut(self.state_idx) };
            dst.material_key = new_st.material_key;
            dst.pawn_key = new_st.pawn_key;
            dst.minor_piece_key = new_st.minor_piece_key;
            dst.non_pawn_key = new_st.non_pawn_key;
            dst.non_pawn_material = new_st.non_pawn_material;
            dst.castling_rights = new_st.castling_rights;
            dst.rule50 = new_st.rule50;
            dst.plies_from_null = new_st.plies_from_null;
            dst.ep_square = new_st.ep_square;
            dst.key = new_st.key;
            dst.captured_piece = new_st.captured_piece;
            dst.checkers_bb = new_st.checkers_bb;
            pack_pieces_into(&self.board, &mut dst.piece_placement);
        }
        self.set_extra_bitboards();

        // Calculate the repetition info. It is the ply distance from the previous
        // occurrence of the same position, negative in the 3-fold case, or zero
        // if the position was not repeated.
        let mut rep = 0;
        let cur_idx = self.state_idx;
        let cur_st = unsafe { self.state_stack.get_unchecked(cur_idx) };
        let end = std::cmp::min(cur_st.rule50, cur_st.plies_from_null);
        if end >= 4 {
            let cur_key = cur_st.key;
            let cur_placement = cur_st.piece_placement;
            let max_i = std::cmp::min(end as usize, cur_idx);
            let mut i = 4;
            while i <= max_i {
                let prev_st = unsafe { self.state_stack.get_unchecked(cur_idx - i) };
                if prev_st.key == cur_key && prev_st.piece_placement == cur_placement {
                    rep = if prev_st.repetition != 0 { -(i as i32) } else { i as i32 };
                    break;
                }
                i += 2;
            }
        }
        unsafe {
            self.state_stack.get_unchecked_mut(cur_idx).repetition = rep;
        }
    }

    #[inline(never)]
    pub fn undo_move(&mut self, m: Move) {
        debug_assert!(m.is_ok());
        self.side_to_move = !self.side_to_move;
        let us = self.side_to_move;
        let from = m.from_sq();
        let to = m.to_sq();

        if m.move_type() == MoveType::Promotion {
            let pawn = make_piece(us, PieceType::Pawn);
            self.swap_piece(to, pawn, None);
        }

        if m.move_type() == MoveType::Castling {
            let mut rfrom = Square::SqNone;
            let mut rto = Square::SqNone;
            let mut to_sq = to;
            self.do_castling::<false>(us, from, &mut to_sq, &mut rfrom, &mut rto, None, None);
        } else {
            self.move_piece(to, from, None);

            let captured = self.state().captured_piece;
            if captured != Piece::NoPiece {
                let mut capsq = to;
                if m.move_type() == MoveType::EnPassant {
                    capsq = capsq.add_dir(pawn_push(!us));
                }
                self.put_piece(captured, capsq, None);
            }
        }

        self.state_idx -= 1;
        self.game_ply -= 1;
    }

    #[inline(never)]
    pub fn do_null_move(&mut self) {
        debug_assert!(self.checkers() == 0);
        let prev_idx = self.state_idx;
        let new_idx = prev_idx + 1;
        self.state_idx = new_idx;

        let (head, tail) = self.state_stack.split_at_mut(new_idx);
        let src = unsafe { head.get_unchecked(prev_idx) };
        let dst = unsafe { tail.get_unchecked_mut(0) };

        dst.material_key = src.material_key;
        dst.pawn_key = src.pawn_key;
        dst.minor_piece_key = src.minor_piece_key;
        dst.non_pawn_key = src.non_pawn_key;
        dst.non_pawn_material = src.non_pawn_material;
        dst.castling_rights = src.castling_rights;
        dst.rule50 = src.rule50;
        dst.plies_from_null = 0;
        dst.ep_square = Square::SqNone;
        let mut k = src.key ^ ZOBRIST.side;
        if src.ep_square != Square::SqNone {
            k ^= ZOBRIST.enpassant[file_of(src.ep_square) as usize];
        }
        dst.key = k;
        dst.checkers_bb = 0;
        dst.captured_piece = Piece::NoPiece;
        dst.piece_placement = src.piece_placement;
        dst.repetition = 0;

        self.side_to_move = !self.side_to_move;
        self.set_extra_bitboards();
    }

    pub fn undo_null_move(&mut self) {
        debug_assert!(self.checkers() == 0);
        self.state_idx -= 1;

        self.side_to_move = !self.side_to_move;
    }

    #[inline(never)]
    pub fn see_ge(&self, m: Move, threshold: i32) -> bool {
        debug_assert!(m.is_ok());
        if m.move_type() != MoveType::Normal {
            return VALUE_ZERO >= threshold;
        }

        let from = m.from_sq();
        let to = m.to_sq();

        debug_assert!(self.piece_on(from) != Piece::NoPiece);

        let mut swap = PIECE_VALUE[self.piece_on(to) as usize] - threshold;
        if swap < 0 {
            return false;
        }

        swap = PIECE_VALUE[self.piece_on(from) as usize] - swap;
        if swap <= 0 {
            return true;
        }

        debug_assert!(color_of(self.piece_on(from)) == self.side_to_move);
        let mut occupied = self.pieces() ^ square_bb(from) ^ square_bb(to);
        let mut stm = self.side_to_move;
        let mut attackers = self.attackers_to(to, occupied);
        let mut res = 1;

        loop {
            stm = !stm;
            attackers &= occupied;

            let mut stm_attackers = attackers & self.pieces_c(stm);
            if stm_attackers == 0 {
                break;
            }

            if (self.pinners(!stm) & occupied) != 0 {
                stm_attackers &= !self.blockers_for_king(stm);
                if stm_attackers == 0 {
                    break;
                }
            }

            res ^= 1;

            let bb = stm_attackers & self.pieces_pt(PieceType::Pawn);
            if bb != 0 {
                swap = PAWN_VALUE - swap;
                if swap < res {
                    break;
                }
                occupied ^= square_bb(lsb(bb));
                attackers |= crate::attacks::attacks_bb(PieceType::Bishop, to, occupied)
                    & self.pieces_2pt(PieceType::Bishop, PieceType::Queen);
                continue;
            }

            let bb = stm_attackers & self.pieces_pt(PieceType::Knight);
            if bb != 0 {
                swap = KNIGHT_VALUE - swap;
                if swap < res {
                    break;
                }
                occupied ^= square_bb(lsb(bb));
                continue;
            }

            let bb = stm_attackers & self.pieces_pt(PieceType::Bishop);
            if bb != 0 {
                swap = BISHOP_VALUE - swap;
                if swap < res {
                    break;
                }
                occupied ^= square_bb(lsb(bb));
                attackers |= crate::attacks::attacks_bb(PieceType::Bishop, to, occupied)
                    & self.pieces_2pt(PieceType::Bishop, PieceType::Queen);
                continue;
            }

            let bb = stm_attackers & self.pieces_pt(PieceType::Rook);
            if bb != 0 {
                swap = ROOK_VALUE - swap;
                if swap < res {
                    break;
                }
                occupied ^= square_bb(lsb(bb));
                attackers |= crate::attacks::attacks_bb(PieceType::Rook, to, occupied)
                    & self.pieces_2pt(PieceType::Rook, PieceType::Queen);
                continue;
            }

            let bb = stm_attackers & self.pieces_pt(PieceType::Queen);
            if bb != 0 {
                swap = QUEEN_VALUE - swap;
                debug_assert!(swap >= res);
                occupied ^= square_bb(lsb(bb));
                let (bishop_attacks, rook_attacks) = crate::attacks::both_attacks_bb(to, occupied);
                attackers |= (bishop_attacks & self.pieces_2pt(PieceType::Bishop, PieceType::Queen))
                    | (rook_attacks & self.pieces_2pt(PieceType::Rook, PieceType::Queen));
                continue;
            }

            // King
            return if (attackers & !self.pieces_c(stm)) != 0 {
                (res ^ 1) != 0
            } else {
                res != 0
            };
        }

        res != 0
    }

    pub fn is_draw(&self, ply: usize) -> bool {
        if self.state().rule50 > 99 && (self.checkers() == 0 || self.can_move()) {
            return true;
        }
        self.is_repetition(ply)
    }

    pub fn can_move(&self) -> bool {
        use crate::movegen::*;
        let mut list = [Move::none(); MAX_MOVES];
        let count = generate_moves_all(self, &mut list, 0);
        count > 0
    }

    #[inline(always)]
    pub fn is_repetition(&self, ply: usize) -> bool {
        let rep = self.state().repetition;
        rep != 0 && rep < (ply as i32)
    }

    pub fn has_repeated(&self) -> bool {
        let mut curr_idx = self.state_idx;
        let mut end = std::cmp::min(self.state().rule50, self.state().plies_from_null);
        while end >= 4 {
            if self.state_stack[curr_idx].repetition != 0 {
                return true;
            }
            if curr_idx == 0 {
                break;
            }
            curr_idx -= 1;
            end -= 1;
        }
        false
    }

    pub fn upcoming_repetition(&self, ply: usize) -> bool {
        let st = self.state();
        let end = std::cmp::min(st.rule50, st.plies_from_null);
        if end < 3 {
            return false;
        }

        let original_key = st.key;
        let curr_idx = self.state_idx;
        if curr_idx < 1 {
            return false;
        }
        let mut other = original_key ^ self.state_stack[curr_idx - 1].key ^ ZOBRIST.side;

        let mut i = 3;
        while i <= end as usize {
            if curr_idx < i {
                break;
            }
            let prev_st = &self.state_stack[curr_idx - i];
            other ^= self.state_stack[curr_idx - (i - 1)].key ^ prev_st.key ^ ZOBRIST.side;

            if other == 0 {
                let move_key = original_key ^ prev_st.key;
                let mut j = h1(move_key);
                if CUCKOO.keys[j] == move_key || { j = h2(move_key); CUCKOO.keys[j] == move_key } {
                    let m = CUCKOO.moves[j];
                    let s1 = m.from_sq();
                    let s2 = m.to_sq();

                    if ((crate::attacks::between_bb(s1, s2) ^ square_bb(s2)) & self.pieces()) == 0 {
                        let mut b = st.piece_placement;
                        swap_nibble(&mut b, s1, s2);

                        if b == prev_st.piece_placement {
                            if (ply as usize) > i || prev_st.repetition != 0 {
                                return true;
                            }
                        }
                    }
                }
            }
            i += 2;
        }
        false
    }
}

#[inline(always)]
pub fn swap_nibble(p: &mut [u8; 32], s1: Square, s2: Square) {
    let s1 = s1 as usize;
    let s2 = s2 as usize;
    let i1 = s1 & 31;
    let sh1 = (s1 & 0x20) >> 3;
    let i2 = s2 & 31;
    let sh2 = (s2 & 0x20) >> 3;
    let swap = ((p[i1] >> sh1) ^ (p[i2] >> sh2)) & 0xF;
    p[i1] ^= swap << sh1;
    p[i2] ^= swap << sh2;
}

#[inline(always)]
const fn h1(h: Key) -> usize {
    (h & 0x1fff) as usize
}

#[inline(always)]
const fn h2(h: Key) -> usize {
    ((h >> 16) & 0x1fff) as usize
}

pub struct CuckooTables {
    pub keys: [Key; 8192],
    pub moves: [Move; 8192],
    pub count: usize,
}

const fn generate_cuckoo() -> CuckooTables {
    let mut c = [0u64; 8192];
    let mut m = [Move::none(); 8192];
    let mut count = 0;

    let pieces = [
        Piece::WKnight, Piece::WBishop, Piece::WRook, Piece::WQueen, Piece::WKing,
        Piece::BKnight, Piece::BBishop, Piece::BRook, Piece::BQueen, Piece::BKing,
    ];

    let mut pi = 0;
    while pi < pieces.len() {
        let pc = pieces[pi];
        let pt = type_of(pc);
        let mut s1_u = 0;
        while s1_u < 64 {
            let s1 = Square::from_usize(s1_u);
            let mut s2_u = s1_u + 1;
            while s2_u < 64 {
                let s2 = Square::from_usize(s2_u);
                if (crate::attacks::PSEUDO_ATTACKS[pt as usize][s1_u] & square_bb(s2)) != 0 {
                    let mut move_ = Move::new(s1, s2);
                    let mut key = ZOBRIST.psq[pc as usize][s1_u]
                        ^ ZOBRIST.psq[pc as usize][s2_u]
                        ^ ZOBRIST.side;
                    let mut i = h1(key);
                    loop {
                        let tmp_k = c[i];
                        c[i] = key;
                        key = tmp_k;
                        let tmp_m = m[i];
                        m[i] = move_;
                        move_ = tmp_m;
                        if move_.raw() == 0 {
                            break;
                        }
                        i = if i == h1(key) { h2(key) } else { h1(key) };
                    }
                    count += 1;
                }
                s2_u += 1;
            }
            s1_u += 1;
        }
        pi += 1;
    }
    CuckooTables { keys: c, moves: m, count }
}

pub static CUCKOO: CuckooTables = generate_cuckoo();

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cuckoo_count() {
        assert_eq!(CUCKOO.count, 3668);
    }
}
