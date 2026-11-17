use super::common::{HIDDEN_ONE_VAL, OUTPUT_SCALE, WEIGHT_SCALE_BITS, L1, L2, L3};
use super::layers::{AffineTransform, AffineTransformSparseInput, SqrClippedReLU};
use super::nnz::NNZInfo;

#[repr(C, align(64))]
pub struct NetworkArchitecture {
    pub fc_0: AffineTransformSparseInput,
    pub fc_1: AffineTransform<64, 32, 2048>,
    pub fc_2: AffineTransform<128, 1, 128>,
}

impl NetworkArchitecture {
    pub fn new() -> Self {
        Self {
            fc_0: AffineTransformSparseInput::new(),
            fc_1: AffineTransform::new(),
            fc_2: AffineTransform::new(),
        }
    }

    #[inline(always)]
    pub unsafe fn propagate(
        &self,
        transformed_features: &[u8; L1],
        nnz_info: &NNZInfo<L1>,
    ) -> i32 {
        #[repr(C, align(64))]
        struct Buffer {
            fc_0_out: [i32; L2],
            concat_buffer: [u8; 128],
            fc_1_out: [i32; L3],
            fc_2_out: [i32; 1],
        }

        let mut buf_uninit = core::mem::MaybeUninit::<Buffer>::uninit();
        let buf = &mut *buf_uninit.as_mut_ptr();

        // 1. fc_0: 1024 -> 32
        self.fc_0.propagate(transformed_features, &mut buf.fc_0_out, nnz_info);

        // 2. ac_0 pair: produces squared (concat_buffer[0..32]) and clipped (concat_buffer[32..64])
        let (left, right) = buf.concat_buffer[0..64].split_at_mut(32);
        SqrClippedReLU::<32, { WEIGHT_SCALE_BITS + 1 }>::propagate_pair(
            &buf.fc_0_out,
            &mut *(left.as_mut_ptr() as *mut [u8; 32]),
            &mut *(right.as_mut_ptr() as *mut [u8; 32]),
        );

        // 3. fc_1: 64 -> 32
        let fc_1_in: &[u8; 64] = &*(buf.concat_buffer.as_ptr() as *const [u8; 64]);
        self.fc_1.propagate(fc_1_in, &mut buf.fc_1_out);

        // 4. ac_1 pair: produces squared (concat_buffer[64..96]) and clipped (concat_buffer[96..128])
        let (left1, right1) = buf.concat_buffer[64..128].split_at_mut(32);
        SqrClippedReLU::<32, { WEIGHT_SCALE_BITS }>::propagate_pair(
            &buf.fc_1_out,
            &mut *(left1.as_mut_ptr() as *mut [u8; 32]),
            &mut *(right1.as_mut_ptr() as *mut [u8; 32]),
        );

        // 5. fc_2: 128 -> 1
        self.fc_2.propagate(&buf.concat_buffer, &mut buf.fc_2_out);

        // 6. Skip connection
        let mut fwd_out = buf.fc_2_out[0];
        let skip_0 = buf.fc_0_out[L2 - 2] - buf.fc_0_out[L2 - 1];
        fwd_out += skip_0;

        // 7. Quantization scaling
        const MULTIPLIER: i64 = (600 * OUTPUT_SCALE) as i64;
        const DENOMINATOR: i64 = (HIDDEN_ONE_VAL as i64) * ((1i64 << WEIGHT_SCALE_BITS) as i64) * 2;

        let output_value = ((fwd_out as i64 * MULTIPLIER) / DENOMINATOR) as i32;
        output_value
    }
}
