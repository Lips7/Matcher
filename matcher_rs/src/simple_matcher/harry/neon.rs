use std::arch::aarch64::*;

use super::{HarryMatcher, MASK_ROWS, MAX_SCAN_LEN};

impl HarryMatcher {
    /// NEON column-vector scan kernel (general path).
    ///
    /// Processes M=9 haystack positions per iteration using `vqtbl4q_u8` table
    /// lookups across all 8 mask columns.
    ///
    /// # Safety
    ///
    /// Requires AArch64 NEON (baseline on all AArch64 targets). Pointer arithmetic
    /// is bounded by the loop condition `start + M + MAX_SCAN_LEN - 1 <= haystack.len()`.
    #[target_feature(enable = "neon")]
    pub(super) unsafe fn scan_neon(
        &self,
        haystack: &[u8],
        on_value: &mut impl FnMut(u32) -> bool,
    ) -> bool {
        const M: usize = 9;

        if haystack.len() < M + MAX_SCAN_LEN - 1 {
            return self.scan_scalar_range(haystack, 0, haystack.len() - 1, on_value);
        }

        // Pre-load all column mask tables into NEON register groups (stack-allocated,
        // no heap allocation).  Each column occupies 4 consecutive uint8x16 registers
        // (vqtbl4q_u8 requires a uint8x16x4_t source).
        let load_cols = |tbl: &[[u8; MASK_ROWS]; MAX_SCAN_LEN]| {
            std::array::from_fn(|column| {
                let ptr = tbl[column].as_ptr();
                // Safety: `tbl[column]` is `[u8; 64]`; the four loads cover
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
        let mut start = 0usize;

        // The loop condition guarantees start + M + MAX_SCAN_LEN - 1 <= haystack.len(),
        // i.e., start + 16 <= haystack.len() (M=9, MAX_SCAN_LEN=8 → 9+8-1=16).
        while start + M + MAX_SCAN_LEN - 1 <= haystack.len() {
            // Safety: loop condition guarantees haystack[start..start+16] is valid.
            let (low_idx, high_idx) = unsafe {
                let raw = vld1q_u8(haystack.as_ptr().add(start));
                (
                    vandq_u8(raw, mask_6b),
                    vandq_u8(vshrq_n_u8(raw, 1), mask_6b),
                )
            };

            // Column 0: no shift — each lane's lookup corresponds directly to the
            // byte at haystack[start + lane].
            let lo0 = vqtbl4q_u8(low_cols[0], low_idx);
            let hi0 = vqtbl4q_u8(high_cols[0], high_idx);
            let mut state = vorrq_u8(lo0, hi0);

            // Early exit: if every lane's state byte is already 0xFF after column 0,
            // no bucket has a candidate first-byte match in this entire 9-lane chunk.
            // Skip the remaining 7 columns.  On CJK haystacks with ASCII patterns
            // this branch fires for ~95% of chunks, cutting work ~8x.
            if vminvq_u8(state) == 0xFF {
                start += M;
                continue;
            }

            macro_rules! apply_col {
                ($shift:literal) => {{
                    let lo = vqtbl4q_u8(low_cols[$shift], low_idx);
                    let hi = vqtbl4q_u8(high_cols[$shift], high_idx);
                    state = vorrq_u8(
                        state,
                        vorrq_u8(vextq_u8(lo, zero, $shift), vextq_u8(hi, zero, $shift)),
                    );
                }};
            }

            apply_col!(1);
            apply_col!(2);
            apply_col!(3);
            apply_col!(4);
            apply_col!(5);
            apply_col!(6);
            apply_col!(7);

            // Horizontal min: if every lane is 0xFF, no bucket bit was cleared
            // in any lane — skip the store entirely.
            if vminvq_u8(state) != 0xFF {
                let mut state_buf = [0u8; 16];
                // Safety: `state_buf` is a 16-byte local array; `vst1q_u8` writes
                // exactly 16 bytes starting at `as_mut_ptr()`.
                unsafe { vst1q_u8(state_buf.as_mut_ptr(), state) };

                for (lane, &byte) in state_buf[..M].iter().enumerate() {
                    let hit_mask = !byte;
                    if hit_mask != 0 && self.verify_hits(haystack, start + lane, hit_mask, on_value)
                    {
                        return true;
                    }
                }
            }

            start += M;
        }

        self.scan_scalar_range(haystack, start, haystack.len() - 1, on_value)
    }

    /// NEON kernel for ASCII-only pattern sets.
    ///
    /// When every pattern byte is ASCII, a match can only begin at an ASCII
    /// haystack byte. This kernel exploits that by loading 16 bytes and checking
    /// bit 7 of each — if ALL bytes are non-ASCII (≥ 0x80), the entire 16-byte
    /// window is skipped without any column work. On CJK haystacks this
    /// eliminates virtually 100% of column scans.
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
        const M: usize = 9;

        if haystack.len() < M + MAX_SCAN_LEN - 1 {
            return self.scan_scalar_range_ascii(haystack, 0, haystack.len() - 1, on_value);
        }

        let load_cols = |tbl: &[[u8; MASK_ROWS]; MAX_SCAN_LEN]| {
            std::array::from_fn(|column| {
                let ptr = tbl[column].as_ptr();
                // Safety: `tbl[column]` is `[u8; 64]`; the four loads cover
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
        let ascii_hi_bit = vdupq_n_u8(0x80);
        let mut start = 0usize;

        while start + M + MAX_SCAN_LEN - 1 <= haystack.len() {
            // Safety: loop condition guarantees haystack[start..start+16] is valid.
            let raw = unsafe { vld1q_u8(haystack.as_ptr().add(start)) };

            // ASCII fast path: if ALL 16 bytes have bit 7 set (non-ASCII), no
            // ASCII pattern can match at any position in this window. Skip the
            // full 16-byte load width (wider than M=9) since even positions
            // start..start+15 cannot begin an ASCII-only match.
            let has_ascii = vminvq_u8(vandq_u8(raw, ascii_hi_bit));
            if has_ascii == 0x80 {
                // All bytes are non-ASCII. Safe to skip 16 because:
                // - positions start..start+15 all start with a non-ASCII byte
                // - we'll re-check from start+16 onward
                start += 16;
                continue;
            }

            // Some ASCII bytes present — fall through to the normal column scan.
            let low_idx = vandq_u8(raw, mask_6b);
            let high_idx = vandq_u8(vshrq_n_u8(raw, 1), mask_6b);

            let lo0 = vqtbl4q_u8(low_cols[0], low_idx);
            let hi0 = vqtbl4q_u8(high_cols[0], high_idx);
            let mut state = vorrq_u8(lo0, hi0);

            if vminvq_u8(state) == 0xFF {
                start += M;
                continue;
            }

            macro_rules! apply_col {
                ($shift:literal) => {{
                    let lo = vqtbl4q_u8(low_cols[$shift], low_idx);
                    let hi = vqtbl4q_u8(high_cols[$shift], high_idx);
                    state = vorrq_u8(
                        state,
                        vorrq_u8(vextq_u8(lo, zero, $shift), vextq_u8(hi, zero, $shift)),
                    );
                }};
            }

            apply_col!(1);
            apply_col!(2);
            apply_col!(3);
            apply_col!(4);
            apply_col!(5);
            apply_col!(6);
            apply_col!(7);

            if vminvq_u8(state) != 0xFF {
                let mut state_buf = [0u8; 16];
                // Safety: `state_buf` is a 16-byte local array.
                unsafe { vst1q_u8(state_buf.as_mut_ptr(), state) };

                for (lane, &byte) in state_buf[..M].iter().enumerate() {
                    // Safety: `start + lane < haystack.len()` guaranteed by loop condition.
                    if unsafe { *haystack.as_ptr().add(start + lane) } >= 0x80 {
                        continue;
                    }
                    let hit_mask = !byte;
                    if hit_mask != 0 && self.verify_hits(haystack, start + lane, hit_mask, on_value)
                    {
                        return true;
                    }
                }
            }

            start += M;
        }

        self.scan_scalar_range_ascii(haystack, start, haystack.len() - 1, on_value)
    }
}
