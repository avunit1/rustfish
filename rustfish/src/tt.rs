use core::sync::atomic::{AtomicI16, AtomicU16, AtomicU64, AtomicU8, Ordering};
use crate::memory::{CachePadded, PageAlignedSlice};
use crate::types::*;

pub const GENERATION_BITS: u8 = 5;
pub const GENERATION_MASK: u8 = (1 << GENERATION_BITS) - 1;
pub const BOUND_SHIFT: u8 = GENERATION_BITS;
pub const BOUND_MASK: u8 = 0b11 << BOUND_SHIFT;
pub const PV_SHIFT: u8 = BOUND_SHIFT + 2;
pub const PV_MASK: u8 = 1 << PV_SHIFT;

pub const CLUSTER_SIZE: usize = 3;
pub const KEY_BITS: usize = 21;
pub const KEY_MASK: u64 = (1 << KEY_BITS) - 1;

#[derive(Clone, Copy, Debug)]
pub struct TTData {
    pub move_: Move,
    pub value: Value,
    pub eval: Value,
    pub depth: Depth,
    pub bound: Bound,
    pub is_pv: bool,
}

impl TTData {
    pub const fn empty() -> Self {
        Self {
            move_: Move::none(),
            value: VALUE_NONE,
            eval: VALUE_NONE,
            depth: DEPTH_NONE,
            bound: Bound::None,
            is_pv: false,
        }
    }
}

#[repr(C)]
pub struct TTEntry {
    pub depth8: AtomicU8,
    pub gen_bound8: AtomicU8,
    pub move16: AtomicU16,
    pub value16: AtomicI16,
    pub eval16: AtomicI16,
}

impl TTEntry {
    pub const fn new() -> Self {
        Self {
            depth8: AtomicU8::new(0),
            gen_bound8: AtomicU8::new(0),
            move16: AtomicU16::new(0),
            value16: AtomicI16::new(0),
            eval16: AtomicI16::new(0),
        }
    }

    #[inline(always)]
    pub fn read(&self) -> TTData {
        let depth_raw = self.depth8.load(Ordering::Relaxed);
        let gen_bound = self.gen_bound8.load(Ordering::Relaxed);
        let m = Move::from_raw(self.move16.load(Ordering::Relaxed));
        let v = self.value16.load(Ordering::Relaxed) as Value;
        let ev = self.eval16.load(Ordering::Relaxed) as Value;

        TTData {
            move_: m,
            value: v,
            eval: ev,
            depth: (DEPTH_NONE as i32 + depth_raw as i32) as Depth,
            bound: Bound::from_u8((gen_bound & BOUND_MASK) >> BOUND_SHIFT),
            is_pv: (gen_bound & PV_MASK) != 0,
        }
    }

    #[inline(always)]
    pub fn is_occupied(&self) -> bool {
        self.depth8.load(Ordering::Relaxed) != 0
    }

    #[inline(always)]
    pub fn relative_age(&self, curr_generation: u8) -> u8 {
        let gb = self.gen_bound8.load(Ordering::Relaxed);
        curr_generation.wrapping_sub(gb) & GENERATION_MASK
    }
}

#[repr(C, align(32))]
pub struct Cluster {
    pub keys: AtomicU64,
    pub entry: [TTEntry; CLUSTER_SIZE],
}

const _: () = assert!(core::mem::size_of::<Cluster>() == 32);

impl Cluster {
    pub const fn new() -> Self {
        Self {
            keys: AtomicU64::new(0),
            entry: [const { TTEntry::new() }; CLUSTER_SIZE],
        }
    }
}

pub struct TTWriter<'a> {
    cluster: &'a Cluster,
    index: usize,
}

impl<'a> TTWriter<'a> {
    #[inline(always)]
    pub fn new(cluster: &'a Cluster, index: usize) -> Self {
        Self { cluster, index }
    }

    #[inline(always)]
    pub fn write(
        &self,
        k: Key,
        v: Value,
        pv: bool,
        b: Bound,
        d: Depth,
        m: Move,
        ev: Value,
        curr_generation: u8,
    ) {
        let shift = KEY_BITS * self.index;
        let keys = self.cluster.keys.load(Ordering::Relaxed);
        let key_diff = ((keys >> shift) ^ k) & KEY_MASK;
        let entry = unsafe { self.cluster.entry.get_unchecked(self.index) };

        if m != Move::none() || key_diff != 0 {
            entry.move16.store(m.raw(), Ordering::Relaxed);
        }

        let entry_depth = entry.depth8.load(Ordering::Relaxed) as i32;
        let relative_age = entry.relative_age(curr_generation);

        if b == Bound::Exact
            || key_diff != 0
            || (d as i32 - DEPTH_NONE as i32 + 2 * (pv as i32)) > (entry_depth - 4)
            || relative_age != 0
        {
            debug_assert!(d > DEPTH_NONE);
            debug_assert!((d as i32 - DEPTH_NONE as i32) < 256);
            debug_assert!(curr_generation <= GENERATION_MASK);

            self.cluster
                .keys
                .store(keys ^ (key_diff << shift), Ordering::Relaxed);
            entry
                .depth8
                .store((d as i32 - DEPTH_NONE as i32) as u8, Ordering::Relaxed);
            let gb = curr_generation
                | ((b as u8) << BOUND_SHIFT)
                | ((pv as u8) << PV_SHIFT);
            entry.gen_bound8.store(gb, Ordering::Relaxed);
            entry.value16.store(v as i16, Ordering::Relaxed);
            entry.eval16.store(ev as i16, Ordering::Relaxed);
        } else if entry_depth + (DEPTH_NONE as i32) >= 5 {
            let gb = entry.gen_bound8.load(Ordering::Relaxed);
            let cur_b = Bound::from_u8((gb & BOUND_MASK) >> BOUND_SHIFT);
            if cur_b != Bound::Exact {
                let v16 = entry.value16.load(Ordering::Relaxed) as Value;
                if v16.abs() < VALUE_INFINITE && is_decisive(v16) {
                    entry
                        .depth8
                        .store((entry_depth - 1).max(0) as u8, Ordering::Relaxed);
                }
            }
        }
    }

    #[inline(always)]
    pub fn penalize(&self, penalty: i32) {
        let entry = unsafe { self.cluster.entry.get_unchecked(self.index) };
        let d = entry.depth8.load(Ordering::Relaxed) as i32;
        entry
            .depth8
            .store((d - penalty).max(0) as u8, Ordering::Relaxed);
    }
}

pub struct TranspositionTable {
    table: PageAlignedSlice<Cluster>,
    cluster_count: usize,
    generation8: CachePadded<AtomicU8>,
}

impl TranspositionTable {
    pub fn new(mb_size: usize) -> Self {
        let cluster_count = ((mb_size.max(1)) * 1024 * 1024) / std::mem::size_of::<Cluster>();
        let table = PageAlignedSlice::new_large_pages_zeroed(cluster_count);
        Self {
            table,
            cluster_count,
            generation8: CachePadded::new(AtomicU8::new(0)),
        }
    }

    pub fn resize(&mut self, mb_size: usize) {
        let cluster_count = ((mb_size.max(1)) * 1024 * 1024) / std::mem::size_of::<Cluster>();
        self.cluster_count = cluster_count;
        // Allocate minimal dummy first so old buffer is freed before allocating new one
        self.table = PageAlignedSlice::new_zeroed(1);
        self.table = PageAlignedSlice::new_large_pages_zeroed(cluster_count);
        self.generation8.store(0, Ordering::Relaxed);
    }

    pub fn clear(&self) {
        self.generation8.store(0, Ordering::Relaxed);
        self.clear_range(0, 1);
    }

    pub fn reset_generation(&self) {
        self.generation8.store(0, Ordering::Relaxed);
    }

    pub fn clear_range(&self, thread_idx: usize, thread_count: usize) {
        let stride = self.cluster_count / thread_count;
        let start = stride * thread_idx;
        let len = if thread_idx + 1 != thread_count {
            stride
        } else {
            self.cluster_count - start
        };
        unsafe {
            core::ptr::write_bytes(
                self.table.as_ptr().add(start) as *mut u8,
                0,
                len * core::mem::size_of::<Cluster>(),
            );
        }
    }

    #[inline(always)]
    pub fn new_search(&self) {
        let gen = self.generation8.load(Ordering::Relaxed);
        self.generation8.store((gen + 1) & GENERATION_MASK, Ordering::Relaxed);
    }

    #[inline(always)]
    pub fn generation(&self) -> u8 {
        self.generation8.load(Ordering::Relaxed)
    }

    pub fn hashfull(&self, max_age: i32) -> i32 {
        let mut cnt = 0;
        let limit = 1000.min(self.cluster_count);
        let gen = self.generation();
        let slice = self.table.as_slice();
        for i in 0..limit {
            for j in 0..CLUSTER_SIZE {
                let e = &slice[i].entry[j];
                if e.is_occupied() && (e.relative_age(gen) as i32) <= max_age {
                    cnt += 1;
                }
            }
        }
        cnt / (CLUSTER_SIZE as i32)
    }

    #[inline(always)]
    fn get_cluster(&self, key: Key) -> &Cluster {
        let idx = ((key as u128 * self.cluster_count as u128) >> 64) as usize;
        unsafe { self.table.get_unchecked(idx) }
    }

    #[inline(always)]
    pub fn prefetch(&self, key: Key) {
        #[cfg(target_arch = "x86_64")]
        unsafe {
            let ptr = self.get_cluster(key) as *const Cluster as *const i8;
            core::arch::x86_64::_mm_prefetch(ptr, core::arch::x86_64::_MM_HINT_T0);
        }
    }

    #[inline(always)]
    pub fn probe(&self, key: Key) -> (bool, TTData, TTWriter<'_>) {
        let cluster = self.get_cluster(key);
        let key21 = key & KEY_MASK;

        const KEY_LSB: u64 = 1 | (1 << KEY_BITS) | (1 << (2 * KEY_BITS));
        const KEY_MSB: u64 = KEY_LSB << (KEY_BITS - 1);
        let keys = cluster.keys.load(Ordering::Relaxed);
        let key_xors = keys ^ (KEY_LSB.wrapping_mul(key21));
        let matches = (key_xors.wrapping_sub(KEY_LSB)) & !key_xors & KEY_MSB;

        if matches != 0 {
            let i = (((matches.trailing_zeros() as i32) >> 4) - 1) as usize;
            let entry = unsafe { cluster.entry.get_unchecked(i) };
            return (
                entry.is_occupied(),
                entry.read(),
                TTWriter::new(cluster, i),
            );
        }

        let gen = self.generation();
        let mut replace = 0;
        let mut replace_value = (cluster.entry[0].depth8.load(Ordering::Relaxed) as i32)
            - 8 * (cluster.entry[0].relative_age(gen) as i32);

        for i in 1..CLUSTER_SIZE {
            let val = (cluster.entry[i].depth8.load(Ordering::Relaxed) as i32)
                - 8 * (cluster.entry[i].relative_age(gen) as i32);
            if replace_value > val {
                replace = i;
                replace_value = val;
            }
        }

        (
            false,
            TTData::empty(),
            TTWriter::new(cluster, replace),
        )
    }
}