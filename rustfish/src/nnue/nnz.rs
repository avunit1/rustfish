use core::arch::x86_64::__m256i;
use super::common::L1;
use super::simd::vec_nnz;

pub const BITSET_SIZE: usize = (L1 + 31) / 32; // 32 bytes

#[repr(C, align(8))]
#[derive(Clone, Copy)]
pub struct NNZInfo<const DIMS: usize = L1> {
    pub bitset: [u8; BITSET_SIZE],
}

impl<const DIMS: usize> Default for NNZInfo<DIMS> {
    fn default() -> Self {
        Self {
            bitset: [0u8; BITSET_SIZE],
        }
    }
}

pub struct NNZCursor<'a> {
    bitset: &'a mut [u8],
    out_idx: usize,
}

impl<'a> NNZCursor<'a> {
    #[inline(always)]
    pub unsafe fn record2(&mut self, neurons1: __m256i, neurons2: __m256i) {
        let m1 = vec_nnz(neurons1) as u8;
        let m2 = vec_nnz(neurons2) as u8;
        *self.bitset.get_unchecked_mut(self.out_idx) = m1;
        *self.bitset.get_unchecked_mut(self.out_idx + 1) = m2;
        self.out_idx += 2;
    }
}

impl<const DIMS: usize> NNZInfo<DIMS> {
    #[inline(always)]
    pub fn make_cursor(&mut self, perspective: usize) -> NNZCursor<'_> {
        let start = perspective * (BITSET_SIZE / 2);
        NNZCursor {
            bitset: &mut self.bitset,
            out_idx: start,
        }
    }
}
