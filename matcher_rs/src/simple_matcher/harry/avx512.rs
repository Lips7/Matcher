use std::arch::x86_64::*;

use super::{HarryMatcher, MASK_ROWS, MAX_SCAN_LEN};

impl HarryMatcher {
    /// AVX512-VBMI column-vector scan kernel (general path).
    ///
    /// Processes M=56 haystack positions per iteration using `_mm512_permutexvar_epi8`.
    ///
    /// # Safety
    ///
    /// Requires x86-64 with AVX512F + AVX512BW + AVX512VBMI. The caller must verify
    /// feature support at runtime via `is_x86_feature_detected!("avx512vbmi")` before
    /// calling. Pointer arithmetic is bounded by the loop condition.
    #[target_feature(enable = "avx512f,avx512bw,avx512vbmi")]
    pub(super) unsafe fn scan_avx512vbmi(
        &self,
        haystack: &[u8],
        on_value: &mut impl FnMut(u32) -> bool,
    ) -> bool {
        const M: usize = 56;

        if haystack.len() < M + MAX_SCAN_LEN - 1 {
            return self.scan_scalar_range(haystack, 0, haystack.len() - 1, on_value);
        }

        // Pre-load all column mask tables (stack-allocated, no heap allocation).
        let low_cols: [__m512i; MAX_SCAN_LEN] = std::array::from_fn(|column| unsafe {
            _mm512_loadu_si512(self.low_mask[column].as_ptr().cast())
        });
        let high_cols: [__m512i; MAX_SCAN_LEN] = std::array::from_fn(|column| unsafe {
            _mm512_loadu_si512(self.high_mask[column].as_ptr().cast())
        });

        let shift_idx: [__m512i; MAX_SCAN_LEN] = std::array::from_fn(|shift| {
            let mut idx = [0u8; 64];
            for (lane, slot) in idx.iter_mut().enumerate().take(M) {
                *slot = (lane + shift) as u8;
            }
            unsafe { _mm512_loadu_si512(idx.as_ptr().cast()) }
        });

        let mask_6b = unsafe { _mm512_set1_epi8(0x3F_i8) };
        // Mask for the 63 valid bytes (bits 0..62); lane 63 stays as 0xFF from all_ff.
        let valid_mask: u64 = (1u64 << 63) - 1;
        let all_ff = unsafe { _mm512_set1_epi8(-1_i8) };
        let valid_lane_mask: u64 = (1u64 << M) - 1;
        let mut start = 0usize;

        while start + M + MAX_SCAN_LEN - 1 <= haystack.len() {
            // Load exactly 63 valid haystack bytes; lane 63 padded with 0xFF.
            let raw = unsafe {
                _mm512_mask_loadu_epi8(all_ff, valid_mask, haystack.as_ptr().add(start).cast())
            };
            let low_idx = unsafe { _mm512_and_si512(raw, mask_6b) };
            // _mm512_srli_epi16 shifts each 16-bit lane right — the AND with mask_6b
            // ensures only bits [1:6] of the original byte survive.
            let high_idx = unsafe { _mm512_and_si512(_mm512_srli_epi16(raw, 1), mask_6b) };

            // Column 0: no alignment shift.
            let lo0 = unsafe { _mm512_permutexvar_epi8(low_idx, low_cols[0]) };
            let hi0 = unsafe { _mm512_permutexvar_epi8(high_idx, high_cols[0]) };
            let mut state = unsafe { _mm512_or_si512(lo0, hi0) };

            // UTF-8 continuation-byte mask: mark lanes starting at continuation
            // bytes (0x80-0xBF) as "no match". A valid match can never begin at a
            // continuation byte. Skipped when all patterns are ASCII (column scan
            // self-filters since non-ASCII bytes never have bucket bits set).
            if !self.all_patterns_ascii {
                let cont_mask = unsafe {
                    let masked = _mm512_and_si512(raw, _mm512_set1_epi8(0xC0_u8 as i8));
                    _mm512_cmpeq_epi8_mask(masked, _mm512_set1_epi8(0x80_u8 as i8))
                };
                state = unsafe { _mm512_mask_set1_epi8(state, cont_mask, -1_i8) };
            }

            // Early exit: if no valid lane has any bucket bit cleared after column 0
            // + continuation mask, no bucket can match any start position in this chunk.
            if unsafe { _mm512_cmpneq_epi8_mask(state, all_ff) as u64 } & valid_lane_mask == 0 {
                start += M;
                continue;
            }

            unsafe {
                for column in 1..self.max_prefix_len {
                    let lo_lookup = _mm512_permutexvar_epi8(low_idx, low_cols[column]);
                    let lo_aligned = _mm512_permutexvar_epi8(shift_idx[column], lo_lookup);
                    let hi_lookup = _mm512_permutexvar_epi8(high_idx, high_cols[column]);
                    let hi_aligned = _mm512_permutexvar_epi8(shift_idx[column], hi_lookup);
                    state = _mm512_or_si512(state, _mm512_or_si512(lo_aligned, hi_aligned));
                }
            }

            // Compute a lane-hit bitmask without touching memory.
            let lane_hits: u64 =
                unsafe { _mm512_cmpneq_epi8_mask(state, all_ff) as u64 } & valid_lane_mask;

            if lane_hits != 0 {
                let mut state_buf = [0u8; 64];
                unsafe { _mm512_storeu_si512(state_buf.as_mut_ptr().cast(), state) };

                let mut remaining = lane_hits;
                while remaining != 0 {
                    let lane = remaining.trailing_zeros() as usize;
                    remaining &= remaining - 1;
                    let hit_mask = !state_buf[lane];
                    debug_assert!(hit_mask != 0);
                    if self.verify_hits(haystack, start + lane, hit_mask, on_value) {
                        return true;
                    }
                }
            }

            start += M;
        }

        self.scan_scalar_range(haystack, start, haystack.len() - 1, on_value)
    }

    /// AVX512 kernel for ASCII-only pattern sets.
    ///
    /// Loads 64 bytes per iteration and checks bit 7 via `_mm512_movepi8_mask`.
    /// When all 64 bytes are non-ASCII, skips the full 64-byte window.
    ///
    /// # Safety
    ///
    /// Requires runtime-confirmed AVX512F + AVX512BW + AVX512VBMI. Additionally
    /// relies on `all_patterns_ascii` being correctly set.
    #[target_feature(enable = "avx512f,avx512bw,avx512vbmi")]
    pub(super) unsafe fn scan_avx512vbmi_ascii(
        &self,
        haystack: &[u8],
        on_value: &mut impl FnMut(u32) -> bool,
    ) -> bool {
        const M: usize = 56;

        if haystack.len() < M + MAX_SCAN_LEN - 1 {
            return self.scan_scalar_range_ascii(haystack, 0, haystack.len() - 1, on_value);
        }

        let low_cols: [__m512i; MAX_SCAN_LEN] = std::array::from_fn(|column| unsafe {
            _mm512_loadu_si512(self.low_mask[column].as_ptr().cast())
        });
        let high_cols: [__m512i; MAX_SCAN_LEN] = std::array::from_fn(|column| unsafe {
            _mm512_loadu_si512(self.high_mask[column].as_ptr().cast())
        });

        let shift_idx: [__m512i; MAX_SCAN_LEN] = std::array::from_fn(|shift| {
            let mut idx = [0u8; 64];
            for (lane, slot) in idx.iter_mut().enumerate().take(M) {
                *slot = (lane + shift) as u8;
            }
            unsafe { _mm512_loadu_si512(idx.as_ptr().cast()) }
        });

        let mask_6b = unsafe { _mm512_set1_epi8(0x3F_i8) };
        let valid_mask: u64 = (1u64 << 63) - 1;
        let all_ff = unsafe { _mm512_set1_epi8(-1_i8) };
        let valid_lane_mask: u64 = (1u64 << M) - 1;
        let mut start = 0usize;

        while start + M + MAX_SCAN_LEN - 1 <= haystack.len() {
            let raw = unsafe {
                _mm512_mask_loadu_epi8(all_ff, valid_mask, haystack.as_ptr().add(start).cast())
            };

            // ASCII fast path: _mm512_movepi8_mask extracts the sign bit (bit 7) of
            // each byte into a 64-bit mask. If all valid-lane bits are set, every byte
            // is ≥ 0x80 (non-ASCII), so no ASCII pattern can match here.
            let sign_mask = unsafe { _mm512_movepi8_mask(raw) } & valid_mask;
            if sign_mask == valid_mask {
                // All 63 valid bytes are non-ASCII — skip the full window.
                start += M;
                continue;
            }

            let low_idx = unsafe { _mm512_and_si512(raw, mask_6b) };
            let high_idx = unsafe { _mm512_and_si512(_mm512_srli_epi16(raw, 1), mask_6b) };

            let lo0 = unsafe { _mm512_permutexvar_epi8(low_idx, low_cols[0]) };
            let hi0 = unsafe { _mm512_permutexvar_epi8(high_idx, high_cols[0]) };
            let mut state = unsafe { _mm512_or_si512(lo0, hi0) };

            if unsafe { _mm512_cmpneq_epi8_mask(state, all_ff) as u64 } & valid_lane_mask == 0 {
                start += M;
                continue;
            }

            unsafe {
                for column in 1..self.max_prefix_len {
                    let lo_lookup = _mm512_permutexvar_epi8(low_idx, low_cols[column]);
                    let lo_aligned = _mm512_permutexvar_epi8(shift_idx[column], lo_lookup);
                    let hi_lookup = _mm512_permutexvar_epi8(high_idx, high_cols[column]);
                    let hi_aligned = _mm512_permutexvar_epi8(shift_idx[column], hi_lookup);
                    state = _mm512_or_si512(state, _mm512_or_si512(lo_aligned, hi_aligned));
                }
            }

            let lane_hits: u64 =
                unsafe { _mm512_cmpneq_epi8_mask(state, all_ff) as u64 } & valid_lane_mask;

            if lane_hits != 0 {
                // Filter out hits at non-ASCII start positions.
                let ascii_lane_mask = !sign_mask & valid_lane_mask;
                let filtered_hits = lane_hits & ascii_lane_mask;

                if filtered_hits != 0 {
                    let mut state_buf = [0u8; 64];
                    unsafe { _mm512_storeu_si512(state_buf.as_mut_ptr().cast(), state) };

                    let mut remaining = filtered_hits;
                    while remaining != 0 {
                        let lane = remaining.trailing_zeros() as usize;
                        remaining &= remaining - 1;
                        let hit_mask = !state_buf[lane];
                        debug_assert!(hit_mask != 0);
                        if self.verify_hits(haystack, start + lane, hit_mask, on_value) {
                            return true;
                        }
                    }
                }
            }

            start += M;
        }

        self.scan_scalar_range_ascii(haystack, start, haystack.len() - 1, on_value)
    }
}
