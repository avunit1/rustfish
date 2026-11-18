use core::arch::x86_64::*;
use super::super::nnz::NNZInfo;

pub const IN_DIMS: usize = 1024;
pub const OUT_DIMS: usize = 32;
pub const CHUNK_SIZE: usize = 4;

#[inline(always)]
pub const fn get_weight_index(i: usize) -> usize {
    ((i / CHUNK_SIZE) % (IN_DIMS / CHUNK_SIZE)) * OUT_DIMS * CHUNK_SIZE
        + (i / IN_DIMS) * CHUNK_SIZE
        + (i % CHUNK_SIZE)
}

#[repr(C, align(64))]
pub struct AffineTransformSparseInput {
    pub biases: [i32; OUT_DIMS],
    pub weights: [i8; OUT_DIMS * IN_DIMS],
}

impl AffineTransformSparseInput {
    pub fn new() -> Self {
        Self {
            biases: [0; OUT_DIMS],
            weights: [0; OUT_DIMS * IN_DIMS],
        }
    }

    #[inline(always)]
    pub unsafe fn propagate(
        &self,
        input: &[u8; IN_DIMS],
        output: &mut [i32; OUT_DIMS],
        nnz_info: &NNZInfo<IN_DIMS>,
    ) {
        let bias_ptr = self.biases.as_ptr() as *const __m256i;
        let mut acc0 = _mm256_load_si256(bias_ptr.add(0));
        let mut acc1 = _mm256_load_si256(bias_ptr.add(1));
        let mut acc2 = _mm256_load_si256(bias_ptr.add(2));
        let mut acc3 = _mm256_load_si256(bias_ptr.add(3));
        let ones = _mm256_set1_epi16(1);

        let weights_ptr = self.weights.as_ptr();
        let input_ptr = input.as_ptr();

        for k in 0..(IN_DIMS / 256) {
            let mut bits = *(nnz_info.bitset.as_ptr().add(k * 8) as *const u64);
            let base = k * 64;
            let base_addr = input_ptr.add(base * 4);
            let weights_base = weights_ptr.add(base * OUT_DIMS * CHUNK_SIZE);

            while bits != 0 {
                let i = bits.trailing_zeros() as usize;
                bits &= bits - 1;

                let in_vec = _mm256_set1_epi32(*(base_addr.add(i * 4) as *const i32));
                let col_ptr = weights_base.add(i * OUT_DIMS * CHUNK_SIZE) as *const __m256i;

                acc0 = _mm256_add_epi32(acc0, _mm256_madd_epi16(_mm256_maddubs_epi16(in_vec, _mm256_load_si256(col_ptr.add(0))), ones));
                acc1 = _mm256_add_epi32(acc1, _mm256_madd_epi16(_mm256_maddubs_epi16(in_vec, _mm256_load_si256(col_ptr.add(1))), ones));
                acc2 = _mm256_add_epi32(acc2, _mm256_madd_epi16(_mm256_maddubs_epi16(in_vec, _mm256_load_si256(col_ptr.add(2))), ones));
                acc3 = _mm256_add_epi32(acc3, _mm256_madd_epi16(_mm256_maddubs_epi16(in_vec, _mm256_load_si256(col_ptr.add(3))), ones));
            }
        }

        let out_ptr = output.as_mut_ptr() as *mut __m256i;
        _mm256_store_si256(out_ptr.add(0), acc0);
        _mm256_store_si256(out_ptr.add(1), acc1);
        _mm256_store_si256(out_ptr.add(2), acc2);
        _mm256_store_si256(out_ptr.add(3), acc3);
    }
}

