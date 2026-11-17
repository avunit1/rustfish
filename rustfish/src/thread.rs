use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use crate::history::SharedHistories;
use crate::position::Position;
use crate::search::Worker;
use crate::timeman::LimitsType;
use crate::tt::TranspositionTable;
use crate::types::Move;

#[repr(align(64))]
pub struct CachePaddedAtomicU64(pub AtomicU64);

pub struct ThreadPool {
    pub workers: Vec<Worker>,
    pub shared_history: Arc<SharedHistories>,
    pub tt: Arc<TranspositionTable>,
    pub stop: Arc<AtomicBool>,
    pub thread_nodes: Arc<Vec<CachePaddedAtomicU64>>,
    pub threads_count: usize,
}

impl ThreadPool {
    pub fn new(num_threads: usize, tt_size_mb: usize) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let tt = Arc::new(TranspositionTable::new(tt_size_mb));
        let count = num_threads.max(1);
        let shared_history = Arc::new(SharedHistories::new(count));
        let thread_nodes = Arc::new(
            (0..count)
                .map(|_| CachePaddedAtomicU64(AtomicU64::new(0)))
                .collect::<Vec<_>>(),
        );

        let mut workers = Vec::with_capacity(count);
        for i in 0..count {
            workers.push(Worker::new(i, stop.clone(), thread_nodes.clone()));
        }

        let mut pool = Self {
            workers,
            shared_history,
            tt,
            stop,
            thread_nodes,
            threads_count: count,
        };
        pool.clear();
        pool
    }

    pub fn set_threads(&mut self, count: usize) {
        let count = count.max(1);
        if count == self.threads_count {
            return;
        }
        self.threads_count = count;
        self.shared_history = Arc::new(SharedHistories::new(count));
        self.thread_nodes = Arc::new(
            (0..count)
                .map(|_| CachePaddedAtomicU64(AtomicU64::new(0)))
                .collect::<Vec<_>>(),
        );
        self.workers.clear();
        for i in 0..count {
            self.workers.push(Worker::new(i, self.stop.clone(), self.thread_nodes.clone()));
        }
        self.clear();
    }

    pub fn set_hash(&mut self, tt_size_mb: usize) {
        self.tt = Arc::new(TranspositionTable::new(tt_size_mb));
    }

    pub fn clear(&mut self) {
        self.tt.clear();
        let mut sh = Arc::get_mut(&mut self.shared_history);
        for w in &mut self.workers {
            if let Some(ref mut s) = sh {
                w.clear(s);
            } else {
                let mut dummy = SharedHistories::new(1);
                w.clear(&mut dummy);
            }
        }
    }

    pub fn start_thinking(&mut self, pos: &Position, limits: LimitsType) {
        self.stop.store(false, Ordering::Relaxed);
        for tn in self.thread_nodes.iter() {
            tn.0.store(0, Ordering::Relaxed);
        }
        self.tt.new_search();

        if self.threads_count == 1 {
            self.workers[0].setup_search(pos, limits);
            if self.workers[0].root_moves.is_empty() {
                return;
            }
            self.workers[0].iterative_deepening(&self.tt, &self.shared_history);
            self.thread_nodes[0].0.store(self.workers[0].nodes, Ordering::Relaxed);
            return;
        }

        // Multi-threaded Lazy SMP
        for i in 0..self.threads_count {
            self.workers[i].setup_search(pos, limits.clone());
        }

        if self.workers[0].root_moves.is_empty() {
            return;
        }

        let tt = self.tt.clone();
        let shared_history = self.shared_history.clone();
        let stop = self.stop.clone();

        std::thread::scope(|s| {
            let (main_worker, helper_workers) = self.workers.split_at_mut(1);
            for w in helper_workers.iter_mut() {
                let tt_ref = &*tt;
                let sh_ref = &*shared_history;
                std::thread::Builder::new()
                    .stack_size(8 * 1024 * 1024)
                    .spawn_scoped(s, move || {
                        w.iterative_deepening(tt_ref, sh_ref);
                    })
                    .expect("failed to spawn helper thread");
            }

            main_worker[0].iterative_deepening(&tt, &shared_history);
            stop.store(true, Ordering::Relaxed);
        });
    }

    pub fn nodes_searched(&self) -> u64 {
        let mut sum = 0;
        for w in &self.workers {
            sum += w.nodes;
        }
        sum
    }

    pub fn main_worker(&self) -> &Worker {
        &self.workers[0]
    }

    pub fn main_worker_mut(&mut self) -> &mut Worker {
        &mut self.workers[0]
    }

    pub fn best_move(&self) -> Move {
        if self.workers[0].root_moves.is_empty() {
            Move::none()
        } else {
            self.workers[0].root_moves[0].pv[0]
        }
    }
}
