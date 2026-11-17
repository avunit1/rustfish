use crate::movegen::*;
use crate::position::*;

pub fn perft<const ROOT: bool>(pos: &mut Position, depth: usize) -> u64 {
    if depth == 0 {
        return 1;
    }

    let mut nodes = 0u64;
    let leaf = depth == 2;

    let move_list = MoveList::new_all(pos);

    for i in 0..move_list.count {
        let m = move_list.moves[i];

        if ROOT && depth <= 1 {
            nodes += 1;
        } else {
            let gives_check = pos.gives_check(m);
            pos.do_move(m, gives_check, None);
            let cnt = if leaf {
                MoveList::new_all(pos).count as u64
            } else {
                perft::<false>(pos, depth - 1)
            };
            nodes += cnt;
            pos.undo_move(m);
        }
    }

    nodes
}

pub fn run_perft(fen: &str, depth: usize, is_chess960: bool) -> Result<u64, String> {
    let mut pos = Position::new();
    pos.set(fen, is_chess960)?;
    Ok(perft::<true>(&mut pos, depth))
}
