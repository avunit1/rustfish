use crate::nnue::{AccumulatorCaches, AccumulatorStack, Network};
use crate::position::Position;
use crate::types::*;

pub fn simple_eval(pos: &Position) -> Value {
    let c = pos.side_to_move();
    let pawns = pos.count_c_pt(c, PieceType::Pawn) as Value - pos.count_c_pt(!c, PieceType::Pawn) as Value;
    PAWN_VALUE * pawns + pos.non_pawn_material_c(c) - pos.non_pawn_material_c(!c)
}

pub fn scale_evaluation(nnue: Value, optimism: Value, pos: &Position) -> Value {
    let se = simple_eval(pos);

    let se_norm = (se * 1024) / (se.abs() + 1024);
    let nnue_norm = (nnue * 1024) / (nnue.abs() + 1024);
    let correl = (se_norm * nnue_norm) / 512;

    let mut v = nnue + (nnue + optimism) * correl / 65536;

    let material = 521 * pos.count_pt(PieceType::Pawn) as i32 + pos.non_pawn_material();
    v += ((material as i64 * v as i64) / 90649) as i32;

    v -= (v * pos.rule50_count() as i32) / 189;

    v.clamp(VALUE_TB_LOSS_IN_MAX_PLY + 1, VALUE_TB_WIN_IN_MAX_PLY - 1)
}

#[inline(always)]
pub unsafe fn evaluate(
    network: &Network,
    pos: &Position,
    accumulator_stack: &mut AccumulatorStack,
    caches: &mut AccumulatorCaches,
    optimism: Value,
) -> Value {
    debug_assert_eq!(pos.checkers(), 0);
    let nnue = network.evaluate(pos, accumulator_stack, caches);
    scale_evaluation(nnue, optimism, pos)
}

pub fn win_rate_params(pos: &Position) -> (f64, f64) {
    let material = pos.count_pt(PieceType::Pawn)
        + 3 * pos.count_pt(PieceType::Knight)
        + 3 * pos.count_pt(PieceType::Bishop)
        + 5 * pos.count_pt(PieceType::Rook)
        + 9 * pos.count_pt(PieceType::Queen);

    let m = (material.clamp(17, 78) as f64) / 58.0;

    const AS: [f64; 4] = [-142.72052667, 372.35176398, -340.71073572, 415.23490212];
    const BS: [f64; 4] = [5.93832785, 15.61267078, -30.57816876, 69.63866711];

    let a = (((AS[0] * m + AS[1]) * m + AS[2]) * m) + AS[3];
    let b = (((BS[0] * m + BS[1]) * m + BS[2]) * m) + BS[3];

    (a, b)
}

pub fn to_cp(v: Value, pos: &Position) -> i32 {
    let (a, _) = win_rate_params(pos);
    ((100.0 * (v as f64) / a).round()) as i32
}

pub unsafe fn trace(pos: &Position, network: &Network) -> String {
    if pos.checkers() != 0 {
        return "Final evaluation: none (in check)\n".into();
    }

    let mut accumulators = AccumulatorStack::new();
    let mut caches = AccumulatorCaches::new(&network.feature_transformer.biases);

    let raw_nnue = network.evaluate(pos, &mut accumulators, &mut caches);
    let scaled = scale_evaluation(raw_nnue, VALUE_ZERO, pos);

    let white_raw = if pos.side_to_move() == Color::White { raw_nnue } else { -raw_nnue };
    let white_scaled = if pos.side_to_move() == Color::White { scaled } else { -scaled };

    let cp_raw = to_cp(white_raw, pos);
    let cp_scaled = to_cp(white_scaled, pos);

    format!(
        "NNUE evaluation          {} (side to move, internal units)\n\
         NNUE evaluation        {:.2} (white side)\n\
         Final evaluation      {:.2} (white side) [with scaled NNUE, ...]\n",
        raw_nnue,
        0.01 * (cp_raw as f64),
        0.01 * (cp_scaled as f64),
    )
}
