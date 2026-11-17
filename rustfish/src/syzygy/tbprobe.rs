use crate::types::*;
use crate::position::Position;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Config {
    pub cardinality: i32,
    pub root_in_tb: bool,
    pub use_rule50: bool,
    pub probe_depth: Depth,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i8)]
pub enum WDLScore {
    WDLLoss = -2,
    WDLBlessedLoss = -1,
    WDLDraw = 0,
    WDLCursedWin = 1,
    WDLWin = 2,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i8)]
pub enum ProbeState {
    Fail = 0,
    Ok = 1,
    ChangeStm = -1,
    ZeroingBestMove = 2,
}

pub static mut MAX_CARDINALITY: i32 = 0;

pub fn init(_paths: &str) {
    // Syzygy path initialization
}

pub fn probe_wdl(_pos: &Position, result: &mut ProbeState) -> WDLScore {
    *result = ProbeState::Fail;
    WDLScore::WDLDraw
}

pub fn probe_dtz(_pos: &Position, result: &mut ProbeState) -> i32 {
    *result = ProbeState::Fail;
    0
}

