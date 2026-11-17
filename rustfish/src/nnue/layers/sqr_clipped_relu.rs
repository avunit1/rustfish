use core::arch::x86_64::*;

pub struct SqrClippedReLU<const IN_DIMS: usize, const WEIGHT_SCALE_BITS: i32>;

// Specialization for Layer 0: 32 inputs, WeightScaleBits = 7 (shift = 7*2+7-16 = 5)
impl SqrClippedReLU<32, 7> {
    #[inline(always)]
    pub unsafe fn propagate_pair(
        input: &[i32; 32],
        squared: &mut [u8; 32],
        clipped: &mut [u8; 32],
    ) {
        let in_ptr = input.as_ptr() as *const __m256i;
        let sqr_ptr = squared.as_mut_ptr() as *mut __m256i;
        let clip_ptr = clipped.as_mut_ptr() as *mut __m256i;
        let zero = _mm256_setzero_si256();

        let words0 = _mm256_packs_epi32(
            _mm256_load_si256(in_ptr.add(0)),
            _mm256_load_si256(in_ptr.add(1)),
        );
        let words1 = _mm256_packs_epi32(
            _mm256_load_si256(in_ptr.add(2)),
            _mm256_load_si256(in_ptr.add(3)),
        );

        let sqr0 = _mm256_srli_epi16::<5>(_mm256_mulhi_epi16(words0, words0));
        let sqr1 = _mm256_srli_epi16::<5>(_mm256_mulhi_epi16(words1, words1));
        let sqr_packed = _mm256_packs_epi16(sqr0, sqr1);
        _mm256_store_si256(sqr_ptr, sqr_packed);

        let clip0 = _mm256_srli_epi16::<7>(_mm256_max_epi16(words0, zero));
        let clip1 = _mm256_srli_epi16::<7>(_mm256_max_epi16(words1, zero));
        let clip_packed = _mm256_packs_epi16(clip0, clip1);
        _mm256_store_si256(clip_ptr, clip_packed);
    }
}

// Specialization for Layer 1: 32 inputs, WeightScaleBits = 6 (shift = 6*2+7-16 = 3)
impl SqrClippedReLU<32, 6> {
    #[inline(always)]
    pub unsafe fn propagate_pair(
        input: &[i32; 32],
        squared: &mut [u8; 32],
        clipped: &mut [u8; 32],
    ) {
        let in_ptr = input.as_ptr() as *const __m256i;
        let sqr_ptr = squared.as_mut_ptr() as *mut __m256i;
        let clip_ptr = clipped.as_mut_ptr() as *mut __m256i;
        let zero = _mm256_setzero_si256();

        let words0 = _mm256_packs_epi32(
            _mm256_load_si256(in_ptr.add(0)),
            _mm256_load_si256(in_ptr.add(1)),
        );
        let words1 = _mm256_packs_epi32(
            _mm256_load_si256(in_ptr.add(2)),
            _mm256_load_si256(in_ptr.add(3)),
        );

        let sqr0 = _mm256_srli_epi16::<3>(_mm256_mulhi_epi16(words0, words0));
        let sqr1 = _mm256_srli_epi16::<3>(_mm256_mulhi_epi16(words1, words1));
        let sqr_packed = _mm256_packs_epi16(sqr0, sqr1);
        _mm256_store_si256(sqr_ptr, sqr_packed);

        let clip0 = _mm256_srli_epi16::<6>(_mm256_max_epi16(words0, zero));
        let clip1 = _mm256_srli_epi16::<6>(_mm256_max_epi16(words1, zero));
        let clip_packed = _mm256_packs_epi16(clip0, clip1);
        _mm256_store_si256(clip_ptr, clip_packed);
    }
}
