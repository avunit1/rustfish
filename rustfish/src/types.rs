use std::fmt;

pub type Key = u64;
pub type Bitboard = u64;

pub const MAX_MOVES: usize = 256;
pub const MAX_PLY: usize = 246;

pub const START_FEN: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum Color {
    White = 0,
    Black = 1,
}

pub const COLOR_NB: usize = 2;

impl Color {
    #[inline(always)]
    pub const fn from_u8(v: u8) -> Self {
        if v == 0 {
            Color::White
        } else {
            Color::Black
        }
    }

    #[inline(always)]
    pub const fn other(self) -> Self {
        match self {
            Color::White => Color::Black,
            Color::Black => Color::White,
        }
    }
}

impl std::ops::Not for Color {
    type Output = Color;
    #[inline(always)]
    fn not(self) -> Self::Output {
        self.other()
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
#[repr(u8)]
pub enum CastlingRights {
    NoCastling = 0,
    WhiteOO = 1,
    WhiteOOO = 2,
    BlackOO = 4,
    BlackOOO = 8,
    KingSide = 5,
    QueenSide = 10,
    WhiteCastling = 3,
    BlackCastling = 12,
    AnyCastling = 15,
}

pub const CASTLING_RIGHT_NB: usize = 16;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum Bound {
    None = 0,
    Upper = 1,
    Lower = 2,
    Exact = 3,
}

impl Bound {
    #[inline(always)]
    pub const fn from_u8(v: u8) -> Self {
        match v & 3 {
            1 => Bound::Upper,
            2 => Bound::Lower,
            3 => Bound::Exact,
            _ => Bound::None,
        }
    }
}

pub type Value = i32;

pub const VALUE_ZERO: Value = 0;
pub const VALUE_DRAW: Value = 0;
pub const VALUE_NONE: Value = 32002;
pub const VALUE_INFINITE: Value = 32001;

pub const VALUE_MATE: Value = 32000;
pub const VALUE_MATE_IN_MAX_PLY: Value = VALUE_MATE - MAX_PLY as Value;
pub const VALUE_MATED_IN_MAX_PLY: Value = -VALUE_MATE_IN_MAX_PLY;

pub const VALUE_TB: Value = VALUE_MATE_IN_MAX_PLY - 1;
pub const VALUE_TB_WIN_IN_MAX_PLY: Value = VALUE_TB - MAX_PLY as Value;
pub const VALUE_TB_LOSS_IN_MAX_PLY: Value = -VALUE_TB_WIN_IN_MAX_PLY;

#[inline(always)]
pub const fn is_valid(value: Value) -> bool {
    value != VALUE_NONE
}

#[inline(always)]
pub const fn is_win(value: Value) -> bool {
    value >= VALUE_TB_WIN_IN_MAX_PLY
}

#[inline(always)]
pub const fn is_loss(value: Value) -> bool {
    value <= VALUE_TB_LOSS_IN_MAX_PLY
}

#[inline(always)]
pub const fn is_decisive(value: Value) -> bool {
    is_win(value) || is_loss(value)
}

#[inline(always)]
pub const fn is_mate(value: Value) -> bool {
    value >= VALUE_MATE_IN_MAX_PLY
}

#[inline(always)]
pub const fn is_mated(value: Value) -> bool {
    value <= VALUE_MATED_IN_MAX_PLY
}

#[inline(always)]
pub const fn is_mate_or_mated(value: Value) -> bool {
    is_mate(value) || is_mated(value)
}

#[inline(always)]
pub const fn mate_in(ply: usize) -> Value {
    VALUE_MATE - (ply as Value)
}

#[inline(always)]
pub const fn mated_in(ply: usize) -> Value {
    -VALUE_MATE + (ply as Value)
}

pub const PAWN_VALUE: Value = 208;
pub const KNIGHT_VALUE: Value = 781;
pub const BISHOP_VALUE: Value = 825;
pub const ROOK_VALUE: Value = 1276;
pub const QUEEN_VALUE: Value = 2538;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
#[repr(u8)]
pub enum PieceType {
    NoPieceType = 0,
    Pawn = 1,
    Knight = 2,
    Bishop = 3,
    Rook = 4,
    Queen = 5,
    King = 6,
    PieceTypeNb = 7,
}

impl PieceType {
    #[allow(non_upper_case_globals)]
    pub const AllPieces: PieceType = PieceType::NoPieceType;
}

pub const PIECE_TYPE_NB: usize = 8;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
#[repr(u8)]
pub enum Piece {
    #[default]
    NoPiece = 0,
    WPawn = 1,
    WKnight = 2,
    WBishop = 3,
    WRook = 4,
    WQueen = 5,
    WKing = 6,
    Piece7 = 7,
    BNoPiece = 8,
    BPawn = 9,
    BKnight = 10,
    BBishop = 11,
    BRook = 12,
    BQueen = 13,
    BKing = 14,
    Piece15 = 15,
}

pub const PIECE_NB: usize = 16;

pub const PIECE_VALUE: [Value; PIECE_NB] = [
    VALUE_ZERO,
    PAWN_VALUE,
    KNIGHT_VALUE,
    BISHOP_VALUE,
    ROOK_VALUE,
    QUEEN_VALUE,
    VALUE_ZERO,
    VALUE_ZERO,
    VALUE_ZERO,
    PAWN_VALUE,
    KNIGHT_VALUE,
    BISHOP_VALUE,
    ROOK_VALUE,
    QUEEN_VALUE,
    VALUE_ZERO,
    VALUE_ZERO,
];

pub type Depth = i32;
pub type TimePoint = i64;

pub const DEPTH_QS: Depth = 0;
pub const DEPTH_UNSEARCHED: Depth = -2;
pub const DEPTH_NONE: Depth = -3;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Default)]
#[repr(u8)]
pub enum Square {
    SqA1, SqB1, SqC1, SqD1, SqE1, SqF1, SqG1, SqH1,
    SqA2, SqB2, SqC2, SqD2, SqE2, SqF2, SqG2, SqH2,
    SqA3, SqB3, SqC3, SqD3, SqE3, SqF3, SqG3, SqH3,
    SqA4, SqB4, SqC4, SqD4, SqE4, SqF4, SqG4, SqH4,
    SqA5, SqB5, SqC5, SqD5, SqE5, SqF5, SqG5, SqH5,
    SqA6, SqB6, SqC6, SqD6, SqE6, SqF6, SqG6, SqH6,
    SqA7, SqB7, SqC7, SqD7, SqE7, SqF7, SqG7, SqH7,
    SqA8, SqB8, SqC8, SqD8, SqE8, SqF8, SqG8, SqH8,
    #[default]
    SqNone,
}

pub const SQUARE_ZERO: Square = Square::SqA1;
pub const SQUARE_NB: usize = 64;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(i8)]
pub enum Direction {
    North = 8,
    East = 1,
    South = -8,
    West = -1,
    NorthEast = 9,
    SouthEast = -7,
    SouthWest = -9,
    NorthWest = 7,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
#[repr(u8)]
pub enum File {
    FileA = 0,
    FileB = 1,
    FileC = 2,
    FileD = 3,
    FileE = 4,
    FileF = 5,
    FileG = 6,
    FileH = 7,
}

pub const FILE_NB: usize = 8;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
#[repr(u8)]
pub enum Rank {
    Rank1 = 0,
    Rank2 = 1,
    Rank3 = 2,
    Rank4 = 3,
    Rank5 = 4,
    Rank6 = 5,
    Rank7 = 6,
    Rank8 = 7,
}

pub const RANK_NB: usize = 8;

#[inline(always)]
pub const fn make_square(f: File, r: Rank) -> Square {
    unsafe { Square::from_u8_unchecked(((r as u8) << 3) + (f as u8)) }
}

#[inline(always)]
pub const fn make_piece(c: Color, pt: PieceType) -> Piece {
    Piece::from_u8(((c as u8) << 3) + (pt as u8))
}

#[inline(always)]
pub const fn type_of(pc: Piece) -> PieceType {
    PieceType::from_u8((pc as u8) & 7)
}

#[inline(always)]
pub const fn color_of(pc: Piece) -> Color {
    Color::from_u8((pc as u8) >> 3)
}

#[inline(always)]
pub const fn is_ok(s: Square) -> bool {
    (s as u8) <= (Square::SqH8 as u8)
}

#[inline(always)]
pub const fn file_of(s: Square) -> File {
    File::from_u8((s as u8) & 7)
}

#[inline(always)]
pub const fn rank_of(s: Square) -> Rank {
    Rank::from_u8((s as u8) >> 3)
}

#[inline(always)]
pub const fn relative_square(c: Color, s: Square) -> Square {
    unsafe { Square::from_u8_unchecked(((s as u8) ^ ((c as u8) * 56)) & 63) }
}

#[inline(always)]
pub const fn relative_rank(c: Color, r: Rank) -> Rank {
    Rank::from_u8((r as u8) ^ ((c as u8) * 7))
}

#[inline(always)]
pub const fn relative_rank_of_sq(c: Color, s: Square) -> Rank {
    relative_rank(c, rank_of(s))
}

#[inline(always)]
pub const fn flip_rank(s: Square) -> Square {
    unsafe { Square::from_u8_unchecked(((s as u8) ^ (Square::SqA8 as u8)) & 63) }
}

#[inline(always)]
pub const fn flip_file(s: Square) -> Square {
    unsafe { Square::from_u8_unchecked(((s as u8) ^ (Square::SqH1 as u8)) & 63) }
}

#[inline(always)]
pub const fn pawn_push(c: Color) -> Direction {
    match c {
        Color::White => Direction::North,
        Color::Black => Direction::South,
    }
}

impl Square {
    #[allow(non_upper_case_globals)]
    pub const None: Square = Square::SqNone;

    #[inline(always)]
    pub const fn from_u8(v: u8) -> Self {
        if v < 64 {
            unsafe { std::mem::transmute(v) }
        } else {
            Square::SqNone
        }
    }

    #[inline(always)]
    pub const unsafe fn from_u8_unchecked(v: u8) -> Self {
        std::mem::transmute(v)
    }

    #[inline(always)]
    pub const fn idx(self) -> usize {
        (self as usize) & 63
    }

    #[inline(always)]
    pub const fn from_usize(v: usize) -> Self {
        Self::from_u8(v as u8)
    }

    #[inline(always)]
    pub const fn add_dir(self, d: Direction) -> Square {
        let val = (self as i8).wrapping_add(d as i8);
        if val >= 0 && val < 64 {
            unsafe { Square::from_u8_unchecked(val as u8) }
        } else {
            Square::SqNone
        }
    }

    #[inline(always)]
    pub const unsafe fn add_dir_unchecked(self, d: Direction) -> Square {
        Square::from_u8_unchecked((self as i8).wrapping_add(d as i8) as u8)
    }
}

impl Color {
    #[inline(always)]
    pub const fn from_usize(v: usize) -> Self {
        Self::from_u8(v as u8)
    }
}

impl File {
    #[inline(always)]
    pub const fn from_u8(v: u8) -> Self {
        unsafe { std::mem::transmute(v & 7) }
    }

    #[inline(always)]
    pub const fn from_usize(v: usize) -> Self {
        Self::from_u8(v as u8)
    }
}

impl Rank {
    #[inline(always)]
    pub const fn from_u8(v: u8) -> Self {
        unsafe { std::mem::transmute(v & 7) }
    }

    #[inline(always)]
    pub const fn from_usize(v: usize) -> Self {
        Self::from_u8(v as u8)
    }
}

impl PieceType {
    #[inline(always)]
    pub const fn from_u8(v: u8) -> Self {
        unsafe { std::mem::transmute(v & 7) }
    }

    #[inline(always)]
    pub const fn from_usize(v: usize) -> Self {
        Self::from_u8(v as u8)
    }
}

impl Piece {
    #[allow(non_upper_case_globals)]
    pub const None: Piece = Piece::NoPiece;

    #[inline(always)]
    pub const fn make(c: Color, pt: PieceType) -> Piece {
        make_piece(c, pt)
    }

    #[inline(always)]
    pub const fn from_u8(v: u8) -> Self {
        unsafe { std::mem::transmute(v & 15) }
    }

    #[inline(always)]
    pub const fn from_usize(v: usize) -> Self {
        Self::from_u8(v as u8)
    }

    #[inline(always)]
    pub const fn type_of(self) -> PieceType {
        type_of(self)
    }

    #[inline(always)]
    pub const fn color_of(self) -> Color {
        color_of(self)
    }

    #[inline(always)]
    pub const fn flip_color(self) -> Piece {
        Piece::from_u8((self as u8) ^ 8)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u16)]
pub enum MoveType {
    Normal = 0,
    Promotion = 1 << 14,
    EnPassant = 2 << 14,
    Castling = 3 << 14,
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub struct Move {
    pub data: u16,
}

impl Move {
    #[inline(always)]
    pub const fn from_raw(d: u16) -> Self {
        Move { data: d }
    }

    #[inline(always)]
    pub const fn new(from: Square, to: Square) -> Self {
        Move {
            data: ((from as u16) << 6) + (to as u16),
        }
    }

    #[inline(always)]
    pub const fn make(from: Square, to: Square, mt: MoveType, pt: PieceType) -> Self {
        let promo_bits = if (mt as u16) == (MoveType::Promotion as u16) {
            ((pt as u16).wrapping_sub(PieceType::Knight as u16)) << 12
        } else {
            0
        };
        Move {
            data: (mt as u16) + promo_bits + ((from as u16) << 6) + (to as u16),
        }
    }

    #[inline(always)]
    pub const fn from_sq(self) -> Square {
        unsafe { Square::from_u8_unchecked(((self.data >> 6) & 0x3F) as u8) }
    }

    #[inline(always)]
    pub const fn to_sq(self) -> Square {
        unsafe { Square::from_u8_unchecked((self.data & 0x3F) as u8) }
    }

    #[inline(always)]
    pub const fn move_type(self) -> MoveType {
        match self.data & (3 << 14) {
            0 => MoveType::Normal,
            0x4000 => MoveType::Promotion,
            0x8000 => MoveType::EnPassant,
            _ => MoveType::Castling,
        }
    }

    #[inline(always)]
    pub const fn promotion_type(self) -> PieceType {
        PieceType::from_u8((((self.data >> 12) & 3) + (PieceType::Knight as u16)) as u8)
    }

    #[inline(always)]
    pub const fn is_ok(self) -> bool {
        self.data != Self::none().data && self.data != Self::null().data
    }

    #[inline(always)]
    pub const fn none() -> Self {
        Move { data: 0 }
    }

    #[inline(always)]
    pub const fn null() -> Self {
        Move { data: 65 }
    }

    #[inline(always)]
    pub const fn raw(self) -> u16 {
        self.data
    }
}

impl fmt::Debug for Move {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if *self == Move::none() {
            write!(f, "(none)")
        } else if *self == Move::null() {
            write!(f, "0000")
        } else {
            write!(f, "{:?}{:?}", self.from_sq(), self.to_sq())
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub struct ExtMove {
    pub mov: Move,
    pub value: i32,
}

impl PartialOrd for ExtMove {
    #[inline(always)]
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ExtMove {
    #[inline(always)]
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.value.cmp(&other.value)
    }
}

#[inline(always)]
pub const fn make_key(seed: u64) -> Key {
    seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407)
}

#[derive(Clone, Copy, Default, Debug)]
pub struct DirtyPiece {
    pub pc: Piece,
    pub from: Square,
    pub to: Square,
    pub remove_sq: Square,
    pub add_sq: Square,
    pub remove_pc: Piece,
    pub add_pc: Piece,
}

#[derive(Clone, Copy, Default, Debug)]
pub struct DirtyThreat {
    pub data: u32,
}

impl DirtyThreat {
    pub const PC_SQ_OFFSET: usize = 0;
    pub const THREATENED_SQ_OFFSET: usize = 8;
    pub const THREATENED_PC_OFFSET: usize = 16;
    pub const PC_OFFSET: usize = 20;

    #[inline(always)]
    pub const fn from_raw(raw: u32) -> Self {
        DirtyThreat { data: raw }
    }

    #[inline(always)]
    pub const fn new(
        pc: Piece,
        threatened_pc: Piece,
        pc_sq: Square,
        threatened_sq: Square,
        add: bool,
    ) -> Self {
        let d = ((add as u32) << 31)
            | ((pc as u32) << Self::PC_OFFSET)
            | ((threatened_pc as u32) << Self::THREATENED_PC_OFFSET)
            | ((threatened_sq as u32) << Self::THREATENED_SQ_OFFSET)
            | ((pc_sq as u32) << Self::PC_SQ_OFFSET);
        DirtyThreat { data: d }
    }

    #[inline(always)]
    pub const fn pc(self) -> Piece {
        Piece::from_u8(((self.data >> Self::PC_OFFSET) & 0xF) as u8)
    }

    #[inline(always)]
    pub const fn threatened_pc(self) -> Piece {
        Piece::from_u8(((self.data >> Self::THREATENED_PC_OFFSET) & 0xF) as u8)
    }

    #[inline(always)]
    pub const fn threatened_sq(self) -> Square {
        Square::from_u8(((self.data >> Self::THREATENED_SQ_OFFSET) & 0xFF) as u8)
    }

    #[inline(always)]
    pub const fn pc_sq(self) -> Square {
        Square::from_u8(((self.data >> Self::PC_SQ_OFFSET) & 0xFF) as u8)
    }

    #[inline(always)]
    pub const fn add(self) -> bool {
        (self.data >> 31) != 0
    }

    #[inline(always)]
    pub const fn raw(self) -> u32 {
        self.data
    }
}

pub const DIRTY_THREAT_LIST_MAX: usize = 96;

#[derive(Clone, Copy)]
pub struct ValueList<T: Copy, const MAX_SIZE: usize> {
    values: [core::mem::MaybeUninit<T>; MAX_SIZE],
    pub size: usize,
}

impl<T: Copy, const MAX_SIZE: usize> ValueList<T, MAX_SIZE> {
    #[inline(always)]
    pub const fn new() -> Self {
        Self {
            values: [const { core::mem::MaybeUninit::uninit() }; MAX_SIZE],
            size: 0,
        }
    }

    #[inline(always)]
    pub fn clear(&mut self) {
        self.size = 0;
    }

    #[inline(always)]
    pub fn push(&mut self, val: T) {
        debug_assert!(self.size < MAX_SIZE);
        unsafe {
            self.values.get_unchecked_mut(self.size).write(val);
        }
        self.size += 1;
    }

    #[inline(always)]
    pub fn push_if_lt(&mut self, val_usize: usize, bound: usize)
    where
        T: From<u16>,
    {
        debug_assert!(self.size < MAX_SIZE);
        unsafe {
            self.values.get_unchecked_mut(self.size).write(T::from(val_usize as u16));
        }
        self.size += (val_usize < bound) as usize;
    }

    #[inline(always)]
    pub fn len(&self) -> usize {
        self.size
    }

    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.size == 0
    }

    #[inline(always)]
    pub fn as_slice(&self) -> &[T] {
        unsafe { core::slice::from_raw_parts(self.values.as_ptr() as *const T, self.size) }
    }
}

impl<T: Copy, const MAX_SIZE: usize> Default for ValueList<T, MAX_SIZE> {
    #[inline(always)]
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Copy, const MAX_SIZE: usize> core::ops::Index<usize> for ValueList<T, MAX_SIZE> {
    type Output = T;
    #[inline(always)]
    fn index(&self, index: usize) -> &Self::Output {
        debug_assert!(index < self.size);
        unsafe { self.values.get_unchecked(index).assume_init_ref() }
    }
}

impl<T: Copy, const MAX_SIZE: usize> core::ops::IndexMut<usize> for ValueList<T, MAX_SIZE> {
    #[inline(always)]
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        debug_assert!(index < self.size);
        unsafe { self.values.get_unchecked_mut(index).assume_init_mut() }
    }
}

pub type DirtyThreatList = ValueList<DirtyThreat, DIRTY_THREAT_LIST_MAX>;

#[derive(Clone, Copy, Default)]
pub struct DirtyThreats {
    pub list: DirtyThreatList,
}

#[derive(Clone, Copy, Default, Debug)]
pub struct DirtyPawnPairs {
    pub before: [Bitboard; COLOR_NB],
    pub after: [Bitboard; COLOR_NB],
}

#[derive(Clone, Copy, Default)]
pub struct Dirties {
    pub dirty_piece: DirtyPiece,
    pub dirty_threats: DirtyThreats,
    pub dirty_pawn_pairs: DirtyPawnPairs,
}
