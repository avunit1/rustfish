use core::arch::x86_64::*;
use core::ops::{Deref, DerefMut};
use super::super::simd::{m256_add_dpbusd_epi32, m256_hadd};

#[inline(always)]
pub const fn get_weight_index<const IN_DIMS: usize, const OUT_DIMS: usize>(i: usize) -> usize {
    let input_index = i % IN_DIMS;
    let block = input_index / 32;
    let chunk = (input_index % 32) / 4;
    let scrambled_in = block * 32 + ((chunk % 2) * 4 + chunk / 2) * 4 + (input_index % 4);
    (scrambled_in / 4) * OUT_DIMS * 4 + (i / IN_DIMS) * 4 + (scrambled_in % 4)
}

#[repr(C, align(64))]
pub struct AlignedBiases<const N: usize>(pub [i32; N]);

impl<const N: usize> Deref for AlignedBiases<N> {
    type Target = [i32; N];
    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl<const N: usize> DerefMut for AlignedBiases<N> {
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[repr(C, align(64))]
pub struct AlignedWeights<const N: usize>(pub [i8; N]);

impl<const N: usize> Deref for AlignedWeights<N> {
    type Target = [i8; N];
    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl<const N: usize> DerefMut for AlignedWeights<N> {
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[repr(C, align(64))]
pub struct AffineTransform<const IN_DIMS: usize, const OUT_DIMS: usize, const WEIGHTS_COUNT: usize> {
    pub biases: AlignedBiases<OUT_DIMS>,
    pub weights: AlignedWeights<WEIGHTS_COUNT>,
}

impl<const IN_DIMS: usize, const OUT_DIMS: usize, const WEIGHTS_COUNT: usize> AffineTransform<IN_DIMS, OUT_DIMS, WEIGHTS_COUNT> {
    pub fn new() -> Self {
        Self {
            biases: AlignedBiases([0; OUT_DIMS]),
            weights: AlignedWeights([0; WEIGHTS_COUNT]),
        }
    }
}

// Specialization for fc_1: 64 inputs -> 32 outputs (2048 weights)
impl AffineTransform<64, 32, 2048> {
    #[inline(always)]
    pub unsafe fn propagate(&self, input: &[u8; 64], output: &mut [i32; 32]) {
        let bias_ptr = self.biases.as_ptr() as *const __m256i;
        let mut acc: [__m256i; 4] = [
            _mm256_load_si256(bias_ptr.add(0)),
            _mm256_load_si256(bias_ptr.add(1)),
            _mm256_load_si256(bias_ptr.add(2)),
            _mm256_load_si256(bias_ptr.add(3)),
        ];

        let weights_ptr = self.weights.as_ptr();
        let input_ptr = input.as_ptr();

        for i in 0..16 {
            let in_val = *(input_ptr.add(i * 4) as *const i32);
            let in_vec = _mm256_set1_epi32(in_val);

            let col_ptr = weights_ptr.add(i * 32 * 4) as *const __m256i;
            let col0 = _mm256_load_si256(col_ptr.add(0));
            let col1 = _mm256_load_si256(col_ptr.add(1));
            let col2 = _mm256_load_si256(col_ptr.add(2));
            let col3 = _mm256_load_si256(col_ptr.add(3));

            m256_add_dpbusd_epi32(&mut acc[0], in_vec, col0);
            m256_add_dpbusd_epi32(&mut acc[1], in_vec, col1);
            m256_add_dpbusd_epi32(&mut acc[2], in_vec, col2);
            m256_add_dpbusd_epi32(&mut acc[3], in_vec, col3);
        }

        let out_ptr = output.as_mut_ptr() as *mut __m256i;
        _mm256_storeu_si256(out_ptr.add(0), acc[0]);
        _mm256_storeu_si256(out_ptr.add(1), acc[1]);
        _mm256_storeu_si256(out_ptr.add(2), acc[2]);
        _mm256_storeu_si256(out_ptr.add(3), acc[3]);
    }
}

// Specialization for fc_2: 128 inputs -> 1 output (128 weights)
impl AffineTransform<128, 1, 128> {
    #[inline(always)]
    pub unsafe fn propagate(&self, input: &[u8; 128], output: &mut [i32; 1]) {
        let input_vec = input.as_ptr() as *const __m256i;
        let weights_vec = self.weights.as_ptr() as *const __m256i;

        let mut sum0 = _mm256_setzero_si256();
        for j in 0..4 {
            let in_chunk = _mm256_loadu_si256(input_vec.add(j));
            let weight_chunk = _mm256_load_si256(weights_vec.add(j));
            m256_add_dpbusd_epi32(&mut sum0, in_chunk, weight_chunk);
        }

        output[0] = m256_hadd(sum0, self.biases[0]);
    }
}
