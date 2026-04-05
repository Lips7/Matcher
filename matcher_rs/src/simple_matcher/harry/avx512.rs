//! AVX512 scan kernels for [`HarryMatcher`].
//!
//! Requires `avx512f`, `avx512bw`, and `avx512vbmi` — confirmed at runtime via
//! [`ScanPlan`](crate::simple_matcher::engine::ScanPlan) before dispatch.
//! Processes 56 positions per chunk (64-byte SIMD register minus
//! `max_prefix_len` overlap). All functions are `pub(super) unsafe`.

use std::arch::x86_64::*;

use super::{HarryMatcher, MAX_SCAN_LEN};

impl HarryMatcher {
    /// AVX512 fast path for ASCII haystacks when matching single-byte patterns.
    ///
    /// # Safety
    ///
    /// Requires runtime-confirmed AVX512F + AVX512BW + AVX512VBMI support.
    #[target_feature(enable = "avx512f,avx512bw,avx512vbmi")]
    pub(super) unsafe fn scan_single_byte_any_ascii_haystack_avx512(
        &self,
        haystack: &[u8],
    ) -> bool {
        let keys = &self.single_byte_keys;
        debug_assert!(!keys.is_empty() && keys.len() <= 4);

        let k0 = _mm512_set1_epi8(keys[0] as i8);
        let k1 = (keys.len() > 1).then(|| _mm512_set1_epi8(keys[1] as i8));
        let k2 = (keys.len() > 2).then(|| _mm512_set1_epi8(keys[2] as i8));
        let k3 = (keys.len() > 3).then(|| _mm512_set1_epi8(keys[3] as i8));
        let mut i = 0usize;

        while i + 64 <= haystack.len() {
            // SAFETY: `i + 64 <= haystack.len()` guarantees the 64-byte unaligned load is in bounds.
            let raw = unsafe { _mm512_loadu_si512(haystack.as_ptr().add(i).cast()) };
            let mut hits = _mm512_cmpeq_epi8_mask(raw, k0);
            if let Some(key) = k1 {
                hits |= _mm512_cmpeq_epi8_mask(raw, key);
            }
            if let Some(key) = k2 {
                hits |= _mm512_cmpeq_epi8_mask(raw, key);
            }
            if let Some(key) = k3 {
                hits |= _mm512_cmpeq_epi8_mask(raw, key);
            }
            if hits != 0 {
                return true;
            }
            i += 64;
        }

        haystack[i..]
            .iter()
            .copied()
            .any(|byte| self.single_byte_contains(byte))
    }

    /// AVX512 fast path for dispatching ASCII single-byte literal values.
    ///
    /// # Safety
    ///
    /// Requires runtime-confirmed AVX512F + AVX512BW + AVX512VBMI support.
    #[target_feature(enable = "avx512f,avx512bw,avx512vbmi")]
    pub(super) unsafe fn scan_single_byte_literals_ascii_avx512(
        &self,
        haystack: &[u8],
        on_value: &mut impl FnMut(u32) -> bool,
    ) -> bool {
        let mut i = 0usize;

        while i + 64 <= haystack.len() {
            // SAFETY: `i + 64 <= haystack.len()` guarantees the 64-byte unaligned load is in bounds.
            let raw = unsafe { _mm512_loadu_si512(haystack.as_ptr().add(i).cast()) };
            let sign_mask = _mm512_movepi8_mask(raw);
            if sign_mask == u64::MAX {
                i += 64;
                continue;
            }

            let end = (i + 64).min(haystack.len());
            while i < end {
                let byte = haystack[i];
                if byte < 128 {
                    for &value in &self.single_byte_values[byte as usize] {
                        if on_value(value) {
                            return true;
                        }
                    }
                }
                i += 1;
            }
        }

        while i < haystack.len() {
            let byte = haystack[i];
            if byte < 128 {
                for &value in &self.single_byte_values[byte as usize] {
                    if on_value(value) {
                        return true;
                    }
                }
            }
            i += 1;
        }

        false
    }

    /// AVX512-VBMI scan that only reports whether any match exists.
    ///
    /// # Safety
    ///
    /// Requires runtime-confirmed AVX512F + AVX512BW + AVX512VBMI support.
    #[target_feature(enable = "avx512f,avx512bw,avx512vbmi")]
    pub(super) unsafe fn scan_avx512vbmi_any(&self, haystack: &[u8]) -> bool {
        // SAFETY: The caller guarantees the required AVX512 features; dispatch only selects
        // the const-generic specialization for the already-validated matcher shape.
        unsafe {
            match self.max_prefix_len {
                2 => self.scan_avx512vbmi_inner_any::<2, false>(haystack),
                3 => self.scan_avx512vbmi_inner_any::<3, false>(haystack),
                4 => self.scan_avx512vbmi_inner_any::<4, false>(haystack),
                5 => self.scan_avx512vbmi_inner_any::<5, false>(haystack),
                6 => self.scan_avx512vbmi_inner_any::<6, false>(haystack),
                7 => self.scan_avx512vbmi_inner_any::<7, false>(haystack),
                _ => self.scan_avx512vbmi_inner_any::<8, false>(haystack),
            }
        }
    }

    /// AVX512-VBMI scan for ASCII-only pattern sets that only reports whether any match exists.
    ///
    /// # Safety
    ///
    /// Requires runtime-confirmed AVX512F + AVX512BW + AVX512VBMI support.
    #[target_feature(enable = "avx512f,avx512bw,avx512vbmi")]
    pub(super) unsafe fn scan_avx512vbmi_ascii_any(&self, haystack: &[u8]) -> bool {
        // SAFETY: The caller guarantees the required AVX512 features; dispatch only selects
        // the const-generic specialization for the already-validated matcher shape.
        unsafe {
            match self.max_prefix_len {
                2 => self.scan_avx512vbmi_inner_any::<2, true>(haystack),
                3 => self.scan_avx512vbmi_inner_any::<3, true>(haystack),
                4 => self.scan_avx512vbmi_inner_any::<4, true>(haystack),
                5 => self.scan_avx512vbmi_inner_any::<5, true>(haystack),
                6 => self.scan_avx512vbmi_inner_any::<6, true>(haystack),
                7 => self.scan_avx512vbmi_inner_any::<7, true>(haystack),
                _ => self.scan_avx512vbmi_inner_any::<8, true>(haystack),
            }
        }
    }

    /// AVX512-VBMI scan for ASCII-leading pattern sets that only reports whether any match exists.
    ///
    /// # Safety
    ///
    /// Requires runtime-confirmed AVX512F + AVX512BW + AVX512VBMI support.
    #[target_feature(enable = "avx512f,avx512bw,avx512vbmi")]
    pub(super) unsafe fn scan_avx512vbmi_ascii_lead_any(&self, haystack: &[u8]) -> bool {
        // SAFETY: The caller guarantees the required AVX512 features; dispatch only selects
        // the const-generic specialization for the already-validated matcher shape.
        unsafe {
            match self.max_prefix_len {
                2 => self.scan_avx512vbmi_inner_ascii_lead_any::<2>(haystack),
                3 => self.scan_avx512vbmi_inner_ascii_lead_any::<3>(haystack),
                4 => self.scan_avx512vbmi_inner_ascii_lead_any::<4>(haystack),
                5 => self.scan_avx512vbmi_inner_ascii_lead_any::<5>(haystack),
                6 => self.scan_avx512vbmi_inner_ascii_lead_any::<6>(haystack),
                7 => self.scan_avx512vbmi_inner_ascii_lead_any::<7>(haystack),
                _ => self.scan_avx512vbmi_inner_ascii_lead_any::<8>(haystack),
            }
        }
    }

    /// AVX512-VBMI column-vector scan kernel (general path).
    ///
    /// Dispatches to a const-generic inner kernel based on `max_prefix_len`.
    ///
    /// # Safety
    ///
    /// Requires x86-64 with AVX512F + AVX512BW + AVX512VBMI. The caller must verify
    /// feature support at runtime via `is_x86_feature_detected!("avx512vbmi")` before
    /// calling.
    #[target_feature(enable = "avx512f,avx512bw,avx512vbmi")]
    pub(super) unsafe fn scan_avx512vbmi(
        &self,
        haystack: &[u8],
        on_value: &mut impl FnMut(u32) -> bool,
    ) -> bool {
        // SAFETY: The caller guarantees the required AVX512 features; dispatch only selects
        // the const-generic specialization for the already-validated matcher shape.
        unsafe {
            match self.max_prefix_len {
                2 => self.scan_avx512vbmi_inner::<2, false>(haystack, on_value),
                3 => self.scan_avx512vbmi_inner::<3, false>(haystack, on_value),
                4 => self.scan_avx512vbmi_inner::<4, false>(haystack, on_value),
                5 => self.scan_avx512vbmi_inner::<5, false>(haystack, on_value),
                6 => self.scan_avx512vbmi_inner::<6, false>(haystack, on_value),
                7 => self.scan_avx512vbmi_inner::<7, false>(haystack, on_value),
                _ => self.scan_avx512vbmi_inner::<8, false>(haystack, on_value),
            }
        }
    }

    /// AVX512 kernel for ASCII-only pattern sets.
    ///
    /// Dispatches to a const-generic inner kernel based on `max_prefix_len`.
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
        // SAFETY: The caller guarantees the required AVX512 features; dispatch only selects
        // the const-generic specialization for the already-validated matcher shape.
        unsafe {
            match self.max_prefix_len {
                2 => self.scan_avx512vbmi_inner::<2, true>(haystack, on_value),
                3 => self.scan_avx512vbmi_inner::<3, true>(haystack, on_value),
                4 => self.scan_avx512vbmi_inner::<4, true>(haystack, on_value),
                5 => self.scan_avx512vbmi_inner::<5, true>(haystack, on_value),
                6 => self.scan_avx512vbmi_inner::<6, true>(haystack, on_value),
                7 => self.scan_avx512vbmi_inner::<7, true>(haystack, on_value),
                _ => self.scan_avx512vbmi_inner::<8, true>(haystack, on_value),
            }
        }
    }

    /// Const-generic inner kernel parameterised by prefix length and ASCII mode.
    ///
    /// - `PREFIX_LEN`: number of columns to scan (2..=8).
    /// - `ASCII_ONLY`: when `true`, enables the 64-byte all-non-ASCII fast skip.
    ///
    /// Processes M=56 haystack positions per iteration using `_mm512_permutexvar_epi8`.
    ///
    /// # Safety
    ///
    /// Requires x86-64 with AVX512F + AVX512BW + AVX512VBMI. Pointer arithmetic is
    /// bounded by the loop condition.
    #[target_feature(enable = "avx512f,avx512bw,avx512vbmi")]
    unsafe fn scan_avx512vbmi_inner<const PREFIX_LEN: usize, const ASCII_ONLY: bool>(
        &self,
        haystack: &[u8],
        on_value: &mut impl FnMut(u32) -> bool,
    ) -> bool {
        const { assert!(PREFIX_LEN >= 2 && PREFIX_LEN <= MAX_SCAN_LEN) };
        const M: usize = 56;

        if haystack.len() < M + MAX_SCAN_LEN - 1 {
            return if ASCII_ONLY {
                self.scan_scalar_range_ascii(haystack, 0, haystack.len() - 1, on_value)
            } else {
                self.scan_scalar_range(haystack, 0, haystack.len() - 1, on_value)
            };
        }

        // SAFETY: All AVX512 intrinsics below require AVX512F + AVX512BW + AVX512VBMI,
        // guaranteed by this function's #[target_feature] attribute. Pointer arithmetic
        // is bounded by the loop condition `start + M + MAX_SCAN_LEN - 1 <= haystack.len()`.
        unsafe {
            let low_cols: [__m512i; PREFIX_LEN] = std::array::from_fn(|column| {
                _mm512_loadu_si512(self.low_mask[column].as_ptr().cast())
            });
            let high_cols: [__m512i; PREFIX_LEN] = std::array::from_fn(|column| {
                _mm512_loadu_si512(self.high_mask[column].as_ptr().cast())
            });

            let shift_idx: [__m512i; PREFIX_LEN] = std::array::from_fn(|shift| {
                let mut idx = [0u8; 64];
                for (lane, slot) in idx.iter_mut().enumerate().take(M) {
                    *slot = (lane + shift) as u8;
                }
                _mm512_loadu_si512(idx.as_ptr().cast())
            });

            let mask_6b = _mm512_set1_epi8(0x3F_i8);
            let valid_mask: u64 = (1u64 << 63) - 1;
            let all_ff = _mm512_set1_epi8(-1_i8);
            let valid_lane_mask: u64 = (1u64 << M) - 1;
            let mut start = 0usize;

            while start + M + MAX_SCAN_LEN - 1 <= haystack.len() {
                let raw =
                    _mm512_mask_loadu_epi8(all_ff, valid_mask, haystack.as_ptr().add(start).cast());

                // ── ASCII-only fast path: skip 63 bytes if all non-ASCII ──
                if ASCII_ONLY {
                    let sign_mask = _mm512_movepi8_mask(raw) & valid_mask;
                    if sign_mask == valid_mask {
                        start += M;
                        continue;
                    }
                }

                let low_idx = _mm512_and_si512(raw, mask_6b);
                let high_idx = _mm512_and_si512(_mm512_srli_epi16(raw, 1), mask_6b);

                // Column 0: no alignment shift.
                let lo0 = _mm512_permutexvar_epi8(low_idx, low_cols[0]);
                let hi0 = _mm512_permutexvar_epi8(high_idx, high_cols[0]);
                let mut state = _mm512_or_si512(lo0, hi0);

                // UTF-8 continuation-byte mask (general mode only).
                if !ASCII_ONLY && !self.all_patterns_ascii {
                    let masked = _mm512_and_si512(raw, _mm512_set1_epi8(0xC0_u8 as i8));
                    let cont_mask = _mm512_cmpeq_epi8_mask(masked, _mm512_set1_epi8(0x80_u8 as i8));
                    state = _mm512_mask_set1_epi8(state, cont_mask, -1_i8);
                }

                // ── Column-0 early exit ──
                if _mm512_cmpneq_epi8_mask(state, all_ff) as u64 & valid_lane_mask == 0 {
                    start += M;
                    continue;
                }

                // ── Apply remaining columns via static dispatch ──
                macro_rules! apply_col_avx {
                    ($col:literal) => {{
                        let lo_lookup = _mm512_permutexvar_epi8(low_idx, low_cols[$col]);
                        let hi_lookup = _mm512_permutexvar_epi8(high_idx, high_cols[$col]);
                        let combined = _mm512_or_si512(lo_lookup, hi_lookup);
                        let aligned = _mm512_permutexvar_epi8(shift_idx[$col], combined);
                        state = _mm512_or_si512(state, aligned);
                    }};
                }

                apply_col_avx!(1);

                // ── Column-1 progressive early exit ──
                // After columns 0+1, check again. On non-ASCII patterns where column 0
                // is ~50% selective (bit 7 lost), 0+1 together may reach ~90%.
                if PREFIX_LEN >= 3
                    && !self.all_patterns_ascii
                    && _mm512_cmpneq_epi8_mask(state, all_ff) as u64 & valid_lane_mask == 0
                {
                    start += M;
                    continue;
                }

                if PREFIX_LEN >= 3 {
                    apply_col_avx!(2);
                }
                if PREFIX_LEN >= 4 {
                    apply_col_avx!(3);
                }
                if PREFIX_LEN >= 5 {
                    apply_col_avx!(4);
                }
                if PREFIX_LEN >= 6 {
                    apply_col_avx!(5);
                }
                if PREFIX_LEN >= 7 {
                    apply_col_avx!(6);
                }
                if PREFIX_LEN >= 8 {
                    apply_col_avx!(7);
                }

                // ── Verify hits ──
                let lane_hits: u64 =
                    _mm512_cmpneq_epi8_mask(state, all_ff) as u64 & valid_lane_mask;

                if lane_hits != 0 {
                    if ASCII_ONLY {
                        let sign_mask = _mm512_movepi8_mask(raw) & valid_lane_mask;
                        let filtered_hits = lane_hits & !sign_mask;

                        if filtered_hits != 0 {
                            let mut state_buf = [0u8; 64];
                            _mm512_storeu_si512(state_buf.as_mut_ptr().cast(), state);

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
                    } else {
                        let mut state_buf = [0u8; 64];
                        _mm512_storeu_si512(state_buf.as_mut_ptr().cast(), state);

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
                }

                start += M;
            }

            if ASCII_ONLY {
                self.scan_scalar_range_ascii(haystack, start, haystack.len() - 1, on_value)
            } else {
                self.scan_scalar_range(haystack, start, haystack.len() - 1, on_value)
            }
        }
    }

    #[target_feature(enable = "avx512f,avx512bw,avx512vbmi")]
    unsafe fn scan_avx512vbmi_inner_any<const PREFIX_LEN: usize, const ASCII_ONLY: bool>(
        &self,
        haystack: &[u8],
    ) -> bool {
        const { assert!(PREFIX_LEN >= 2 && PREFIX_LEN <= MAX_SCAN_LEN) };
        const M: usize = 56;

        if haystack.len() < M + MAX_SCAN_LEN - 1 {
            return if ASCII_ONLY {
                self.scan_scalar_range_any_ascii(haystack, 0, haystack.len() - 1)
            } else {
                self.scan_scalar_range_any(haystack, 0, haystack.len() - 1)
            };
        }

        // SAFETY: All AVX512 intrinsics below require AVX512F + AVX512BW + AVX512VBMI,
        // guaranteed by this function's #[target_feature] attribute. Pointer arithmetic
        // is bounded by the loop condition `start + M + MAX_SCAN_LEN - 1 <= haystack.len()`.
        unsafe {
            let low_cols: [__m512i; PREFIX_LEN] = std::array::from_fn(|column| {
                _mm512_loadu_si512(self.low_mask[column].as_ptr().cast())
            });
            let high_cols: [__m512i; PREFIX_LEN] = std::array::from_fn(|column| {
                _mm512_loadu_si512(self.high_mask[column].as_ptr().cast())
            });

            let shift_idx: [__m512i; PREFIX_LEN] = std::array::from_fn(|shift| {
                let mut idx = [0u8; 64];
                for (lane, slot) in idx.iter_mut().enumerate().take(M) {
                    *slot = (lane + shift) as u8;
                }
                _mm512_loadu_si512(idx.as_ptr().cast())
            });

            let mask_6b = _mm512_set1_epi8(0x3F_i8);
            let valid_mask: u64 = (1u64 << 63) - 1;
            let all_ff = _mm512_set1_epi8(-1_i8);
            let valid_lane_mask: u64 = (1u64 << M) - 1;
            let mut start = 0usize;

            while start + M + MAX_SCAN_LEN - 1 <= haystack.len() {
                let raw =
                    _mm512_mask_loadu_epi8(all_ff, valid_mask, haystack.as_ptr().add(start).cast());

                if ASCII_ONLY {
                    let sign_mask = _mm512_movepi8_mask(raw) & valid_mask;
                    if sign_mask == valid_mask {
                        start += M;
                        continue;
                    }
                }

                let low_idx = _mm512_and_si512(raw, mask_6b);
                let high_idx = _mm512_and_si512(_mm512_srli_epi16(raw, 1), mask_6b);
                let lo0 = _mm512_permutexvar_epi8(low_idx, low_cols[0]);
                let hi0 = _mm512_permutexvar_epi8(high_idx, high_cols[0]);
                let mut state = _mm512_or_si512(lo0, hi0);

                if !ASCII_ONLY && !self.all_patterns_ascii {
                    let masked = _mm512_and_si512(raw, _mm512_set1_epi8(0xC0_u8 as i8));
                    let cont_mask = _mm512_cmpeq_epi8_mask(masked, _mm512_set1_epi8(0x80_u8 as i8));
                    state = _mm512_mask_set1_epi8(state, cont_mask, -1_i8);
                }

                if _mm512_cmpneq_epi8_mask(state, all_ff) as u64 & valid_lane_mask == 0 {
                    start += M;
                    continue;
                }

                macro_rules! apply_col_avx {
                    ($col:literal) => {{
                        let lo_lookup = _mm512_permutexvar_epi8(low_idx, low_cols[$col]);
                        let hi_lookup = _mm512_permutexvar_epi8(high_idx, high_cols[$col]);
                        let combined = _mm512_or_si512(lo_lookup, hi_lookup);
                        let aligned = _mm512_permutexvar_epi8(shift_idx[$col], combined);
                        state = _mm512_or_si512(state, aligned);
                    }};
                }

                apply_col_avx!(1);

                if PREFIX_LEN >= 3
                    && !self.all_patterns_ascii
                    && _mm512_cmpneq_epi8_mask(state, all_ff) as u64 & valid_lane_mask == 0
                {
                    start += M;
                    continue;
                }

                if PREFIX_LEN >= 3 {
                    apply_col_avx!(2);
                }
                if PREFIX_LEN >= 4 {
                    apply_col_avx!(3);
                }
                if PREFIX_LEN >= 5 {
                    apply_col_avx!(4);
                }
                if PREFIX_LEN >= 6 {
                    apply_col_avx!(5);
                }
                if PREFIX_LEN >= 7 {
                    apply_col_avx!(6);
                }
                if PREFIX_LEN >= 8 {
                    apply_col_avx!(7);
                }

                let lane_hits: u64 =
                    _mm512_cmpneq_epi8_mask(state, all_ff) as u64 & valid_lane_mask;

                if lane_hits != 0 {
                    if ASCII_ONLY {
                        let sign_mask = _mm512_movepi8_mask(raw) & valid_lane_mask;
                        let filtered_hits = lane_hits & !sign_mask;

                        if filtered_hits != 0 {
                            let mut state_buf = [0u8; 64];
                            _mm512_storeu_si512(state_buf.as_mut_ptr().cast(), state);

                            let mut remaining = filtered_hits;
                            while remaining != 0 {
                                let lane = remaining.trailing_zeros() as usize;
                                remaining &= remaining - 1;
                                let hit_mask = !state_buf[lane];
                                if hit_mask != 0
                                    && self.verify_hits_any(haystack, start + lane, hit_mask)
                                {
                                    return true;
                                }
                            }
                        }
                    } else {
                        let mut state_buf = [0u8; 64];
                        _mm512_storeu_si512(state_buf.as_mut_ptr().cast(), state);

                        let mut remaining = lane_hits;
                        while remaining != 0 {
                            let lane = remaining.trailing_zeros() as usize;
                            remaining &= remaining - 1;
                            let hit_mask = !state_buf[lane];
                            if hit_mask != 0
                                && self.verify_hits_any(haystack, start + lane, hit_mask)
                            {
                                return true;
                            }
                        }
                    }
                }

                start += M;
            }

            if ASCII_ONLY {
                self.scan_scalar_range_any_ascii(haystack, start, haystack.len() - 1)
            } else {
                self.scan_scalar_range_any(haystack, start, haystack.len() - 1)
            }
        }
    }

    #[target_feature(enable = "avx512f,avx512bw,avx512vbmi")]
    unsafe fn scan_avx512vbmi_inner_ascii_lead_any<const PREFIX_LEN: usize>(
        &self,
        haystack: &[u8],
    ) -> bool {
        const { assert!(PREFIX_LEN >= 2 && PREFIX_LEN <= MAX_SCAN_LEN) };
        const M: usize = 56;

        if haystack.len() < M + MAX_SCAN_LEN - 1 {
            return self.scan_scalar_range_any_no_single_byte(haystack, 0, haystack.len() - 1);
        }

        // SAFETY: All AVX512 intrinsics below require AVX512F + AVX512BW + AVX512VBMI,
        // guaranteed by this function's #[target_feature] attribute. Pointer arithmetic
        // is bounded by the loop condition `start + M + MAX_SCAN_LEN - 1 <= haystack.len()`.
        unsafe {
            let low_cols: [__m512i; PREFIX_LEN] = std::array::from_fn(|column| {
                _mm512_loadu_si512(self.low_mask[column].as_ptr().cast())
            });
            let high_cols: [__m512i; PREFIX_LEN] = std::array::from_fn(|column| {
                _mm512_loadu_si512(self.high_mask[column].as_ptr().cast())
            });

            let shift_idx: [__m512i; PREFIX_LEN] = std::array::from_fn(|shift| {
                let mut idx = [0u8; 64];
                for (lane, slot) in idx.iter_mut().enumerate().take(M) {
                    *slot = (lane + shift) as u8;
                }
                _mm512_loadu_si512(idx.as_ptr().cast())
            });

            let mask_6b = _mm512_set1_epi8(0x3F_i8);
            let all_ff = _mm512_set1_epi8(-1_i8);
            let valid_mask: u64 = (1u64 << 63) - 1;
            let valid_lane_mask: u64 = (1u64 << M) - 1;
            let mut start = 0usize;

            while start + M + MAX_SCAN_LEN - 1 <= haystack.len() {
                let raw =
                    _mm512_mask_loadu_epi8(all_ff, valid_mask, haystack.as_ptr().add(start).cast());
                let low_idx = _mm512_and_si512(raw, mask_6b);
                let high_idx = _mm512_and_si512(_mm512_srli_epi16(raw, 1), mask_6b);
                let lo0 = _mm512_permutexvar_epi8(low_idx, low_cols[0]);
                let hi0 = _mm512_permutexvar_epi8(high_idx, high_cols[0]);
                let mut state = _mm512_or_si512(lo0, hi0);

                if _mm512_cmpneq_epi8_mask(state, all_ff) as u64 & valid_lane_mask == 0 {
                    start += M;
                    continue;
                }

                macro_rules! apply_col_avx {
                    ($col:literal) => {{
                        let lo_lookup = _mm512_permutexvar_epi8(low_idx, low_cols[$col]);
                        let hi_lookup = _mm512_permutexvar_epi8(high_idx, high_cols[$col]);
                        let combined = _mm512_or_si512(lo_lookup, hi_lookup);
                        let aligned = _mm512_permutexvar_epi8(shift_idx[$col], combined);
                        state = _mm512_or_si512(state, aligned);
                    }};
                }

                apply_col_avx!(1);
                if PREFIX_LEN >= 3 {
                    apply_col_avx!(2);
                }
                if PREFIX_LEN >= 4 {
                    apply_col_avx!(3);
                }
                if PREFIX_LEN >= 5 {
                    apply_col_avx!(4);
                }
                if PREFIX_LEN >= 6 {
                    apply_col_avx!(5);
                }
                if PREFIX_LEN >= 7 {
                    apply_col_avx!(6);
                }
                if PREFIX_LEN >= 8 {
                    apply_col_avx!(7);
                }

                let lane_hits: u64 =
                    _mm512_cmpneq_epi8_mask(state, all_ff) as u64 & valid_lane_mask;
                if lane_hits != 0 {
                    let mut state_buf = [0u8; 64];
                    _mm512_storeu_si512(state_buf.as_mut_ptr().cast(), state);

                    let mut remaining = lane_hits;
                    while remaining != 0 {
                        let lane = remaining.trailing_zeros() as usize;
                        remaining &= remaining - 1;
                        let hit_mask = !state_buf[lane];
                        if hit_mask != 0 && self.verify_hits_any(haystack, start + lane, hit_mask) {
                            return true;
                        }
                    }
                }

                start += M;
            }

            self.scan_scalar_range_any_no_single_byte(haystack, start, haystack.len() - 1)
        }
    }
}
