//! AArch64 NEON scan kernels for [`HarryMatcher`].
//!
//! NEON is baseline on AArch64, so no runtime feature detection is needed.
//! Processes `16 - max_prefix_len + 1` positions per 16-byte chunk. All
//! functions are `pub(super) unsafe`.

use std::arch::aarch64::*;

use super::{HarryMatcher, MASK_ROWS, MAX_SCAN_LEN};

impl HarryMatcher {
    #[target_feature(enable = "neon")]
    pub(super) unsafe fn scan_single_byte_any_ascii_haystack_neon(&self, haystack: &[u8]) -> bool {
        let keys = &self.single_byte_keys;
        debug_assert!(!keys.is_empty() && keys.len() <= 4);

        let k0 = vdupq_n_u8(keys[0]);
        let k1 = (keys.len() > 1).then(|| vdupq_n_u8(keys[1]));
        let k2 = (keys.len() > 2).then(|| vdupq_n_u8(keys[2]));
        let k3 = (keys.len() > 3).then(|| vdupq_n_u8(keys[3]));
        let mut i = 0usize;

        while i + 16 <= haystack.len() {
            // SAFETY: `i + 16 <= haystack.len()` guarantees a full 16-byte load.
            let raw = unsafe { vld1q_u8(haystack.as_ptr().add(i)) };
            let mut hits = vceqq_u8(raw, k0);
            if let Some(key) = k1 {
                hits = vorrq_u8(hits, vceqq_u8(raw, key));
            }
            if let Some(key) = k2 {
                hits = vorrq_u8(hits, vceqq_u8(raw, key));
            }
            if let Some(key) = k3 {
                hits = vorrq_u8(hits, vceqq_u8(raw, key));
            }
            if vmaxvq_u8(hits) != 0 {
                return true;
            }
            i += 16;
        }

        haystack[i..]
            .iter()
            .copied()
            .any(|byte| self.single_byte_contains(byte))
    }

    #[target_feature(enable = "neon")]
    pub(super) unsafe fn scan_single_byte_literals_ascii_neon(
        &self,
        haystack: &[u8],
        on_value: &mut impl FnMut(u32) -> bool,
    ) -> bool {
        let ascii_hi_bit = vdupq_n_u8(0x80);
        let mut i = 0usize;

        while i + 16 <= haystack.len() {
            // SAFETY: `i + 16 <= haystack.len()` guarantees a full 16-byte load.
            let raw = unsafe { vld1q_u8(haystack.as_ptr().add(i)) };
            let has_ascii = vminvq_u8(vandq_u8(raw, ascii_hi_bit));
            if has_ascii == 0x80 {
                i += 16;
                continue;
            }

            let end = (i + 16).min(haystack.len());
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

    #[target_feature(enable = "neon")]
    pub(super) unsafe fn scan_neon_ascii_lead_any(&self, haystack: &[u8]) -> bool {
        // SAFETY: `scan_neon_inner_ascii_lead_any` requires NEON, guaranteed by this
        // function's `#[target_feature(enable = "neon")]`.
        unsafe {
            match self.max_prefix_len {
                2 => self.scan_neon_inner_ascii_lead_any::<2>(haystack),
                3 => self.scan_neon_inner_ascii_lead_any::<3>(haystack),
                4 => self.scan_neon_inner_ascii_lead_any::<4>(haystack),
                5 => self.scan_neon_inner_ascii_lead_any::<5>(haystack),
                6 => self.scan_neon_inner_ascii_lead_any::<6>(haystack),
                7 => self.scan_neon_inner_ascii_lead_any::<7>(haystack),
                _ => self.scan_neon_inner_ascii_lead_any::<8>(haystack),
            }
        }
    }

    #[target_feature(enable = "neon")]
    pub(super) unsafe fn scan_neon_any(&self, haystack: &[u8]) -> bool {
        // SAFETY: `scan_neon_inner_any` requires NEON, guaranteed by this function's
        // `#[target_feature(enable = "neon")]`.
        unsafe {
            match self.max_prefix_len {
                2 => self.scan_neon_inner_any::<2, false>(haystack),
                3 => self.scan_neon_inner_any::<3, false>(haystack),
                4 => self.scan_neon_inner_any::<4, false>(haystack),
                5 => self.scan_neon_inner_any::<5, false>(haystack),
                6 => self.scan_neon_inner_any::<6, false>(haystack),
                7 => self.scan_neon_inner_any::<7, false>(haystack),
                _ => self.scan_neon_inner_any::<8, false>(haystack),
            }
        }
    }

    #[target_feature(enable = "neon")]
    pub(super) unsafe fn scan_neon_ascii_any(&self, haystack: &[u8]) -> bool {
        // SAFETY: `scan_neon_inner_any` requires NEON, guaranteed by this function's
        // `#[target_feature(enable = "neon")]`.
        unsafe {
            match self.max_prefix_len {
                2 => self.scan_neon_inner_any::<2, true>(haystack),
                3 => self.scan_neon_inner_any::<3, true>(haystack),
                4 => self.scan_neon_inner_any::<4, true>(haystack),
                5 => self.scan_neon_inner_any::<5, true>(haystack),
                6 => self.scan_neon_inner_any::<6, true>(haystack),
                7 => self.scan_neon_inner_any::<7, true>(haystack),
                _ => self.scan_neon_inner_any::<8, true>(haystack),
            }
        }
    }

    /// NEON column-vector scan kernel (general path).
    ///
    /// Dispatches to a const-generic inner kernel based on `max_prefix_len`, which
    /// determines both the number of columns scanned and the lane count M per chunk.
    ///
    /// # Safety
    ///
    /// Requires AArch64 NEON (baseline on all AArch64 targets).
    #[target_feature(enable = "neon")]
    pub(super) unsafe fn scan_neon(
        &self,
        haystack: &[u8],
        on_value: &mut impl FnMut(u32) -> bool,
    ) -> bool {
        // Safety: scan_neon_inner requires NEON, guaranteed by our own
        // #[target_feature(enable = "neon")] attribute.
        unsafe {
            match self.max_prefix_len {
                2 => self.scan_neon_inner::<2, false>(haystack, on_value),
                3 => self.scan_neon_inner::<3, false>(haystack, on_value),
                4 => self.scan_neon_inner::<4, false>(haystack, on_value),
                5 => self.scan_neon_inner::<5, false>(haystack, on_value),
                6 => self.scan_neon_inner::<6, false>(haystack, on_value),
                7 => self.scan_neon_inner::<7, false>(haystack, on_value),
                _ => self.scan_neon_inner::<8, false>(haystack, on_value),
            }
        }
    }

    /// NEON kernel for ASCII-only pattern sets.
    ///
    /// When every pattern byte is ASCII, a match can only begin at an ASCII
    /// haystack byte. This kernel exploits that by checking bit 7 of each byte —
    /// if ALL 16 bytes are non-ASCII (≥ 0x80), the entire 16-byte window is skipped
    /// without any column work.
    ///
    /// # Safety
    ///
    /// Requires AArch64 NEON (baseline). Additionally relies on `all_patterns_ascii`
    /// being correctly set so that skipping non-ASCII byte runs does not miss matches.
    #[target_feature(enable = "neon")]
    pub(super) unsafe fn scan_neon_ascii(
        &self,
        haystack: &[u8],
        on_value: &mut impl FnMut(u32) -> bool,
    ) -> bool {
        // Safety: scan_neon_inner requires NEON, guaranteed by our own
        // #[target_feature(enable = "neon")] attribute.
        unsafe {
            match self.max_prefix_len {
                2 => self.scan_neon_inner::<2, true>(haystack, on_value),
                3 => self.scan_neon_inner::<3, true>(haystack, on_value),
                4 => self.scan_neon_inner::<4, true>(haystack, on_value),
                5 => self.scan_neon_inner::<5, true>(haystack, on_value),
                6 => self.scan_neon_inner::<6, true>(haystack, on_value),
                7 => self.scan_neon_inner::<7, true>(haystack, on_value),
                _ => self.scan_neon_inner::<8, true>(haystack, on_value),
            }
        }
    }

    /// Const-generic inner kernel parameterised by prefix length and ASCII mode.
    ///
    /// - `PREFIX_LEN`: number of columns to scan (2..=8). Determines the lane count
    ///   `M = 17 - PREFIX_LEN` (e.g. M=9 for 8 columns, M=15 for 2 columns).
    /// - `ASCII_ONLY`: when `true`, enables the 16-byte all-non-ASCII fast skip.
    ///   When `false`, enables the UTF-8 continuation-byte mask that eliminates
    ///   positions that can never start a valid match.
    ///
    /// # Safety
    ///
    /// Requires AArch64 NEON. Pointer arithmetic is bounded by the loop condition
    /// `start + 16 <= haystack.len()` (since `M + PREFIX_LEN - 1 = 16` always).
    #[target_feature(enable = "neon")]
    unsafe fn scan_neon_inner<const PREFIX_LEN: usize, const ASCII_ONLY: bool>(
        &self,
        haystack: &[u8],
        on_value: &mut impl FnMut(u32) -> bool,
    ) -> bool {
        const { assert!(PREFIX_LEN >= 2 && PREFIX_LEN <= MAX_SCAN_LEN) };
        // M lanes per chunk: 16-byte load minus (PREFIX_LEN - 1) overlap bytes.
        let m: usize = 17 - PREFIX_LEN;

        if haystack.len() < 16 {
            return if ASCII_ONLY {
                self.scan_scalar_range_ascii(haystack, 0, haystack.len() - 1, on_value)
            } else {
                self.scan_scalar_range(haystack, 0, haystack.len() - 1, on_value)
            };
        }

        /// Loads exactly `N` column mask tables as NEON quad-register groups.
        ///
        /// Only loads the columns that will actually be used (determined by
        /// `PREFIX_LEN`), avoiding register spills from pre-loading unused columns.
        #[inline(always)]
        unsafe fn load_cols_n<const N: usize>(
            tbl: &[[u8; MASK_ROWS]; MAX_SCAN_LEN],
        ) -> [uint8x16x4_t; N] {
            std::array::from_fn(|column| {
                let ptr = tbl[column].as_ptr();
                // SAFETY: `column < N <= MAX_SCAN_LEN`, so `tbl[column]` is valid.
                // Each `[u8; 64]` array covers offsets 0..64 — all four loads are in-bounds.
                unsafe {
                    uint8x16x4_t(
                        vld1q_u8(ptr),
                        vld1q_u8(ptr.add(16)),
                        vld1q_u8(ptr.add(32)),
                        vld1q_u8(ptr.add(48)),
                    )
                }
            })
        }
        // SAFETY: NEON is baseline on AArch64; `load_cols_n` calls `vld1q_u8`
        // on valid `[u8; 64]` arrays within `self.low_mask`.
        let low_cols: [uint8x16x4_t; PREFIX_LEN] = unsafe { load_cols_n(&self.low_mask) };
        // SAFETY: Same as above — `self.high_mask` has the same layout.
        let high_cols: [uint8x16x4_t; PREFIX_LEN] = unsafe { load_cols_n(&self.high_mask) };

        let zero = vdupq_n_u8(0);
        let mask_6b = vdupq_n_u8(0x3F);

        // Constants for ASCII-only mode (16-byte non-ASCII skip).
        let ascii_hi_bit = if ASCII_ONLY { vdupq_n_u8(0x80) } else { zero };

        // Constants for UTF-8 continuation-byte mask (general mode).
        // Continuation bytes satisfy (byte & 0xC0) == 0x80.
        let mask_c0 = if !ASCII_ONLY { vdupq_n_u8(0xC0) } else { zero };
        let val_80 = if !ASCII_ONLY { vdupq_n_u8(0x80) } else { zero };

        let mut start = 0usize;

        // Loop condition guarantees start + 16 <= haystack.len().
        while start + 16 <= haystack.len() {
            // SAFETY: loop condition guarantees haystack[start..start+16] is valid.
            let raw = unsafe { vld1q_u8(haystack.as_ptr().add(start)) };

            // ── ASCII-only fast path: skip 16 bytes if all non-ASCII ──
            if ASCII_ONLY {
                let has_ascii = vminvq_u8(vandq_u8(raw, ascii_hi_bit));
                if has_ascii == 0x80 {
                    start += 16;
                    continue;
                }
            }

            // ── Compute column-0 state ──
            let low_idx = vandq_u8(raw, mask_6b);
            let high_idx = vandq_u8(vshrq_n_u8(raw, 1), mask_6b);

            let lo0 = vqtbl4q_u8(low_cols[0], low_idx);
            let hi0 = vqtbl4q_u8(high_cols[0], high_idx);
            let mut state = vorrq_u8(lo0, hi0);

            // ── UTF-8 continuation-byte mask ──
            if !ASCII_ONLY && !self.all_patterns_ascii {
                let cont_mask = vceqq_u8(vandq_u8(raw, mask_c0), val_80);
                state = vorrq_u8(state, cont_mask);
            }

            // ── Column-0 early exit ──
            if vminvq_u8(state) == 0xFF {
                start += m;
                continue;
            }

            // ── Apply remaining columns ──
            // Each column's lookup is shifted (vextq_u8) to align with the starting
            // lane positions. Only columns 1..PREFIX_LEN are applied.
            macro_rules! apply_col {
                ($shift:literal) => {{
                    let lo = vqtbl4q_u8(low_cols[$shift], low_idx);
                    let hi = vqtbl4q_u8(high_cols[$shift], high_idx);
                    // ext(a, n) | ext(b, n) == ext(a | b, n): merge two shifts into one.
                    state = vorrq_u8(state, vextq_u8(vorrq_u8(lo, hi), zero, $shift));
                }};
            }

            apply_col!(1);

            // ── Column-1 progressive early exit ──
            // After columns 0+1, check again. On non-ASCII patterns where column 0
            // is ~50% selective (bit 7 lost), 0+1 together may reach ~90%, saving
            // the remaining column applications. Only useful when PREFIX_LEN >= 3
            // and patterns contain non-ASCII bytes (otherwise column 0 alone is
            // highly selective and this check wastes a cycle).
            if PREFIX_LEN >= 3 && !self.all_patterns_ascii && vminvq_u8(state) == 0xFF {
                start += m;
                continue;
            }

            if PREFIX_LEN >= 3 {
                apply_col!(2);
            }
            if PREFIX_LEN >= 4 {
                apply_col!(3);
            }
            if PREFIX_LEN >= 5 {
                apply_col!(4);
            }
            if PREFIX_LEN >= 6 {
                apply_col!(5);
            }
            if PREFIX_LEN >= 7 {
                apply_col!(6);
            }
            if PREFIX_LEN >= 8 {
                apply_col!(7);
            }

            // ── Verify hits ──
            if vminvq_u8(state) != 0xFF {
                let mut state_buf = [0u8; 16];
                // SAFETY: `state_buf` is a 16-byte local array.
                unsafe { vst1q_u8(state_buf.as_mut_ptr(), state) };

                for (lane, &byte) in state_buf[..m].iter().enumerate() {
                    if ASCII_ONLY {
                        // Skip lanes starting at non-ASCII bytes.
                        // SAFETY: `start + lane < haystack.len()` guaranteed by loop condition.
                        if unsafe { *haystack.as_ptr().add(start + lane) } >= 0x80 {
                            continue;
                        }
                    }
                    let hit_mask = !byte;
                    if hit_mask != 0 && self.verify_hits(haystack, start + lane, hit_mask, on_value)
                    {
                        return true;
                    }
                }
            }

            start += m;
        }

        // Scalar tail for remaining bytes.
        if ASCII_ONLY {
            self.scan_scalar_range_ascii(haystack, start, haystack.len() - 1, on_value)
        } else {
            self.scan_scalar_range(haystack, start, haystack.len() - 1, on_value)
        }
    }

    #[target_feature(enable = "neon")]
    unsafe fn scan_neon_inner_any<const PREFIX_LEN: usize, const ASCII_ONLY: bool>(
        &self,
        haystack: &[u8],
    ) -> bool {
        const { assert!(PREFIX_LEN >= 2 && PREFIX_LEN <= MAX_SCAN_LEN) };
        let m: usize = 17 - PREFIX_LEN;

        if haystack.len() < 16 {
            return if ASCII_ONLY {
                self.scan_scalar_range_any_ascii(haystack, 0, haystack.len() - 1)
            } else {
                self.scan_scalar_range_any(haystack, 0, haystack.len() - 1)
            };
        }

        #[inline(always)]
        unsafe fn load_cols_n<const N: usize>(
            tbl: &[[u8; MASK_ROWS]; MAX_SCAN_LEN],
        ) -> [uint8x16x4_t; N] {
            std::array::from_fn(|column| {
                let ptr = tbl[column].as_ptr();
                // SAFETY: `column < N <= MAX_SCAN_LEN`, so `tbl[column]` is valid.
                // Each `[u8; 64]` entry covers offsets `0..64`, so all four loads are in-bounds.
                unsafe {
                    uint8x16x4_t(
                        vld1q_u8(ptr),
                        vld1q_u8(ptr.add(16)),
                        vld1q_u8(ptr.add(32)),
                        vld1q_u8(ptr.add(48)),
                    )
                }
            })
        }

        // SAFETY: `load_cols_n` only reads initialized `[u8; 64]` mask rows.
        let low_cols: [uint8x16x4_t; PREFIX_LEN] = unsafe { load_cols_n(&self.low_mask) };
        // SAFETY: Same as above for `self.high_mask`.
        let high_cols: [uint8x16x4_t; PREFIX_LEN] = unsafe { load_cols_n(&self.high_mask) };

        let zero = vdupq_n_u8(0);
        let mask_6b = vdupq_n_u8(0x3F);
        let ascii_hi_bit = if ASCII_ONLY { vdupq_n_u8(0x80) } else { zero };
        let mask_c0 = if !ASCII_ONLY { vdupq_n_u8(0xC0) } else { zero };
        let val_80 = if !ASCII_ONLY { vdupq_n_u8(0x80) } else { zero };
        let mut start = 0usize;

        while start + 16 <= haystack.len() {
            // SAFETY: `start + 16 <= haystack.len()` guarantees a full 16-byte load.
            let raw = unsafe { vld1q_u8(haystack.as_ptr().add(start)) };

            if ASCII_ONLY {
                let has_ascii = vminvq_u8(vandq_u8(raw, ascii_hi_bit));
                if has_ascii == 0x80 {
                    start += 16;
                    continue;
                }
            }

            if self.has_single_byte {
                for lane in 0..m {
                    // SAFETY: `lane < m == 17 - PREFIX_LEN <= 15`, and the loop condition
                    // guarantees `start + lane < start + 16 <= haystack.len()`.
                    let byte = unsafe { *haystack.as_ptr().add(start + lane) };
                    if self.single_byte_contains(byte) {
                        return true;
                    }
                }
            }

            let low_idx = vandq_u8(raw, mask_6b);
            let high_idx = vandq_u8(vshrq_n_u8(raw, 1), mask_6b);

            let lo0 = vqtbl4q_u8(low_cols[0], low_idx);
            let hi0 = vqtbl4q_u8(high_cols[0], high_idx);
            let mut state = vorrq_u8(lo0, hi0);

            if !ASCII_ONLY && !self.all_patterns_ascii {
                let cont_mask = vceqq_u8(vandq_u8(raw, mask_c0), val_80);
                state = vorrq_u8(state, cont_mask);
            }

            if vminvq_u8(state) == 0xFF {
                start += m;
                continue;
            }

            macro_rules! apply_col {
                ($shift:literal) => {{
                    let lo = vqtbl4q_u8(low_cols[$shift], low_idx);
                    let hi = vqtbl4q_u8(high_cols[$shift], high_idx);
                    state = vorrq_u8(state, vextq_u8(vorrq_u8(lo, hi), zero, $shift));
                }};
            }

            apply_col!(1);

            if PREFIX_LEN >= 3 && !self.all_patterns_ascii && vminvq_u8(state) == 0xFF {
                start += m;
                continue;
            }

            if PREFIX_LEN >= 3 {
                apply_col!(2);
            }
            if PREFIX_LEN >= 4 {
                apply_col!(3);
            }
            if PREFIX_LEN >= 5 {
                apply_col!(4);
            }
            if PREFIX_LEN >= 6 {
                apply_col!(5);
            }
            if PREFIX_LEN >= 7 {
                apply_col!(6);
            }
            if PREFIX_LEN >= 8 {
                apply_col!(7);
            }

            if vminvq_u8(state) != 0xFF {
                let mut state_buf = [0u8; 16];
                // SAFETY: `state_buf` is a 16-byte local array.
                unsafe { vst1q_u8(state_buf.as_mut_ptr(), state) };

                for (lane, &byte) in state_buf[..m].iter().enumerate() {
                    if ASCII_ONLY {
                        // SAFETY: `lane < m <= 15`, and the loop condition guarantees
                        // `start + lane < start + 16 <= haystack.len()`.
                        let start_byte = unsafe { *haystack.as_ptr().add(start + lane) };
                        if start_byte >= 0x80 {
                            continue;
                        }
                    }
                    let hit_mask = !byte;
                    if hit_mask != 0 && self.verify_hits_any(haystack, start + lane, hit_mask) {
                        return true;
                    }
                }
            }

            start += m;
        }

        if ASCII_ONLY {
            self.scan_scalar_range_any_ascii(haystack, start, haystack.len() - 1)
        } else {
            self.scan_scalar_range_any(haystack, start, haystack.len() - 1)
        }
    }

    #[target_feature(enable = "neon")]
    unsafe fn scan_neon_inner_ascii_lead_any<const PREFIX_LEN: usize>(
        &self,
        haystack: &[u8],
    ) -> bool {
        const { assert!(PREFIX_LEN >= 2 && PREFIX_LEN <= MAX_SCAN_LEN) };
        let m: usize = 17 - PREFIX_LEN;

        if haystack.len() < 16 {
            return self.scan_scalar_range_any_no_single_byte(haystack, 0, haystack.len() - 1);
        }

        #[inline(always)]
        unsafe fn load_cols_n<const N: usize>(
            tbl: &[[u8; MASK_ROWS]; MAX_SCAN_LEN],
        ) -> [uint8x16x4_t; N] {
            std::array::from_fn(|column| {
                let ptr = tbl[column].as_ptr();
                // SAFETY: `column < N <= MAX_SCAN_LEN`, so `tbl[column]` is valid.
                // Each `[u8; 64]` entry covers offsets `0..64`, so all four loads are in-bounds.
                unsafe {
                    uint8x16x4_t(
                        vld1q_u8(ptr),
                        vld1q_u8(ptr.add(16)),
                        vld1q_u8(ptr.add(32)),
                        vld1q_u8(ptr.add(48)),
                    )
                }
            })
        }

        // SAFETY: `load_cols_n` only reads initialized `[u8; 64]` mask rows.
        let low_cols: [uint8x16x4_t; PREFIX_LEN] = unsafe { load_cols_n(&self.low_mask) };
        // SAFETY: Same as above for `self.high_mask`.
        let high_cols: [uint8x16x4_t; PREFIX_LEN] = unsafe { load_cols_n(&self.high_mask) };
        let zero = vdupq_n_u8(0);
        let mask_6b = vdupq_n_u8(0x3F);
        let mut start = 0usize;

        while start + 16 <= haystack.len() {
            // SAFETY: `start + 16 <= haystack.len()` guarantees a full 16-byte load.
            let raw = unsafe { vld1q_u8(haystack.as_ptr().add(start)) };
            let low_idx = vandq_u8(raw, mask_6b);
            let high_idx = vandq_u8(vshrq_n_u8(raw, 1), mask_6b);

            let lo0 = vqtbl4q_u8(low_cols[0], low_idx);
            let hi0 = vqtbl4q_u8(high_cols[0], high_idx);
            let mut state = vorrq_u8(lo0, hi0);

            if vminvq_u8(state) == 0xFF {
                start += m;
                continue;
            }

            macro_rules! apply_col {
                ($shift:literal) => {{
                    let lo = vqtbl4q_u8(low_cols[$shift], low_idx);
                    let hi = vqtbl4q_u8(high_cols[$shift], high_idx);
                    state = vorrq_u8(state, vextq_u8(vorrq_u8(lo, hi), zero, $shift));
                }};
            }

            apply_col!(1);

            if PREFIX_LEN >= 3 {
                apply_col!(2);
            }
            if PREFIX_LEN >= 4 {
                apply_col!(3);
            }
            if PREFIX_LEN >= 5 {
                apply_col!(4);
            }
            if PREFIX_LEN >= 6 {
                apply_col!(5);
            }
            if PREFIX_LEN >= 7 {
                apply_col!(6);
            }
            if PREFIX_LEN >= 8 {
                apply_col!(7);
            }

            if vminvq_u8(state) != 0xFF {
                let mut state_buf = [0u8; 16];
                // SAFETY: `state_buf` is a 16-byte local array.
                unsafe { vst1q_u8(state_buf.as_mut_ptr(), state) };

                for (lane, &byte) in state_buf[..m].iter().enumerate() {
                    let hit_mask = !byte;
                    if hit_mask != 0 && self.verify_hits_any(haystack, start + lane, hit_mask) {
                        return true;
                    }
                }
            }

            start += m;
        }

        self.scan_scalar_range_any_no_single_byte(haystack, start, haystack.len() - 1)
    }
}
