use core::arch::x86_64::*;

#[inline(always)]
pub unsafe fn m256_add_dpbusd_epi32(acc: &mut __m256i, a: __m256i, b: __m256i) {
    let prod0 = _mm256_maddubs_epi16(a, b);
    let prod1 = _mm256_madd_epi16(prod0, _mm256_set1_epi16(1));
    *acc = _mm256_add_epi32(*acc, prod1);
}

#[inline(always)]
pub unsafe fn m256_hadd(sum: __m256i, bias: i32) -> i32 {
    let lo128 = _mm256_castsi256_si128(sum);
    let hi128 = _mm256_extracti128_si256::<1>(sum);
    let mut sum128 = _mm_add_epi32(lo128, hi128);
    // _MM_PERM_BADC = 0b01_00_11_10 = 0x4E
    let shuf1 = _mm_shuffle_epi32::<0x4E>(sum128);
    sum128 = _mm_add_epi32(sum128, shuf1);
    // _MM_PERM_CDAB = 0b10_11_00_01 = 0xB1
    let shuf2 = _mm_shuffle_epi32::<0xB1>(sum128);
    sum128 = _mm_add_epi32(sum128, shuf2);
    _mm_cvtsi128_si32(sum128) + bias
}

#[inline(always)]
pub unsafe fn vec_nnz(a: __m256i) -> i32 {
    let cmp = _mm256_cmpgt_epi32(a, _mm256_setzero_si256());
    _mm256_movemask_ps(_mm256_castsi256_ps(cmp))
}

