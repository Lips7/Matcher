//! Column-vector SIMD scan backend (Harry12b encoding).
//!
//! This implementation follows the Harry paper with a dual-index encoding:
//!
//! - literals are grouped into 8 buckets,
//! - a **single unified matcher** covers all prefix lengths in the range `2..=8`;
//!   columns beyond a literal's actual length are wildcarded (bucket bit cleared for
//!   all 64 row entries), so the scan reduces to one pass over the haystack regardless
//!   of how many distinct prefix lengths exist,
//! - two mask tables per column — low index (`byte & 0x3F`, bits \[0:5\]) and high
//!   index (`(byte >> 1) & 0x3F`, bits \[1:6\]) — are ORed per lane; a hit fires
//!   only when BOTH tables have the bucket bit cleared,
//! - **column-0 early exit**: after applying the first column, the entire chunk is
//!   skipped when every lane's state byte is 0xFF (no bucket has any candidate first
//!   byte); this filters ~95% of chunks on CJK haystacks with ASCII patterns,
//! - the encoding covers 7 of 8 bits per byte; for ASCII patterns the dual-index
//!   scheme is zero-FP; for non-ASCII bytes bit 7 is lost, creating false positives
//!   between bytes X and X^0x80 — all caught by exact-match verification,
//! - bucket hits are exact-verified against the original literals across all
//!   prefix lengths registered for that bucket.

use ahash::AHashMap;
#[cfg(all(feature = "simd_runtime_dispatch", target_arch = "aarch64"))]
use std::arch::aarch64::*;
#[cfg(all(feature = "simd_runtime_dispatch", target_arch = "x86_64"))]
use std::arch::x86_64::*;

const ASCII_BYTES: usize = 128;
const N_BUCKETS: usize = 8;
const MAX_SCAN_LEN: usize = 8;
const MASK_ROWS: usize = 64;
/// Minimum number of patterns required for [`HarryMatcher::build`] to succeed.
pub const HARRY_MIN_PATTERN_COUNT: usize = 64;

#[derive(Clone)]
struct BucketLiteral {
    bytes: Box<[u8]>,
    value: u32,
}

#[derive(Clone, Default)]
struct PrefixGroup {
    exact_values: Vec<u32>,
    long_literals: Vec<BucketLiteral>,
}

/// Per-bucket verification data across all registered prefix lengths.
#[derive(Clone, Default)]
struct BucketVerify {
    /// Bitmask of which prefix lengths have entries: bit `k-2` set ↔ prefix_len `k` exists.
    length_mask: u8,
    /// Indexed by `prefix_len - 2` (index 0 = length 2, index 6 = length 8).
    groups: [AHashMap<u64, PrefixGroup>; MAX_SCAN_LEN - 1],
}

/// SIMD column-vector scan engine for literal pattern sets.
///
/// Built directly from a `(pattern, value)` slice via [`HarryMatcher::build`].
/// Returns `None` when the pattern set is too small (< `HARRY_MIN_PATTERN_COUNT`)
/// or every pattern has length < 2 (only single-byte patterns, which lack SIMD coverage).
/// Accepts both ASCII and non-ASCII (CJK) patterns and haystacks.
#[derive(Clone)]
pub struct HarryMatcher {
    single_byte_values: Box<[Vec<u32>; ASCII_BYTES]>,
    has_single_byte: bool,
    /// Low-index mask table: indexed by `byte & 0x3F` (bits \[0:5\]).
    low_mask: Box<[[u8; MASK_ROWS]; MAX_SCAN_LEN]>,
    /// High-index mask table: indexed by `(byte >> 1) & 0x3F` (bits \[1:6\]).
    /// Combined with `low_mask` it covers all 7 ASCII bits, eliminating encoding FPs.
    high_mask: Box<[[u8; MASK_ROWS]; MAX_SCAN_LEN]>,
    bucket_verify: [BucketVerify; N_BUCKETS],
}

impl HarryMatcher {
    /// Build a [`HarryMatcher`] from a slice of `(pattern, value)` pairs.
    ///
    /// Returns `None` if the set is too small, contains any empty pattern, or has no
    /// pattern with length ≥ 2. Accepts both ASCII and non-ASCII (CJK) patterns.
    pub fn build(patterns: &[(&str, u32)]) -> Option<Self> {
        if patterns.len() < HARRY_MIN_PATTERN_COUNT {
            return None;
        }
        if patterns.iter().any(|(pattern, _)| pattern.is_empty()) {
            return None;
        }
        if !patterns.iter().any(|(pattern, _)| pattern.len() >= 2) {
            return None;
        }

        let mut single_byte_values = Box::new(std::array::from_fn(|_| Vec::new()));
        let mut has_single_byte = false;
        let mut low_mask = Box::new([[0xFFu8; MASK_ROWS]; MAX_SCAN_LEN]);
        let mut high_mask = Box::new([[0xFFu8; MASK_ROWS]; MAX_SCAN_LEN]);
        let mut bucket_verify: [BucketVerify; N_BUCKETS] =
            std::array::from_fn(|_| Default::default());

        for &(pattern, value) in patterns {
            let bytes = pattern.as_bytes();
            if bytes.len() == 1 {
                single_byte_values[bytes[0] as usize].push(value);
                has_single_byte = true;
                continue;
            }

            let actual_prefix_len = bytes.len().min(MAX_SCAN_LEN);
            let bucket = (bytes[0] & 0x07) as usize;
            let bit = !(1u8 << bucket);

            for (column, &byte) in bytes[..actual_prefix_len].iter().enumerate() {
                low_mask[column][(byte & 0x3F) as usize] &= bit;
                high_mask[column][((byte >> 1) & 0x3F) as usize] &= bit;
            }

            let bv = &mut bucket_verify[bucket];
            let len_idx = actual_prefix_len - 2;
            bv.length_mask |= 1u8 << len_idx;
            let key = prefix_key(&bytes[..actual_prefix_len]);
            let group = bv.groups[len_idx].entry(key).or_default();
            if bytes.len() == actual_prefix_len {
                group.exact_values.push(value);
            } else {
                group.long_literals.push(BucketLiteral {
                    bytes: bytes.to_vec().into_boxed_slice(),
                    value,
                });
            }
        }

        // Wildcard each bucket's columns beyond its shortest pattern length.
        // This makes columns irrelevant for matching that bucket when the haystack
        // byte at that column offset is beyond the pattern — any byte passes.
        // The consequence is more false positives in verification, but zero false
        // negatives, and a single unified scan pass replaces one pass per length.
        for (bucket, bv) in bucket_verify.iter().enumerate() {
            let length_mask = bv.length_mask;
            if length_mask == 0 {
                continue;
            }
            let min_len_idx = length_mask.trailing_zeros() as usize;
            let min_prefix_len = min_len_idx + 2;
            let bit = !(1u8 << bucket);
            for column in min_prefix_len..MAX_SCAN_LEN {
                for row in 0..MASK_ROWS {
                    low_mask[column][row] &= bit;
                    high_mask[column][row] &= bit;
                }
            }
        }

        Some(Self {
            single_byte_values,
            has_single_byte,
            low_mask,
            high_mask,
            bucket_verify,
        })
    }

    /// Returns `true` if `text` contains any registered pattern.
    #[inline(always)]
    pub fn is_match(&self, text: &str) -> bool {
        self.for_each_match_value(text, |_| true)
    }

    /// Calls `on_value` for every pattern hit in `text` (overlapping).
    ///
    /// Stops early and returns `true` if `on_value` returns `true`.
    /// Returns `false` if all matches were visited without early exit.
    /// Works on both ASCII and non-ASCII (CJK) haystacks.
    #[inline(always)]
    pub fn for_each_match_value(&self, text: &str, mut on_value: impl FnMut(u32) -> bool) -> bool {
        let haystack = text.as_bytes();
        if haystack.is_empty() {
            return false;
        }

        if self.has_single_byte && self.scan_single_byte_literals(haystack, &mut on_value) {
            return true;
        }

        self.scan_multi_dispatch(haystack, &mut on_value)
    }

    #[inline(always)]
    fn scan_single_byte_literals(
        &self,
        haystack: &[u8],
        on_value: &mut impl FnMut(u32) -> bool,
    ) -> bool {
        for &byte in haystack {
            // The table has 128 entries (ASCII only); non-ASCII bytes cannot match
            // any single-byte pattern and are skipped.
            if byte < 128 {
                for &value in &self.single_byte_values[byte as usize] {
                    if on_value(value) {
                        return true;
                    }
                }
            }
        }
        false
    }

    #[inline(always)]
    fn scan_multi_dispatch(&self, haystack: &[u8], on_value: &mut impl FnMut(u32) -> bool) -> bool {
        if haystack.len() < 2 {
            return false;
        }

        #[cfg(all(feature = "simd_runtime_dispatch", target_arch = "aarch64"))]
        // SAFETY: NEON is baseline on AArch64.
        return unsafe { self.scan_neon(haystack, on_value) };

        #[cfg(all(feature = "simd_runtime_dispatch", target_arch = "x86_64"))]
        if is_x86_feature_detected!("avx512vbmi") {
            // SAFETY: AVX512-VBMI support was confirmed at runtime.
            return unsafe { self.scan_avx512vbmi(haystack, on_value) };
        }

        #[cfg(not(all(feature = "simd_runtime_dispatch", target_arch = "aarch64")))]
        return self.scan_scalar_range(haystack, 0, haystack.len() - 1, on_value);
    }

    #[inline(always)]
    fn scan_scalar_range(
        &self,
        haystack: &[u8],
        start: usize,
        end: usize,
        on_value: &mut impl FnMut(u32) -> bool,
    ) -> bool {
        for start_idx in start..end {
            let hit_mask = self.match_mask_at(haystack, start_idx);
            if hit_mask != 0 && self.verify_hits(haystack, start_idx, hit_mask, on_value) {
                return true;
            }
        }

        false
    }

    #[cfg(all(feature = "simd_runtime_dispatch", target_arch = "aarch64"))]
    #[target_feature(enable = "neon")]
    unsafe fn scan_neon(&self, haystack: &[u8], on_value: &mut impl FnMut(u32) -> bool) -> bool {
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

    #[cfg(all(feature = "simd_runtime_dispatch", target_arch = "x86_64"))]
    #[target_feature(enable = "avx512f,avx512bw,avx512vbmi")]
    unsafe fn scan_avx512vbmi(
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

            // Early exit: if no valid lane has any bucket bit cleared after column 0,
            // no bucket can match any start position in this chunk.
            if unsafe { _mm512_cmpneq_epi8_mask(state, all_ff) as u64 } & valid_lane_mask == 0 {
                start += M;
                continue;
            }

            unsafe {
                for column in 1..MAX_SCAN_LEN {
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

    #[inline(always)]
    fn match_mask_at(&self, haystack: &[u8], start: usize) -> u8 {
        // Clip to available bytes so we don't read past the end.  Wildcarded columns
        // (bit already cleared for all rows) would contribute zero to state anyway,
        // so omitting them is equivalent.  Any resulting false positives for patterns
        // longer than `available` are filtered in verify_bucket.
        let available = (haystack.len() - start).min(MAX_SCAN_LEN);
        let mut state = 0u8;

        for column in 0..available {
            let byte = haystack[start + column];
            state |= self.low_mask[column][(byte & 0x3F) as usize]
                | self.high_mask[column][((byte >> 1) & 0x3F) as usize];
        }

        !state
    }

    #[inline(always)]
    fn verify_hits(
        &self,
        haystack: &[u8],
        start: usize,
        mut hit_mask: u8,
        on_value: &mut impl FnMut(u32) -> bool,
    ) -> bool {
        while hit_mask != 0 {
            let bucket = hit_mask.trailing_zeros() as usize;
            hit_mask &= hit_mask - 1;

            if self.verify_bucket(haystack, start, bucket, on_value) {
                return true;
            }
        }

        false
    }

    #[inline(always)]
    fn verify_bucket(
        &self,
        haystack: &[u8],
        start: usize,
        bucket: usize,
        on_value: &mut impl FnMut(u32) -> bool,
    ) -> bool {
        let bv = &self.bucket_verify[bucket];
        let mut lengths = bv.length_mask;

        while lengths != 0 {
            let len_idx = lengths.trailing_zeros() as usize;
            lengths &= lengths - 1;
            let prefix_len = len_idx + 2;

            if start + prefix_len > haystack.len() {
                continue;
            }

            let key = prefix_key(&haystack[start..start + prefix_len]);
            let Some(group) = bv.groups[len_idx].get(&key) else {
                continue;
            };

            for &value in &group.exact_values {
                if on_value(value) {
                    return true;
                }
            }

            for literal in &group.long_literals {
                let len = literal.bytes.len();
                if start + len > haystack.len() {
                    continue;
                }

                if haystack[start + prefix_len..start + len] == literal.bytes[prefix_len..]
                    && on_value(literal.value)
                {
                    return true;
                }
            }
        }

        false
    }
}

#[inline(always)]
fn prefix_key(bytes: &[u8]) -> u64 {
    let mut key = 0u64;
    for (shift, &byte) in bytes.iter().enumerate() {
        key |= u64::from(byte) << (shift * 8);
    }
    key
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_patterns(words: &[&str]) -> Vec<(String, u32)> {
        words
            .iter()
            .enumerate()
            .map(|(i, &word)| (word.to_owned(), i as u32))
            .collect()
    }

    fn refs(patterns: &[(String, u32)]) -> Vec<(&str, u32)> {
        patterns
            .iter()
            .map(|(pattern, value)| (pattern.as_str(), *value))
            .collect()
    }

    fn big_set() -> Vec<(String, u32)> {
        (0u32..64).map(|i| (format!("token{i:02}"), i)).collect()
    }

    fn collect_unique_hits(matcher: &HarryMatcher, haystack: &str) -> Vec<u32> {
        let mut hits = Vec::new();
        matcher.for_each_match_value(haystack, |value| {
            hits.push(value);
            false
        });
        hits.sort_unstable();
        hits.dedup();
        hits
    }

    fn collect_naive_hits(patterns: &[(String, u32)], haystack: &str) -> Vec<u32> {
        let mut hits: Vec<u32> = patterns
            .iter()
            .filter(|(pattern, _)| haystack.contains(pattern.as_str()))
            .map(|(_, value)| *value)
            .collect();
        hits.sort_unstable();
        hits.dedup();
        hits
    }

    #[test]
    fn build_rejects_small_sets() {
        let patterns = make_patterns(&["hello", "world"]);
        assert!(HarryMatcher::build(&refs(&patterns)).is_none());
    }

    #[test]
    fn build_rejects_all_single_byte_sets() {
        let patterns: Vec<(String, u32)> = (0u8..64)
            .map(|i| ((char::from(b'!' + i)).to_string(), i as u32))
            .collect();
        assert!(HarryMatcher::build(&refs(&patterns)).is_none());
    }

    #[test]
    fn build_accepts_large_ascii_set() {
        let patterns = big_set();
        assert!(HarryMatcher::build(&refs(&patterns)).is_some());
    }

    #[test]
    fn build_accepts_large_cjk_set() {
        let patterns: Vec<(String, u32)> =
            (0u32..64).map(|i| (format!("测试词{i:02}"), i)).collect();
        assert!(HarryMatcher::build(&refs(&patterns)).is_some());
    }

    #[test]
    fn build_accepts_mixed_ascii_cjk_set() {
        let mut patterns = big_set(); // 64 ASCII patterns
        patterns.extend((0u32..32).map(|i| (format!("词语{i:02}"), i + 100)));
        assert!(HarryMatcher::build(&refs(&patterns)).is_some());
    }

    #[test]
    fn build_accepts_mixed_single_and_multi_byte_set() {
        let mut patterns: Vec<(String, u32)> = (0u8..40)
            .map(|i| ((char::from(b'!' + i)).to_string(), i as u32))
            .collect();
        patterns.extend((0u32..32).map(|i| (format!("word{i:02}"), i + 100)));
        assert!(HarryMatcher::build(&refs(&patterns)).is_some());
    }

    #[test]
    fn is_match_basic() {
        let patterns = big_set();
        let matcher = HarryMatcher::build(&refs(&patterns)).unwrap();
        assert!(matcher.is_match("prefix token42 suffix"));
        assert!(!matcher.is_match("nothing here at all!!"));
    }

    #[test]
    fn for_each_match_value_collects_all_hits() {
        let patterns = big_set();
        let matcher = HarryMatcher::build(&refs(&patterns)).unwrap();
        let mut hits = Vec::new();

        let stopped = matcher.for_each_match_value("token01 token42 token63", |value| {
            hits.push(value);
            false
        });

        assert!(!stopped);
        hits.sort_unstable();
        assert_eq!(hits, vec![1, 42, 63]);
    }

    #[test]
    fn early_exit_returns_true() {
        let patterns = big_set();
        let matcher = HarryMatcher::build(&refs(&patterns)).unwrap();
        let mut count = 0usize;

        let stopped = matcher.for_each_match_value("token00 token01 token02", |_| {
            count += 1;
            count >= 1
        });

        assert!(stopped);
        assert_eq!(count, 1);
    }

    #[test]
    fn matches_long_pattern_via_prefix_filter() {
        let mut patterns = big_set();
        patterns.push(("averyverylongliteral".to_owned(), 999));
        let matcher = HarryMatcher::build(&refs(&patterns)).unwrap();

        let hits = collect_unique_hits(&matcher, "xx averyverylongliteral yy");
        assert!(hits.contains(&999));
    }

    #[test]
    fn single_byte_literals_still_match() {
        let mut patterns = big_set();
        patterns.push(("x".to_owned(), 999));
        patterns.push(("z".to_owned(), 1000));
        let matcher = HarryMatcher::build(&refs(&patterns)).unwrap();

        let hits = collect_unique_hits(&matcher, "x token00 yz");
        assert!(hits.contains(&999));
        assert!(hits.contains(&1000));
        assert!(hits.contains(&0));
    }

    #[test]
    fn encoding_collision_is_filtered_by_exact_match() {
        let mut patterns = big_set();
        patterns.push(("pq".to_owned(), 999));
        let matcher = HarryMatcher::build(&refs(&patterns)).unwrap();

        let hits = collect_unique_hits(&matcher, "0q");
        assert!(!hits.contains(&999));
    }

    #[test]
    fn grouped_bucket_false_positive_is_filtered_by_exact_match() {
        let mut patterns = big_set();
        patterns.push(("ab".to_owned(), 999));
        patterns.push(("ij".to_owned(), 1000));
        let matcher = HarryMatcher::build(&refs(&patterns)).unwrap();

        let hits = collect_unique_hits(&matcher, "aj");
        assert!(!hits.contains(&999));
        assert!(!hits.contains(&1000));
    }

    #[test]
    fn handles_simd_chunk_boundaries() {
        let mut patterns = big_set();
        patterns.push(("boundaryxx".to_owned(), 999));
        let matcher = HarryMatcher::build(&refs(&patterns)).unwrap();

        let haystack = format!("{}boundaryxx{}", "a".repeat(17), "b".repeat(23));
        let hits = collect_unique_hits(&matcher, haystack.as_str());
        assert!(hits.contains(&999));
    }

    #[test]
    fn no_false_negatives_vs_naive_for_mixed_lengths() {
        let mut patterns: Vec<(String, u32)> =
            (0u32..64).map(|i| (format!("pat{i:03}"), i)).collect();
        patterns.push(("x".to_owned(), 900));
        patterns.push(("averyverylongliteral".to_owned(), 901));
        patterns.push(("pq".to_owned(), 902));
        patterns.push(("ij".to_owned(), 903));
        let matcher = HarryMatcher::build(&refs(&patterns)).unwrap();

        let haystack = "xxpat000yyxzzpat031aa averyverylongliteral 0q aj pat063 end";

        let harry = collect_unique_hits(&matcher, haystack);
        let naive = collect_naive_hits(&patterns, haystack);

        assert_eq!(harry, naive);
    }

    #[test]
    fn randomized_parity_against_naive() {
        fn next_u32(state: &mut u64) -> u32 {
            *state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
            (*state >> 32) as u32
        }

        let alphabet = *b"0pqrijabXYZtokenLM";
        let mut seen = std::collections::HashSet::new();
        let mut patterns = Vec::new();
        let mut seed = 1u64;
        let mut next_value = 0u32;

        while patterns.len() < 96 {
            let len_roll = (next_u32(&mut seed) % 10) as usize;
            let len = match len_roll {
                0 => 1,
                1..=7 => len_roll + 1,
                _ => 8 + (next_u32(&mut seed) % 5) as usize,
            };
            let mut bytes = Vec::with_capacity(len);
            for _ in 0..len {
                let idx = (next_u32(&mut seed) as usize) % alphabet.len();
                bytes.push(alphabet[idx]);
            }
            let pattern = String::from_utf8(bytes).unwrap();
            if seen.insert(pattern.clone()) {
                patterns.push((pattern, next_value));
                next_value += 1;
            }
        }

        let matcher = HarryMatcher::build(&refs(&patterns)).unwrap();

        let mut haystack = String::with_capacity(1024);
        for _ in 0..1024 {
            let idx = (next_u32(&mut seed) as usize) % alphabet.len();
            haystack.push(alphabet[idx] as char);
        }
        haystack.push_str("averyverylongliteral");
        haystack.push_str("0q");
        haystack.push_str("aj");

        let harry = collect_unique_hits(&matcher, haystack.as_str());
        let naive = collect_naive_hits(&patterns, haystack.as_str());
        assert_eq!(harry, naive, "Harry missed a match vs naive scan");
    }

    #[test]
    fn ascii_patterns_do_not_match_cjk_haystack() {
        // ASCII patterns have no bytes in common with CJK UTF-8 sequences,
        // so is_match must return false even without a haystack guard.
        let patterns = big_set();
        let matcher = HarryMatcher::build(&refs(&patterns)).unwrap();
        assert!(!matcher.is_match("日本語テキスト"));
    }

    #[test]
    fn cjk_patterns_match_cjk_haystack() {
        let mut patterns = big_set(); // filler to reach ≥64
        patterns.push(("你好世界".to_owned(), 900));
        let matcher = HarryMatcher::build(&refs(&patterns)).unwrap();
        assert!(matcher.is_match("这是一段测试文本你好世界结尾"));
        assert!(!matcher.is_match("this is ascii only text"));
    }

    #[test]
    fn cjk_patterns_no_false_negatives_vs_naive() {
        let mut patterns: Vec<(String, u32)> =
            (0u32..64).map(|i| (format!("模式{i:02}"), i)).collect();
        patterns.push(("关键词".to_owned(), 900));
        let matcher = HarryMatcher::build(&refs(&patterns)).unwrap();

        let haystack = "这段文本包含关键词还有模式00以及模式31等等";
        let harry = collect_unique_hits(&matcher, haystack);
        let naive = collect_naive_hits(&patterns, haystack);
        assert_eq!(harry, naive, "Harry CJK missed a match vs naive scan");
    }

    /// Harry fires on_value once per matching *position*, not once per unique pattern.
    /// For overlapping occurrences (e.g. "aa" in "aaa"), the callback is called
    /// once per start position that produces a hit.
    #[test]
    fn overlapping_matches_reported_per_position() {
        // Build a set large enough for HarryMatcher::build to succeed.
        // We include the two patterns we actually care about in the overlap test.
        let mut patterns = big_set(); // 64 filler patterns
        patterns.push(("aa".to_owned(), 900)); // 2-char overlap candidate
        patterns.push(("aab".to_owned(), 901)); // longer pattern starting same way
        let refs: Vec<(&str, u32)> = patterns.iter().map(|(p, v)| (p.as_str(), *v)).collect();
        let matcher = HarryMatcher::build(&refs).unwrap();

        // "aaa" contains "aa" at position 0 and position 1 — both overlapping.
        let mut calls_900 = 0usize;
        let mut calls_901 = 0usize;
        matcher.for_each_match_value("aaa", |v| {
            if v == 900 {
                calls_900 += 1;
            }
            if v == 901 {
                calls_901 += 1;
            }
            false
        });
        assert_eq!(
            calls_900, 2,
            "\"aa\" should match at both position 0 and 1 in \"aaa\""
        );
        assert_eq!(calls_901, 0, "\"aab\" should not match in \"aaa\"");

        // "aab" contains "aa" at position 0 and "aab" at position 0.
        let mut calls_900 = 0usize;
        let mut calls_901 = 0usize;
        matcher.for_each_match_value("aab", |v| {
            if v == 900 {
                calls_900 += 1;
            }
            if v == 901 {
                calls_901 += 1;
            }
            false
        });
        assert_eq!(
            calls_900, 1,
            "\"aa\" should match once in \"aab\" (position 0)"
        );
        assert_eq!(
            calls_901, 1,
            "\"aab\" should match once in \"aab\" (position 0)"
        );

        // "aabaab" — "aa" appears at positions 0 and 3; "aab" at positions 0 and 3.
        let mut calls_900 = 0usize;
        let mut calls_901 = 0usize;
        matcher.for_each_match_value("aabaab", |v| {
            if v == 900 {
                calls_900 += 1;
            }
            if v == 901 {
                calls_901 += 1;
            }
            false
        });
        assert_eq!(
            calls_900, 2,
            "\"aa\" should match at positions 0 and 3 in \"aabaab\""
        );
        assert_eq!(
            calls_901, 2,
            "\"aab\" should match at positions 0 and 3 in \"aabaab\""
        );
    }
}
