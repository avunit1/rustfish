use crate::types::*;
use std::time::Instant;

static START_INSTANT: std::sync::LazyLock<Instant> = std::sync::LazyLock::new(Instant::now);

#[inline(always)]
pub fn now() -> TimePoint {
    START_INSTANT.elapsed().as_millis() as TimePoint
}

pub const NO_BOUND: TimePoint = TimePoint::MAX / 2;

#[derive(Clone, Debug)]
pub struct LimitsType {
    pub searchmoves: Vec<Move>,
    pub time: [TimePoint; COLOR_NB],
    pub inc: [TimePoint; COLOR_NB],
    pub npmsec: TimePoint,
    pub movetime: TimePoint,
    pub start_time: TimePoint,
    pub movestogo: i32,
    pub depth: Depth,
    pub mate: i32,
    pub perft: i32,
    pub infinite: bool,
    pub nodes: u64,
    pub ponder_mode: bool,
}

impl Default for LimitsType {
    fn default() -> Self {
        Self {
            searchmoves: Vec::new(),
            time: [0; COLOR_NB],
            inc: [0; COLOR_NB],
            npmsec: 0,
            movetime: 0,
            start_time: 0,
            movestogo: 0,
            depth: 0,
            mate: 0,
            perft: 0,
            infinite: false,
            nodes: 0,
            ponder_mode: false,
        }
    }
}

impl LimitsType {
    #[inline(always)]
    pub fn use_time_management(&self) -> bool {
        self.time[Color::White as usize] != 0 || self.time[Color::Black as usize] != 0
    }
}

pub struct TimeManagement {
    pub start_time: TimePoint,
    pub optimum_time: TimePoint,
    pub maximum_time: TimePoint,
    pub use_nodes_time: bool,
    pub available_nodes: i64,
    pub previous_moves_to_go: i32,
    pub cyclic_budget: i64,
}

impl Default for TimeManagement {
    fn default() -> Self {
        Self {
            start_time: 0,
            optimum_time: NO_BOUND,
            maximum_time: NO_BOUND,
            use_nodes_time: false,
            available_nodes: -1,
            previous_moves_to_go: 0,
            cyclic_budget: 0,
        }
    }
}

impl TimeManagement {
    pub fn new() -> Self {
        Self::default()
    }

    #[inline(always)]
    pub fn optimum(&self) -> TimePoint {
        self.optimum_time
    }

    #[inline(always)]
    pub fn maximum(&self) -> TimePoint {
        self.maximum_time
    }

    #[inline(always)]
    pub fn elapsed_time(&self) -> TimePoint {
        now() - self.start_time
    }

    #[inline(always)]
    pub fn elapsed<F: Fn() -> u64>(&self, nodes: F) -> TimePoint {
        if self.use_nodes_time {
            nodes() as TimePoint
        } else {
            self.elapsed_time()
        }
    }

    pub fn clear(&mut self) {
        self.available_nodes = -1;
        self.previous_moves_to_go = 0;
    }

    pub fn advance_nodes_time(&mut self, nodes: i64) {
        debug_assert!(self.use_nodes_time);
        self.available_nodes = 0.max(self.available_nodes - nodes);
    }

    pub fn init(
        &mut self,
        limits: &mut LimitsType,
        us: Color,
        ply: usize,
        nodestime: i64,
        move_overhead: i64,
        ponder: bool,
        original_time_adjust: &mut f64,
    ) {
        let npmsec = nodestime;
        self.start_time = limits.start_time;
        self.use_nodes_time = npmsec != 0;

        if self.use_nodes_time {
            limits.movetime *= npmsec;
        }

        if limits.time[us as usize] == 0 {
            self.optimum_time = NO_BOUND;
            self.maximum_time = NO_BOUND;
            return;
        }

        let mut move_overhead = move_overhead;
        let opt_scale: f64;
        let max_scale: f64;

        if self.use_nodes_time {
            if self.available_nodes == -1 {
                self.available_nodes = npmsec * limits.time[us as usize];
                self.cyclic_budget = npmsec * (limits.time[us as usize] - limits.inc[us as usize]);
            } else if limits.movestogo > 0 && limits.movestogo > self.previous_moves_to_go && self.cyclic_budget > 0 {
                self.available_nodes += self.cyclic_budget;
            }

            self.previous_moves_to_go = limits.movestogo;

            limits.time[us as usize] = self.available_nodes;
            limits.inc[us as usize] *= npmsec;
            limits.npmsec = npmsec;
            move_overhead *= npmsec;
        }

        let scale_factor = if self.use_nodes_time { npmsec } else { 1 };
        let scaled_time = 1.max(limits.time[us as usize] / scale_factor);

        let mut mtg = if limits.movestogo != 0 {
            limits.movestogo.min(50)
        } else {
            50
        };

        if scaled_time < 1000 && limits.movestogo == 0 {
            mtg = (scaled_time as f64 * 0.05) as i32;
        }

        let time_left = 1.max(
            limits.time[us as usize] + limits.inc[us as usize] * (mtg as i64 - 1)
                - move_overhead * (2 + mtg as i64),
        );

        if limits.movestogo == 0 {
            if *original_time_adjust < 0.0 {
                *original_time_adjust = 0.3272 * (time_left as f64).log10() - 0.4141;
            }

            let log_time_in_sec = (scaled_time as f64 / 1000.0).log10();
            let opt_constant = (0.0029869 + 0.00033554 * log_time_in_sec).min(0.004905);
            let max_constant = (3.3744 + 3.0608 * log_time_in_sec).max(3.1441);

            opt_scale = (0.012112 + (ply as f64 + 3.22713).powf(0.46866) * opt_constant)
                .min(0.19404 * limits.time[us as usize] as f64 / time_left as f64)
                * *original_time_adjust;

            max_scale = (6.873_f64).min(max_constant + ply as f64 / 12.352);
        } else {
            opt_scale = ((0.88 + ply as f64 / 116.4) / mtg as f64)
                .min(0.88 * limits.time[us as usize] as f64 / time_left as f64);
            max_scale = 1.3 + 0.11 * mtg as f64;
        }

        let mut final_opt_scale = opt_scale;
        if !self.use_nodes_time && limits.movestogo != 1 {
            let them = !us;
            let time_advantage = (limits.time[us as usize] - limits.time[them as usize]) as f64
                / (1.0 + (limits.time[us as usize] + limits.time[them as usize]) as f64);
            final_opt_scale *= 1.0 + 0.9 * time_advantage.min(0.0);
        }

        self.optimum_time = (1.0_f64).max(final_opt_scale * time_left as f64) as TimePoint;
        self.maximum_time = (self.optimum_time as f64).max(
            (0.8097 * limits.time[us as usize] as f64 - move_overhead as f64).min(max_scale * self.optimum_time as f64),
        ) as TimePoint;

        if ponder {
            self.optimum_time += self.optimum_time / 4;
        }
    }
}
