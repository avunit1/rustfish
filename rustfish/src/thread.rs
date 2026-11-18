use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use crate::history::SharedHistories;
use crate::memory::{CachePadded, PageAlignedBox};
use crate::position::Position;
use crate::search::{RootMove, Worker};
use crate::timeman::LimitsType;
use crate::tt::TranspositionTable;
use crate::types::*;

/// Cache-line isolated shared control flags and lock-free worker counter handles.
#[repr(C, align(64))]
pub struct SharedControl {
    pub stop: CachePadded<AtomicBool>,
    pub increase_depth: CachePadded<AtomicBool>,
    pub threads_count: usize,
    worker_ptrs: Vec<AtomicUsize>,
}

unsafe impl Send for SharedControl {}
unsafe impl Sync for SharedControl {}

impl SharedControl {
    pub fn new(threads_count: usize) -> Self {
        let mut worker_ptrs = Vec::with_capacity(threads_count);
        for _ in 0..threads_count {
            worker_ptrs.push(AtomicUsize::new(0));
        }
        Self {
            stop: CachePadded::new(AtomicBool::new(false)),
            increase_depth: CachePadded::new(AtomicBool::new(true)),
            threads_count,
            worker_ptrs,
        }
    }

    pub fn register_worker(&self, idx: usize, ptr: *const Worker) {
        self.worker_ptrs[idx].store(ptr as usize, Ordering::Release);
    }

    #[inline(always)]
    pub fn nodes_searched(&self) -> u64 {
        let mut sum = 0u64;
        for p in &self.worker_ptrs {
            let ptr = p.load(Ordering::Relaxed) as *const Worker;
            if !ptr.is_null() {
                sum += unsafe { (*ptr).nodes.load() };
            }
        }
        sum
    }

    #[inline(always)]
    pub fn tb_hits(&self) -> u64 {
        let mut sum = 0u64;
        for p in &self.worker_ptrs {
            let ptr = p.load(Ordering::Relaxed) as *const Worker;
            if !ptr.is_null() {
                sum += unsafe { (*ptr).tb_hits.load() };
            }
        }
        sum
    }

    #[inline(always)]
    pub fn take_best_move_changes(&self) -> u64 {
        let mut sum = 0u64;
        for p in &self.worker_ptrs {
            let ptr = p.load(Ordering::Relaxed) as *const Worker;
            if !ptr.is_null() {
                unsafe {
                    sum += (*ptr).best_move_changes.load();
                    (*ptr).best_move_changes.store(0);
                }
            }
        }
        sum
    }
}

type HelperJob = Box<dyn FnOnce(&mut Worker) + Send>;

struct HelperState {
    job: Option<HelperJob>,
    busy: bool,
    exit: bool,
}

/// Persistent worker OS thread parked on a Condvar (matching C++ Stockfish `Thread`).
/// Allocates and first-touches its own `PageAlignedBox<Worker>` inside its own OS thread.
struct HelperThread {
    state: Arc<(Mutex<HelperState>, Condvar)>,
    handle: Option<JoinHandle<()>>,
}

impl HelperThread {
    fn spawn(thread_idx: usize, shared_ctrl: Arc<SharedControl>) -> Self {
        let state = Arc::new((
            Mutex::new(HelperState {
                job: None,
                busy: true, // true until Worker is allocated & thread reaches idle_loop
                exit: false,
            }),
            Condvar::new(),
        ));
        let state_clone = state.clone();

        let handle = std::thread::Builder::new()
            .name(format!("rf-worker-{}", thread_idx))
            .stack_size(8 * 1024 * 1024)
            .spawn(move || {
                // First-touch allocation of Worker and all thread-local tables inside this OS thread
                let mut worker = PageAlignedBox::new(Worker::new(thread_idx, shared_ctrl.clone()));
                shared_ctrl.register_worker(thread_idx, worker.as_ptr());

                let (lock, cvar) = &*state_clone;
                loop {
                    let job = {
                        let mut guard = lock.lock().unwrap();
                        guard.busy = false;
                        cvar.notify_one();
                        while !guard.busy && !guard.exit {
                            guard = cvar.wait(guard).unwrap();
                        }
                        if guard.exit {
                            break;
                        }
                        guard.job.take()
                    };
                    if let Some(f) = job {
                        f(&mut worker);
                    }
                }
            })
            .expect("failed to spawn persistent helper thread");

        let helper = Self {
            state,
            handle: Some(handle),
        };
        helper.wait();
        helper
    }

    fn run_job<F: FnOnce(&mut Worker) + Send + 'static>(&self, f: F) {
        let (lock, cvar) = &*self.state;
        let mut guard = lock.lock().unwrap();
        while guard.busy {
            guard = cvar.wait(guard).unwrap();
        }
        guard.job = Some(Box::new(f));
        guard.busy = true;
        drop(guard);
        cvar.notify_one();
    }

    fn wait(&self) {
        let (lock, cvar) = &*self.state;
        let mut guard = lock.lock().unwrap();
        while guard.busy {
            guard = cvar.wait(guard).unwrap();
        }
    }
}

impl Drop for HelperThread {
    fn drop(&mut self) {
        {
            let (lock, cvar) = &*self.state;
            let mut guard = lock.lock().unwrap();
            while guard.busy {
                guard = cvar.wait(guard).unwrap();
            }
            guard.exit = true;
            cvar.notify_one();
        }
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

pub struct ThreadPool {
    // Declared FIRST so helper OS threads are always joined before main_worker / shared state drop.
    helpers: Vec<HelperThread>,
    main_worker: PageAlignedBox<Worker>,
    pub shared_history: Arc<SharedHistories>,
    pub tt: Arc<TranspositionTable>,
    pub shared_ctrl: Arc<SharedControl>,
    pub threads_count: usize,
    last_best_move: Move,
}

impl ThreadPool {
    pub fn new(num_threads: usize, tt_size_mb: usize) -> Self {
        let count = num_threads.max(1);
        let tt = Arc::new(TranspositionTable::new(tt_size_mb));
        let shared_history = Arc::new(SharedHistories::new_uninit(count));
        let shared_ctrl = Arc::new(SharedControl::new(count));

        let main_worker = PageAlignedBox::new(Worker::new(0, shared_ctrl.clone()));
        shared_ctrl.register_worker(0, main_worker.as_ptr());

        let mut helpers = Vec::with_capacity(count - 1);
        for i in 1..count {
            helpers.push(HelperThread::spawn(i, shared_ctrl.clone()));
        }

        let mut pool = Self {
            main_worker,
            helpers,
            shared_history,
            tt,
            shared_ctrl,
            threads_count: count,
            last_best_move: Move::none(),
        };
        pool.clear();
        pool
    }

    pub fn set_threads(&mut self, count: usize) {
        let count = count.max(1);
        if count == self.threads_count {
            return;
        }
        // Drop existing helper threads cleanly before replacing shared_ctrl
        self.helpers.clear();

        self.threads_count = count;
        self.shared_history = Arc::new(SharedHistories::new_uninit(count));
        self.shared_ctrl = Arc::new(SharedControl::new(count));
        self.main_worker = PageAlignedBox::new(Worker::new(0, self.shared_ctrl.clone()));
        self.shared_ctrl.register_worker(0, self.main_worker.as_ptr());

        for i in 1..count {
            self.helpers.push(HelperThread::spawn(i, self.shared_ctrl.clone()));
        }
        self.clear();
    }

    pub fn set_hash(&mut self, tt_size_mb: usize) {
        // Resize in-place (frees old buffer before allocating new one to avoid 2x peak RSS)
        if let Some(tt) = Arc::get_mut(&mut self.tt) {
            tt.resize(tt_size_mb);
        } else {
            self.tt = Arc::new(TranspositionTable::new(tt_size_mb));
        }
        self.clear_tt();
    }

    pub fn clear_tt(&self) {
        self.tt.reset_generation();
        let count = self.threads_count;
        if count == 1 {
            self.tt.clear_range(0, 1);
            return;
        }
        for (idx, h) in self.helpers.iter().enumerate() {
            let tid = idx + 1;
            let tt = self.tt.clone();
            h.run_job(move |_| {
                tt.clear_range(tid, count);
            });
        }
        self.tt.clear_range(0, count);
        for h in &self.helpers {
            h.wait();
        }
    }

    /// Parallel clear across all persistent worker threads:
    /// TT, SharedHistories (sliced per thread), and per-worker tables.
    pub fn clear(&mut self) {
        self.tt.reset_generation();
        let count = self.threads_count;

        if count == 1 {
            self.tt.clear_range(0, 1);
            self.shared_history.clear_range(0, 1);
            self.main_worker.clear_local();
            return;
        }

        for (idx, h) in self.helpers.iter().enumerate() {
            let tid = idx + 1;
            let tt = self.tt.clone();
            let sh = self.shared_history.clone();
            h.run_job(move |w| {
                tt.clear_range(tid, count);
                sh.clear_range(tid, count);
                w.clear_local();
            });
        }

        self.tt.clear_range(0, count);
        self.shared_history.clear_range(0, count);
        self.main_worker.clear_local();

        for h in &self.helpers {
            h.wait();
        }
    }

    pub fn start_thinking(&mut self, pos: &Position, limits: LimitsType) {
        self.shared_ctrl.stop.store(false, Ordering::Relaxed);
        self.shared_ctrl.increase_depth.store(true, Ordering::Relaxed);
        self.tt.new_search();
        self.last_best_move = Move::none();

        // Generate legal root moves once on the main thread
        let mut root_moves = Vec::with_capacity(64);
        let mut raw_moves = [Move::none(); MAX_MOVES];
        let move_cnt = crate::movegen::generate_moves(
            pos,
            &mut raw_moves,
            0,
            crate::movegen::GenType::All,
        );
        for i in 0..move_cnt {
            let m = raw_moves[i];
            if !limits.searchmoves.is_empty() {
                if limits.searchmoves.contains(&m) {
                    root_moves.push(RootMove::new(m));
                }
            } else {
                root_moves.push(RootMove::new(m));
            }
        }

        self.main_worker.setup_search_with_moves(pos, &limits, &root_moves);

        if self.main_worker.root_moves.is_empty() {
            return;
        }

        if self.threads_count == 1 {
            self.main_worker.iterative_deepening(&self.tt, &self.shared_history);
            self.last_best_move = self.main_worker.root_moves[0].pv[0];
            return;
        }

        // Multi-threaded Lazy SMP on persistent worker threads
        let pos_addr = pos as *const Position as usize;
        let rm_addr = root_moves.as_ptr() as usize;
        let rm_len = root_moves.len();

        for h in &self.helpers {
            let tt = self.tt.clone();
            let sh = self.shared_history.clone();
            let lim = limits.clone();
            h.run_job(move |w| {
                let pos_ref = unsafe { &*(pos_addr as *const Position) };
                let rm_slice = unsafe { core::slice::from_raw_parts(rm_addr as *const RootMove, rm_len) };
                w.setup_search_with_moves(pos_ref, &lim, rm_slice);
                w.iterative_deepening(&tt, &sh);
            });
        }

        self.main_worker.iterative_deepening(&self.tt, &self.shared_history);
        self.shared_ctrl.stop.store(true, Ordering::Relaxed);

        for h in &self.helpers {
            h.wait();
        }

        // Select best thread via Stockfish's voting scheme when not fixed-depth
        let best_idx = if limits.depth == 0 {
            self.get_best_worker_idx()
        } else {
            0
        };

        if best_idx == 0 {
            self.last_best_move = self.main_worker.root_moves[0].pv[0];
        } else {
            let best_ptr = self.shared_ctrl.worker_ptrs[best_idx].load(Ordering::Relaxed) as *const Worker;
            let best_worker = unsafe { &*best_ptr };
            self.last_best_move = best_worker.root_moves[0].pv[0];
            self.main_worker.best_previous_score = best_worker.root_moves[0].score;
            self.main_worker.best_previous_average_score = best_worker.root_moves[0].average_score;
            self.main_worker.output_pv_for(best_worker, &self.main_worker.root_pos, best_worker.root_depth, &self.tt);
        }
    }

    fn get_best_worker_idx(&self) -> usize {
        let mut best_idx = 0usize;
        let mut min_score = VALUE_INFINITE;

        let count = self.threads_count;
        let mut workers: Vec<(usize, &Worker)> = Vec::with_capacity(count);
        for i in 0..count {
            let ptr = self.shared_ctrl.worker_ptrs[i].load(Ordering::Relaxed) as *const Worker;
            if !ptr.is_null() {
                let w = unsafe { &*ptr };
                if !w.root_moves.is_empty() {
                    min_score = min_score.min(w.root_moves[0].score);
                    workers.push((i, w));
                }
            }
        }

        let mut votes: HashMap<u16, i64> = HashMap::with_capacity(2 * count);
        for &(_, w) in &workers {
            let m_raw = w.root_moves[0].pv[0].raw();
            *votes.entry(m_raw).or_insert(0) += (w.root_moves[0].score - min_score + 14) as i64;
        }

        let mut best_w: &Worker = &self.main_worker;
        for &(i, w) in &workers {
            let best_rm = &best_w.root_moves[0];
            let new_rm = &w.root_moves[0];

            let best_vote = *votes.get(&best_rm.pv[0].raw()).unwrap_or(&0);
            let new_vote = *votes.get(&new_rm.pv[0].raw()).unwrap_or(&0);

            let best_decisive = best_rm.score != -VALUE_INFINITE
                && is_decisive(best_rm.score)
                && !best_rm.is_inexact();
            let new_decisive = new_rm.score != -VALUE_INFINITE
                && is_decisive(new_rm.score)
                && !new_rm.is_inexact();

            if best_decisive {
                if new_decisive && new_rm.score.abs() > best_rm.score.abs() {
                    best_idx = i;
                    best_w = w;
                }
            } else if new_decisive
                || (!is_loss(new_rm.score)
                    && (new_vote > best_vote
                        || (new_vote == best_vote && new_rm.pv.len() > best_rm.pv.len())))
            {
                best_idx = i;
                best_w = w;
            }
        }

        best_idx
    }

    pub fn nodes_searched(&self) -> u64 {
        self.shared_ctrl.nodes_searched()
    }

    pub fn main_worker(&self) -> &Worker {
        &self.main_worker
    }

    pub fn main_worker_mut(&mut self) -> &mut Worker {
        &mut self.main_worker
    }

    pub fn best_move(&self) -> Move {
        self.last_best_move
    }

    pub fn set_stop(&self, val: bool) {
        self.shared_ctrl.stop.store(val, Ordering::Relaxed);
    }
}
