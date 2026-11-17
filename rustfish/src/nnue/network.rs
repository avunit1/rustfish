use crate::bitboard::*;
use crate::position::Position;
use crate::types::*;
use super::accumulator::{AccumulatorCaches, AccumulatorStack};
use super::common::*;
use super::feature_transformer::*;
use super::layers::{affine_transform, affine_transform_sparse_input};
use super::network_architecture::NetworkArchitecture;
use super::nnz::NNZInfo;

pub const DEFAULT_NET_NAME: &str = "nn-bddd9d57eb87.nnue";
pub const EMBEDDED_NNUE_DATA: &[u8] = include_bytes!("../../nn-bddd9d57eb87.nnue");

pub struct Network {
    pub feature_transformer: FeatureTransformer,
    pub stored_layer_stacks: usize,
    pub layer_stack_fallback: [u16; RAW_LAYER_STACKS],
    pub layer_stack_index: [u16; RAW_LAYER_STACKS],
    pub network: Vec<NetworkArchitecture>,
    _shm_handle: usize,
    _shm_view: usize,
}

#[cfg(windows)]
mod win_shm {
    use core::sync::atomic::{AtomicU32, Ordering};
    use super::super::feature_transformer::{HALF_DIMENSIONS, PSQ_WEIGHT_SIZE, THREAT_AND_PP_WEIGHT_SIZE};

    const INVALID_HANDLE_VALUE: isize = -1;
    const PAGE_READWRITE: u32 = 0x04;
    const FILE_MAP_ALL_ACCESS: u32 = 0x000F_001F;
    const INFINITE: u32 = 0xFFFF_FFFF;
    const HEADER_SIZE: usize = 4096;
    const MAGIC: u64 = 0x5346_5253_4E4E_5545;

    #[repr(C, align(64))]
    pub struct ShmHeader {
        pub magic: u64,
        pub ready: AtomicU32,
        pub _pad: u32,
        pub biases: [i16; HALF_DIMENSIONS],
    }

    extern "system" {
        fn CreateMutexW(attrs: *mut core::ffi::c_void, initial_owner: i32, name: *const u16) -> isize;
        fn WaitForSingleObject(handle: isize, ms: u32) -> u32;
        fn ReleaseMutex(handle: isize) -> i32;
        fn CreateFileMappingW(
            file: isize,
            attrs: *mut core::ffi::c_void,
            protect: u32,
            max_high: u32,
            max_low: u32,
            name: *const u16,
        ) -> isize;
        fn MapViewOfFile(
            mapping: isize,
            access: u32,
            off_high: u32,
            off_low: u32,
            num_bytes: usize,
        ) -> *mut u8;
        fn CloseHandle(handle: isize) -> i32;
    }

    fn to_wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(core::iter::once(0)).collect()
    }

    pub struct SharedMapping {
        pub handle: usize,
        pub base: *mut u8,
        pub already_initialized: bool,
        pub mutex: isize,
    }

    impl SharedMapping {
        pub unsafe fn acquire() -> Option<Self> {
            let total_size = HEADER_SIZE + THREAT_AND_PP_WEIGHT_SIZE + PSQ_WEIGHT_SIZE;
            let mutex_name = to_wide("Local\\RustFish_NNUE_Mutex_bddd9d57eb87_v1");
            let map_name = to_wide("Local\\RustFish_NNUE_bddd9d57eb87_v1");

            let mutex = CreateMutexW(core::ptr::null_mut(), 0, mutex_name.as_ptr());
            if mutex == 0 {
                return None;
            }
            WaitForSingleObject(mutex, INFINITE);

            let size_hi = ((total_size as u64) >> 32) as u32;
            let size_lo = (total_size as u64) as u32;
            let handle = CreateFileMappingW(
                INVALID_HANDLE_VALUE,
                core::ptr::null_mut(),
                PAGE_READWRITE,
                size_hi,
                size_lo,
                map_name.as_ptr(),
            );
            if handle == 0 {
                ReleaseMutex(mutex);
                CloseHandle(mutex);
                return None;
            }

            let base = MapViewOfFile(handle, FILE_MAP_ALL_ACCESS, 0, 0, total_size);
            if base.is_null() {
                CloseHandle(handle);
                ReleaseMutex(mutex);
                CloseHandle(mutex);
                return None;
            }

            let hdr = &*(base as *const ShmHeader);
            let already_initialized = hdr.magic == MAGIC && hdr.ready.load(Ordering::Acquire) == 1;
            Some(Self {
                handle: handle as usize,
                base,
                already_initialized,
                mutex,
            })
        }

        pub unsafe fn header_mut(&self) -> &mut ShmHeader {
            &mut *(self.base as *mut ShmHeader)
        }

        pub unsafe fn thr_ptr(&self) -> *mut i8 {
            self.base.add(HEADER_SIZE) as *mut i8
        }

        pub unsafe fn psq_ptr(&self) -> *mut i8 {
            self.base.add(HEADER_SIZE + THREAT_AND_PP_WEIGHT_SIZE) as *mut i8
        }

        pub unsafe fn mark_ready(&self, biases: &[i16; HALF_DIMENSIONS]) {
            let hdr = self.header_mut();
            hdr.biases.copy_from_slice(biases);
            hdr.magic = MAGIC;
            hdr.ready.store(1, Ordering::Release);
        }

        pub unsafe fn release_lock(&mut self) {
            if self.mutex != 0 {
                ReleaseMutex(self.mutex);
                CloseHandle(self.mutex);
                self.mutex = 0;
            }
        }
    }
}

impl Network {
    pub fn load_embedded() -> Box<Self> {
        Self::load_from_bytes_inner(EMBEDDED_NNUE_DATA, true).expect("Failed to load embedded NNUE")
    }

    pub fn load_from_bytes(bytes: &[u8]) -> Result<Box<Self>, String> {
        Self::load_from_bytes_inner(bytes, false)
    }

    fn load_from_bytes_inner(bytes: &[u8], use_shm: bool) -> Result<Box<Self>, String> {
        let mut offset = 0;

        let read_u32 = |offset: &mut usize| -> Result<u32, String> {
            if *offset + 4 > bytes.len() {
                return Err("Unexpected EOF reading u32".into());
            }
            let val = u32::from_le_bytes(bytes[*offset..*offset + 4].try_into().unwrap());
            *offset += 4;
            Ok(val)
        };

        let read_u16 = |offset: &mut usize| -> Result<u16, String> {
            if *offset + 2 > bytes.len() {
                return Err("Unexpected EOF reading u16".into());
            }
            let val = u16::from_le_bytes(bytes[*offset..*offset + 2].try_into().unwrap());
            *offset += 2;
            Ok(val)
        };

        let version = read_u32(&mut offset)?;
        if version != VERSION {
            return Err(format!("Invalid NNUE version: 0x{:08X}", version));
        }

        let _hash_val = read_u32(&mut offset)?;
        let desc_size = read_u32(&mut offset)? as usize;
        if offset + desc_size > bytes.len() {
            return Err("Unexpected EOF reading description".into());
        }
        let _desc = String::from_utf8_lossy(&bytes[offset..offset + desc_size]);
        offset += desc_size;

        let layer_hash = read_u32(&mut offset)?;
        let raw_stacks = read_u32(&mut offset)? as usize;
        let stored_stacks = read_u32(&mut offset)? as usize;

        if layer_hash != 0x5A7AB1E5 || raw_stacks != RAW_LAYER_STACKS || stored_stacks > LAYER_STACKS {
            return Err("Invalid layer stack header".into());
        }

        let mut fallback = [0u16; RAW_LAYER_STACKS];
        for f in &mut fallback {
            *f = read_u16(&mut offset)?;
            if *f as usize >= RAW_LAYER_STACKS {
                return Err("Invalid fallback value".into());
            }
        }

        let mut stored = 0u16;
        let mut layer_stack_index = [0u16; RAW_LAYER_STACKS];
        for s in 0..RAW_LAYER_STACKS {
            if fallback[s] as usize == s {
                layer_stack_index[s] = stored;
                stored += 1;
            }
        }
        if stored as usize != stored_stacks {
            return Err(format!("Stored stacks mismatch: {} != {}", stored, stored_stacks));
        }

        for s in 0..RAW_LAYER_STACKS {
            let f = fallback[s] as usize;
            if fallback[f] as usize != f {
                return Err("Invalid multi-hop fallback".into());
            }
            layer_stack_index[s] = layer_stack_index[f];
        }

        // Feature Transformer
        let _ft_hash = read_u32(&mut offset)?;
        let mut shm_handle: usize = 0;
        let mut shm_view: usize = 0;

        #[cfg(windows)]
        let mut shm = if use_shm {
            unsafe { win_shm::SharedMapping::acquire() }
        } else {
            None
        };

        let ft = {
            #[cfg(windows)]
            if let Some(ref mut mapping) = shm {
                shm_handle = mapping.handle;
                shm_view = mapping.base as usize;
                let ft_byte_len = HALF_DIMENSIONS * 2 + THREAT_AND_PP_WEIGHT_SIZE + PSQ_WEIGHT_SIZE;
                if offset + ft_byte_len > bytes.len() {
                    unsafe { mapping.release_lock(); }
                    return Err("Unexpected EOF reading feature transformer".into());
                }

                if mapping.already_initialized {
                    let hdr = unsafe { mapping.header_mut() };
                    let ft_shared = FeatureTransformer {
                        biases: hdr.biases,
                        threat_and_pp_weights: unsafe {
                            AlignedI8Vec::from_raw_parts(mapping.thr_ptr(), THREAT_AND_PP_WEIGHT_SIZE)
                        },
                        weights: unsafe {
                            AlignedI8Vec::from_raw_parts(mapping.psq_ptr(), PSQ_WEIGHT_SIZE)
                        },
                    };
                    offset += ft_byte_len;
                    unsafe { mapping.release_lock(); }
                    ft_shared
                } else {
                    let mut ft_shared = FeatureTransformer {
                        biases: [0; HALF_DIMENSIONS],
                        threat_and_pp_weights: unsafe {
                            AlignedI8Vec::from_raw_parts(mapping.thr_ptr(), THREAT_AND_PP_WEIGHT_SIZE)
                        },
                        weights: unsafe {
                            AlignedI8Vec::from_raw_parts(mapping.psq_ptr(), PSQ_WEIGHT_SIZE)
                        },
                    };
                    for b in &mut ft_shared.biases {
                        *b = read_u16(&mut offset)? as i16;
                    }
                    let thr_slice: &[i8] = unsafe {
                        core::slice::from_raw_parts(bytes.as_ptr().add(offset) as *const i8, THREAT_AND_PP_WEIGHT_SIZE)
                    };
                    ft_shared.threat_and_pp_weights.copy_from_slice(thr_slice);
                    offset += THREAT_AND_PP_WEIGHT_SIZE;

                    let psq_slice: &[i8] = unsafe {
                        core::slice::from_raw_parts(bytes.as_ptr().add(offset) as *const i8, PSQ_WEIGHT_SIZE)
                    };
                    ft_shared.weights.copy_from_slice(psq_slice);
                    offset += PSQ_WEIGHT_SIZE;

                    ft_shared.permute_weights();
                    unsafe {
                        mapping.mark_ready(&ft_shared.biases);
                        mapping.release_lock();
                    }
                    ft_shared
                }
            } else {
                let mut ft_owned = FeatureTransformer::new();
                for b in &mut ft_owned.biases {
                    *b = read_u16(&mut offset)? as i16;
                }
                let thr_pp_len = THREAT_AND_PP_WEIGHT_SIZE;
                if offset + thr_pp_len > bytes.len() {
                    return Err("Unexpected EOF reading threat weights".into());
                }
                let thr_slice: &[i8] = unsafe {
                    core::slice::from_raw_parts(bytes.as_ptr().add(offset) as *const i8, thr_pp_len)
                };
                ft_owned.threat_and_pp_weights.copy_from_slice(thr_slice);
                offset += thr_pp_len;

                let psq_len = PSQ_WEIGHT_SIZE;
                if offset + psq_len > bytes.len() {
                    return Err("Unexpected EOF reading PSQ weights".into());
                }
                let psq_slice: &[i8] = unsafe {
                    core::slice::from_raw_parts(bytes.as_ptr().add(offset) as *const i8, psq_len)
                };
                ft_owned.weights.copy_from_slice(psq_slice);
                offset += psq_len;

                ft_owned.permute_weights();
                ft_owned
            }
            #[cfg(not(windows))]
            {
                let _ = use_shm;
                let mut ft_owned = FeatureTransformer::new();
                for b in &mut ft_owned.biases {
                    *b = read_u16(&mut offset)? as i16;
                }
                let thr_pp_len = THREAT_AND_PP_WEIGHT_SIZE;
                if offset + thr_pp_len > bytes.len() {
                    return Err("Unexpected EOF reading threat weights".into());
                }
                let thr_slice: &[i8] = unsafe {
                    core::slice::from_raw_parts(bytes.as_ptr().add(offset) as *const i8, thr_pp_len)
                };
                ft_owned.threat_and_pp_weights.copy_from_slice(thr_slice);
                offset += thr_pp_len;

                let psq_len = PSQ_WEIGHT_SIZE;
                if offset + psq_len > bytes.len() {
                    return Err("Unexpected EOF reading PSQ weights".into());
                }
                let psq_slice: &[i8] = unsafe {
                    core::slice::from_raw_parts(bytes.as_ptr().add(offset) as *const i8, psq_len)
                };
                ft_owned.weights.copy_from_slice(psq_slice);
                offset += psq_len;

                ft_owned.permute_weights();
                ft_owned
            }
        };

        // Layer stacks
        let mut network_stacks = Vec::with_capacity(stored_stacks);
        for _ in 0..stored_stacks {
            let _net_hash = read_u32(&mut offset)?;
            let mut arch = NetworkArchitecture::new();

            // fc_0: biases [i32; 32]
            for b in &mut arch.fc_0.biases {
                *b = read_u32(&mut offset)? as i32;
            }
            // fc_0: weights [i8; 32 * 1024]
            for i in 0..(32 * 1024) {
                let w = bytes[offset] as i8;
                offset += 1;
                arch.fc_0.weights[affine_transform_sparse_input::get_weight_index(i)] = w;
            }

            // fc_1: biases [i32; 32]
            for b in &mut arch.fc_1.biases.0 {
                *b = read_u32(&mut offset)? as i32;
            }
            // fc_1: weights [i8; 32 * 64]
            for i in 0..(32 * 64) {
                let w = bytes[offset] as i8;
                offset += 1;
                arch.fc_1.weights[affine_transform::get_weight_index::<64, 32>(i)] = w;
            }

            // fc_2: biases [i32; 1]
            arch.fc_2.biases[0] = read_u32(&mut offset)? as i32;
            // fc_2: weights [i8; 1 * 128]
            for i in 0..128 {
                let w = bytes[offset] as i8;
                offset += 1;
                arch.fc_2.weights[affine_transform::get_weight_index::<128, 1>(i)] = w;
            }

            network_stacks.push(arch);
        }

        if offset != bytes.len() {
            return Err(format!("Trailing bytes: {} remaining", bytes.len() - offset));
        }

        Ok(Box::new(Self {
            feature_transformer: ft,
            stored_layer_stacks: stored_stacks,
            layer_stack_fallback: fallback,
            layer_stack_index,
            network: network_stacks,
            _shm_handle: shm_handle,
            _shm_view: shm_view,
        }))
    }

    #[inline(always)]
    pub fn layer_stack(&self, pos: &Position) -> usize {
        let stm = pos.side_to_move();
        let band = (pos.count_all() - 1) / 4;
        let queens = 2 * (pos.pieces_color(PieceType::Queen, stm) != 0) as usize
            + (pos.pieces_color(PieceType::Queen, !stm) != 0) as usize;

        const QUEEN_SIDE: Bitboard = FILE_A_BB | FILE_B_BB | FILE_C_BB | FILE_D_BB;
        let pawns = pos.pieces_type(PieceType::Pawn);
        let layout = if pawns == 0 {
            2
        } else if (pawns & QUEEN_SIDE) != 0 && (pawns & !QUEEN_SIDE) != 0 {
            0
        } else {
            1
        };

        let officers = |c: Color| -> usize {
            (pos.pieces_c(c) & !pos.pieces_2(PieceType::Pawn, PieceType::King)).count_ones() as usize
        };
        let many = officers(Color::White).max(officers(Color::Black)) >= 2;

        let raw_idx = ((band * 4 + queens) * 3 + layout) * 2 + (many as usize);
        self.layer_stack_index[raw_idx] as usize
    }

    #[inline(never)]
    pub unsafe fn evaluate(
        &self,
        pos: &Position,
        accumulator_stack: &mut AccumulatorStack,
        cache: &mut AccumulatorCaches,
    ) -> Value {
        #[repr(align(64))]
        struct AlignedFeatures(core::mem::MaybeUninit<[u8; L1]>);

        let mut transformed_features = AlignedFeatures(core::mem::MaybeUninit::uninit());
        let mut nnz_uninit = core::mem::MaybeUninit::<NNZInfo<L1>>::uninit();
        let nnz_info = &mut *nnz_uninit.as_mut_ptr();

        let bucket = self.layer_stack(pos);
        let feat_slice = &mut *(transformed_features.0.as_mut_ptr());
        accumulator_stack.evaluate(pos, &self.feature_transformer, cache);
        let acc_state = accumulator_stack.latest();
        let stm = pos.side_to_move();
        self.feature_transformer.transform_perspective(
            &acc_state.accumulation[stm as usize],
            feat_slice,
            0,
            nnz_info,
        );
        self.feature_transformer.transform_perspective(
            &acc_state.accumulation[(!stm) as usize],
            feat_slice,
            1,
            nnz_info,
        );
        let positional = self.network.get_unchecked(bucket).propagate(feat_slice, nnz_info);
        (positional / OUTPUT_SCALE) as Value
    }
}

pub static GLOBAL_NETWORK: std::sync::LazyLock<Box<Network>> = std::sync::LazyLock::new(Network::load_embedded);

pub fn get_network() -> &'static Network {
    &GLOBAL_NETWORK
}
