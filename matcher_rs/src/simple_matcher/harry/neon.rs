use std::arch::aarch64::*;

use super::{HarryMatcher, MASK_ROWS, MAX_SCAN_LEN};

impl HarryMatcher {
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

        let load_cols = |tbl: &[[u8; MASK_ROWS]; MAX_SCAN_LEN]| {
            std::array::from_fn(|column| {
                let ptr = tbl[column].as_ptr();
                // SAFETY: `tbl[column]` is `[u8; 64]`; the four loads cover
                // offsets 0..16, 16..32, 32..48, 48..64 — all within the array.
                unsafe {
                    uint8x16x4_t(
                        vld1q_u8(ptr),
                        vld1q_u8(ptr.add(16)),
                        vld1q_u8(ptr.add(32)),
                        vld1q_u8(ptr.add(48)),
                    )
                }
            })
        };
        let low_cols: [uint8x16x4_t; MAX_SCAN_LEN] = load_cols(&self.low_mask);
        let high_cols: [uint8x16x4_t; MAX_SCAN_LEN] = load_cols(&self.high_mask);

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
            // Mark lanes starting at continuation bytes (0x80-0xBF) as "no match"
            // by ORing 0xFF into their state — a valid match can never start at a
            // continuation byte regardless of pattern content.
            //
            // Skipped in ASCII_ONLY mode (handled by the 16-byte non-ASCII skip)
            // and when all patterns are ASCII (column scan self-filters since
            // non-ASCII bytes never have bucket bits set in the mask tables).
            if !ASCII_ONLY && !self.all_patterns_ascii {
                let cont_mask = vceqq_u8(vandq_u8(raw, mask_c0), val_80);
                state = vorrq_u8(state, cont_mask);
            }

            // ── Column-0 early exit ──
            // If every lane's state is 0xFF, no bucket has a candidate first-byte
            // match in this chunk. Skip remaining columns entirely.
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
}
