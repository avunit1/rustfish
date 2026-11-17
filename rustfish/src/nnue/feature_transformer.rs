use core::arch::x86_64::*;
use crate::position::Position;
use super::accumulator::{AccumulatorCaches, AccumulatorStack};
use super::common::*;
use super::features::{FullThreats, K32H3, PP_3Wide};
use super::nnz::NNZInfo;

pub const THREAT_INPUT_DIMENSIONS: usize = FullThreats::DIMENSIONS; // 59808
pub const PAIR_INPUT_DIMENSIONS: usize = PP_3Wide::DIMENSIONS;     // 4560
pub const PSQ_DIMENSIONS: usize = K32H3::DIMENSIONS;              // 67584
pub const THREAT_AND_PP_DIMENSIONS: usize = THREAT_INPUT_DIMENSIONS + PAIR_INPUT_DIMENSIONS; // 64368
pub const INPUT_DIMENSIONS: usize = PSQ_DIMENSIONS + THREAT_AND_PP_DIMENSIONS; // 131952
pub const HALF_DIMENSIONS: usize = L1; // 1024

pub const THREAT_WEIGHT_SIZE: usize = THREAT_INPUT_DIMENSIONS * HALF_DIMENSIONS;
pub const PAIR_WEIGHT_SIZE: usize = PAIR_INPUT_DIMENSIONS * HALF_DIMENSIONS;
pub const THREAT_AND_PP_WEIGHT_SIZE: usize = THREAT_AND_PP_DIMENSIONS * HALF_DIMENSIONS;
pub const PSQ_WEIGHT_SIZE: usize = PSQ_DIMENSIONS * HALF_DIMENSIONS;

#[repr(C, align(64))]
#[derive(Clone, Copy)]
pub struct AlignedI8Block([i8; 64]);

pub struct AlignedI8Vec {
    ptr: *mut i8,
    len: usize,
    _owned: Option<Vec<AlignedI8Block>>,
}

unsafe impl Send for AlignedI8Vec {}
unsafe impl Sync for AlignedI8Vec {}

impl AlignedI8Vec {
    pub fn new(len: usize) -> Self {
        debug_assert!(len % 64 == 0);
        let mut blocks = vec![AlignedI8Block([0; 64]); len / 64];
        let ptr = blocks.as_mut_ptr() as *mut i8;
        Self {
            ptr,
            len,
            _owned: Some(blocks),
        }
    }

    /// Wrap a 64-byte aligned external/shared memory region.
    pub unsafe fn from_raw_parts(ptr: *mut i8, len: usize) -> Self {
        debug_assert!((ptr as usize) % 64 == 0);
        debug_assert!(len % 64 == 0);
        Self {
            ptr,
            len,
            _owned: None,
        }
    }

    #[inline(always)]
    pub fn as_ptr(&self) -> *const i8 {
        self.ptr as *const i8
    }

    #[inline(always)]
    pub fn as_mut_ptr(&mut self) -> *mut i8 {
        self.ptr
    }

    #[inline(always)]
    pub fn len(&self) -> usize {
        self.len
    }

    #[inline(always)]
    pub fn copy_from_slice(&mut self, src: &[i8]) {
        debug_assert_eq!(src.len(), self.len());
        unsafe {
            core::ptr::copy_nonoverlapping(src.as_ptr(), self.as_mut_ptr(), src.len());
        }
    }
}

#[repr(C, align(64))]
pub struct FeatureTransformer {
    pub biases: [i16; HALF_DIMENSIONS],
    pub threat_and_pp_weights: AlignedI8Vec,
    pub weights: AlignedI8Vec,
}

impl FeatureTransformer {
    pub fn new() -> Self {
        Self {
            biases: [0; HALF_DIMENSIONS],
            threat_and_pp_weights: AlignedI8Vec::new(THREAT_AND_PP_WEIGHT_SIZE),
            weights: AlignedI8Vec::new(PSQ_WEIGHT_SIZE),
        }
    }

    pub fn permute_weights(&mut self) {
        // Permute biases: permute<16>(biases, PackusEpi16Order)
        // biases is 2048 bytes (1024 * i16)
        let biases_bytes = unsafe {
            core::slice::from_raw_parts_mut(self.biases.as_mut_ptr() as *mut u8, HALF_DIMENSIONS * 2)
        };
        permute::<16, 8>(biases_bytes, &PACKUS_EPI16_ORDER);

        // Permute weights: permute<8>(weights, PackusEpi16Order)
        let weights_bytes = unsafe {
            core::slice::from_raw_parts_mut(self.weights.as_mut_ptr() as *mut u8, self.weights.len())
        };
        permute::<8, 8>(weights_bytes, &PACKUS_EPI16_ORDER);

        // Permute threatAndPpWeights: permute<8>(threatAndPpWeights, PackusEpi16Order)
        let thr_bytes = unsafe {
            core::slice::from_raw_parts_mut(
                self.threat_and_pp_weights.as_mut_ptr() as *mut u8,
                self.threat_and_pp_weights.len(),
            )
        };
        permute::<8, 8>(thr_bytes, &PACKUS_EPI16_ORDER);

        // Widening pair permutation: permute<8>(..., WideningPairOrder)
        permute::<8, 4>(weights_bytes, &WIDENING_PAIR_ORDER);
        permute::<8, 4>(thr_bytes, &WIDENING_PAIR_ORDER);
    }

    #[inline(always)]
    pub fn psq_weights(&self, block: usize) -> *const i8 {
        unsafe {
            self.weights.as_ptr().add(K32H3::block_offset(block) * HALF_DIMENSIONS)
        }
    }

    pub unsafe fn transform(
        &self,
        pos: &Position,
        accumulator_stack: &mut AccumulatorStack,
        cache: &mut AccumulatorCaches,
        output: &mut [u8; L1],
        nnz_info: &mut NNZInfo<L1>,
    ) {
        accumulator_stack.evaluate(pos, self, cache);
        let acc_state = accumulator_stack.latest();

        let stm = pos.side_to_move();
        let perspectives = [stm, !stm];

        for p in 0..2 {
            self.transform_perspective(
                &acc_state.accumulation[perspectives[p] as usize],
                output,
                p,
                nnz_info,
            );
        }
    }

    #[inline(always)]
    pub unsafe fn transform_perspective(
        &self,
        accumulation: &[i16; HALF_DIMENSIONS],
        output: &mut [u8; L1],
        perspective: usize,
        nnz_info: &mut NNZInfo<L1>,
    ) {
        let mut cursor = nnz_info.make_cursor(perspective);
        let offset = (HALF_DIMENSIONS / 2) * perspective; // 0 or 512

        const NUM_OUTPUT_CHUNKS: usize = (HALF_DIMENSIONS / 2) / 32; // 16
        let zero = _mm256_setzero_si256();
        let ft_max = _mm256_set1_epi16(FT_MAX_VAL);
        const SHIFT: i32 = 7;

        let in0 = accumulation.as_ptr() as *const __m256i;
        let in1 = accumulation.as_ptr().add(HALF_DIMENSIONS / 2) as *const __m256i;
        let out = output.as_mut_ptr().add(offset) as *mut __m256i;

        let mut j = 0;
        while j < NUM_OUTPUT_CHUNKS {
            let i0 = j * 2;
            let sum0a = _mm256_slli_epi16(_mm256_max_epi16(_mm256_min_epi16(_mm256_load_si256(in0.add(i0)), ft_max), zero), SHIFT);
            let sum0b = _mm256_slli_epi16(_mm256_max_epi16(_mm256_min_epi16(_mm256_load_si256(in0.add(i0 + 1)), ft_max), zero), SHIFT);
            let sum1a = _mm256_min_epi16(_mm256_load_si256(in1.add(i0)), ft_max);
            let sum1b = _mm256_min_epi16(_mm256_load_si256(in1.add(i0 + 1)), ft_max);
            let p0 = _mm256_packus_epi16(_mm256_mulhi_epi16(sum0a, sum1a), _mm256_mulhi_epi16(sum0b, sum1b));
            _mm256_store_si256(out.add(j), p0);

            let i1 = i0 + 2;
            let sum0c = _mm256_slli_epi16(_mm256_max_epi16(_mm256_min_epi16(_mm256_load_si256(in0.add(i1)), ft_max), zero), SHIFT);
            let sum0d = _mm256_slli_epi16(_mm256_max_epi16(_mm256_min_epi16(_mm256_load_si256(in0.add(i1 + 1)), ft_max), zero), SHIFT);
            let sum1c = _mm256_min_epi16(_mm256_load_si256(in1.add(i1)), ft_max);
            let sum1d = _mm256_min_epi16(_mm256_load_si256(in1.add(i1 + 1)), ft_max);
            let p1 = _mm256_packus_epi16(_mm256_mulhi_epi16(sum0c, sum1c), _mm256_mulhi_epi16(sum0d, sum1d));
            _mm256_store_si256(out.add(j + 1), p1);

            cursor.record2(p0, p1);
            j += 2;
        }
    }
}
