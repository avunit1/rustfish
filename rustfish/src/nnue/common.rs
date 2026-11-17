pub type BiasType = i16;
pub type ThreatWeightType = i8;
pub type WeightType = i8;
pub type IndexType = usize;
pub type TransformedFeatureType = u8;

pub const VERSION: u32 = 0x6A448AFA;
pub const OUTPUT_SCALE: i32 = 16;
pub const WEIGHT_SCALE_BITS: i32 = 6;
pub const FT_MAX_VAL: i16 = 255;
pub const HIDDEN_ONE_VAL: i32 = 128;
pub const CACHE_LINE_SIZE: usize = 64;

pub const L1: usize = 1024;
pub const L2: usize = 32;
pub const L3: usize = 32;
pub const RAW_LAYER_STACKS: usize = 192;
pub const LAYER_STACKS: usize = RAW_LAYER_STACKS;

#[rustfmt::skip]
pub const ORIENT_TBL: [u8; 64] = [
    7, 7, 7, 7, 0, 0, 0, 0,
    7, 7, 7, 7, 0, 0, 0, 0,
    7, 7, 7, 7, 0, 0, 0, 0,
    7, 7, 7, 7, 0, 0, 0, 0,
    7, 7, 7, 7, 0, 0, 0, 0,
    7, 7, 7, 7, 0, 0, 0, 0,
    7, 7, 7, 7, 0, 0, 0, 0,
    7, 7, 7, 7, 0, 0, 0, 0,
];

pub const PACKUS_EPI16_ORDER: [usize; 8] = [0, 2, 1, 3, 4, 6, 5, 7];
pub const WIDENING_PAIR_ORDER: [usize; 4] = [0, 2, 1, 3];

pub const fn ceil_to_multiple(n: usize, base: usize) -> usize {
    (n + base - 1) / base * base
}

pub fn permute<const BLOCK_SIZE: usize, const ORDER_SIZE: usize>(
    data: &mut [u8],
    order: &[usize; ORDER_SIZE],
) {
    let process_chunk_size = BLOCK_SIZE * ORDER_SIZE;
    assert_eq!(data.len() % process_chunk_size, 0);

    let mut buffer = [0u8; 128]; // Max process_chunk_size is 128 (16 * 8)
    assert!(process_chunk_size <= buffer.len());

    for chunk in data.chunks_exact_mut(process_chunk_size) {
        for j in 0..ORDER_SIZE {
            let src_start = order[j] * BLOCK_SIZE;
            let dst_start = j * BLOCK_SIZE;
            buffer[dst_start..dst_start + BLOCK_SIZE]
                .copy_from_slice(&chunk[src_start..src_start + BLOCK_SIZE]);
        }
        chunk.copy_from_slice(&buffer[..process_chunk_size]);
    }
}

