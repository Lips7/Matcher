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
//!
//! # Module layout
//!
//! - Core types, constants, public API, dispatch, scalar kernels, verification (this file).
//! - [`build`] — [`HarryMatcher::build`] constructor.
//! - `neon` — NEON SIMD kernels (AArch64, feature-gated).
//! - `avx512` — AVX512-VBMI SIMD kernels (x86-64, feature-gated).

#[cfg(all(feature = "simd_runtime_dispatch", target_arch = "x86_64"))]
mod avx512;
mod build;
#[cfg(all(feature = "simd_runtime_dispatch", target_arch = "aarch64"))]
mod neon;
#[cfg(test)]
mod tests;

const ASCII_BYTES: usize = 128;
const N_BUCKETS: usize = 8;
const MAX_SCAN_LEN: usize = 8;
const MASK_ROWS: usize = 64;
/// Minimum number of patterns required for [`HarryMatcher::build`] to succeed.
pub const HARRY_MIN_PATTERN_COUNT: usize = 64;

/// A literal pattern stored for exact-match verification.
#[derive(Clone)]
struct BucketLiteral {
    /// Full pattern bytes (not just the prefix).
    bytes: Box<[u8]>,
    /// Caller-assigned value returned on match.
    value: u32,
}

/// Verification group for a single prefix length within a bucket.
///
/// Exact-length patterns go into `exact_values`; longer patterns need suffix
/// verification via `long_literals`.
#[derive(Clone, Default)]
struct PrefixGroup {
    exact_values: Vec<u32>,
    long_literals: Vec<BucketLiteral>,
}

/// Sorted prefix-key → `PrefixGroup` map for one prefix length within a bucket.
///
/// Keys and values are stored in parallel arrays: binary search runs over the
/// compact `keys` slice (contiguous `u64`s — 16 KB for 2000 entries, fits L1 cache),
/// then indexes into `values` only on a hit. This avoids both hash computation
/// (the `u64` key IS the raw prefix bytes) and the cache pollution of interleaving
/// large `PrefixGroup` structs with small `u64` keys.
#[derive(Clone, Default)]
struct PrefixMap {
    keys: Box<[u64]>,
    values: Box<[PrefixGroup]>,
}

impl PrefixMap {
    /// Builds from an unsorted iterator of `(key, group)` pairs.
    fn from_unsorted(iter: impl Iterator<Item = (u64, PrefixGroup)>) -> Self {
        let mut pairs: Vec<(u64, PrefixGroup)> = iter.collect();
        if pairs.is_empty() {
            return Self::default();
        }
        pairs.sort_unstable_by_key(|(k, _)| *k);
        let (keys, values): (Vec<_>, Vec<_>) = pairs.into_iter().unzip();
        Self {
            keys: keys.into_boxed_slice(),
            values: values.into_boxed_slice(),
        }
    }

    /// Looks up a prefix group by key via binary search on the keys array.
    #[inline(always)]
    fn get(&self, key: u64) -> Option<&PrefixGroup> {
        self.keys
            .binary_search(&key)
            .ok()
            .map(|idx| &self.values[idx])
    }
}

/// Per-bucket verification data across all registered prefix lengths.
#[derive(Clone, Default)]
struct BucketVerify {
    /// Bitmask of which prefix lengths have entries: bit `k-2` set ↔ prefix_len `k` exists.
    length_mask: u8,
    /// Indexed by `prefix_len - 2` (index 0 = length 2, index 6 = length 8).
    groups: [PrefixMap; MAX_SCAN_LEN - 1],
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
    single_byte_keys: Box<[u8]>,
    single_byte_match_mask: [u64; 2],
    has_single_byte: bool,
    /// Low-index mask table: indexed by `byte & 0x3F` (bits \[0:5\]).
    low_mask: Box<[[u8; MASK_ROWS]; MAX_SCAN_LEN]>,
    /// High-index mask table: indexed by `(byte >> 1) & 0x3F` (bits \[1:6\]).
    /// Combined with `low_mask` it covers all 7 ASCII bits, eliminating encoding FPs.
    high_mask: Box<[[u8; MASK_ROWS]; MAX_SCAN_LEN]>,
    bucket_verify: [BucketVerify; N_BUCKETS],
    /// True when every pattern byte is ASCII (< 0x80). Enables a fast path that skips
    /// non-ASCII haystack regions entirely — matches can only start at ASCII bytes.
    all_patterns_ascii: bool,
    /// Maximum prefix length across all multi-byte patterns (2..=MAX_SCAN_LEN).
    /// Columns beyond this are wildcarded and don't contribute useful filtering,
    /// so the SIMD kernels skip them. Fewer columns also allows a larger M (lanes
    /// per chunk) on fixed-width SIMD: M = 16 - max_prefix_len + 1 on NEON.
    max_prefix_len: usize,
}

impl HarryMatcher {
    /// Returns `true` if `text` contains any registered pattern.
    #[inline(always)]
    pub fn is_match(&self, text: &str) -> bool {
        let haystack = text.as_bytes();
        if haystack.is_empty() {
            return false;
        }

        self.is_match_bytes(haystack)
    }

    /// Calls `on_value` for every match (one call per matching position × pattern).
    ///
    /// Stops early and returns `true` if `on_value` returns `true`.
    /// Overlapping matches are reported — e.g. "aa" in "aaa" fires at positions 0 and 1.
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
    fn is_match_bytes(&self, haystack: &[u8]) -> bool {
        #[cfg(any(
            all(feature = "simd_runtime_dispatch", target_arch = "aarch64"),
            all(feature = "simd_runtime_dispatch", target_arch = "x86_64")
        ))]
        {
            if self.has_single_byte && haystack.len() == 1 {
                return self.single_byte_contains(haystack[0]);
            }

            if self.all_patterns_ascii {
                if haystack[0] < 0x80 {
                    if self.has_single_byte && self.scan_single_byte_any_ascii_haystack(haystack) {
                        return true;
                    }
                    return self.scan_multi_dispatch_any_ascii_lead(haystack);
                }

                return self.scan_multi_dispatch_any(haystack);
            }

            if haystack[0] >= 0x80 && self.has_single_byte {
                return self.scan_multi_dispatch_any(haystack);
            }
        }

        // SAFETY: Harry only scans bytes originating from valid `&str` inputs produced by the
        // matcher pipeline, so this fallback haystack slice is still valid UTF-8.
        self.for_each_match_value(unsafe { std::str::from_utf8_unchecked(haystack) }, |_| true)
    }

    /// Checks all single-byte patterns against the haystack.
    #[inline(always)]
    fn scan_single_byte_literals(
        &self,
        haystack: &[u8],
        on_value: &mut impl FnMut(u32) -> bool,
    ) -> bool {
        // Use SIMD-accelerated skip only on non-ASCII-dominated text where
        // bulk skipping is effective. On ASCII text the byte-at-a-time loop
        // below is faster (no SIMD overhead, good branch prediction).
        if self.all_patterns_ascii && haystack[0] >= 0x80 {
            return self.scan_single_byte_literals_ascii(haystack, on_value);
        }
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
    fn single_byte_contains(&self, byte: u8) -> bool {
        if byte >= ASCII_BYTES as u8 {
            return false;
        }
        let word = (byte >> 6) as usize;
        let bit = byte & 0x3F;
        (self.single_byte_match_mask[word] >> bit) & 1 != 0
    }

    #[inline(always)]
    fn scan_single_byte_any_ascii_haystack(&self, haystack: &[u8]) -> bool {
        debug_assert!(self.all_patterns_ascii);
        if self.single_byte_keys.is_empty() {
            return false;
        }

        #[cfg(all(feature = "simd_runtime_dispatch", target_arch = "aarch64"))]
        if self.single_byte_keys.len() <= 4 && haystack.len() >= 16 {
            // SAFETY: NEON is baseline on AArch64, and the helper only reads within `haystack`.
            return unsafe { self.scan_single_byte_any_ascii_haystack_neon(haystack) };
        }

        #[cfg(all(feature = "simd_runtime_dispatch", target_arch = "x86_64"))]
        if self.single_byte_keys.len() <= 4
            && haystack.len() >= 64
            && is_x86_feature_detected!("avx512vbmi")
        {
            // SAFETY: AVX512-VBMI support was confirmed at runtime, and the helper is bounds-safe.
            return unsafe { self.scan_single_byte_any_ascii_haystack_avx512(haystack) };
        }

        haystack
            .iter()
            .copied()
            .any(|byte| self.single_byte_contains(byte))
    }

    /// Single-byte literal scan with SIMD-accelerated non-ASCII skip.
    ///
    /// When all patterns are ASCII, the single_byte_values table only has ASCII
    /// entries. Non-ASCII haystack bytes are guaranteed non-matching, so we can
    /// use NEON/SIMD to skip 16-byte runs of non-ASCII bytes at once.
    #[inline(always)]
    fn scan_single_byte_literals_ascii(
        &self,
        haystack: &[u8],
        on_value: &mut impl FnMut(u32) -> bool,
    ) -> bool {
        #[cfg(all(feature = "simd_runtime_dispatch", target_arch = "aarch64"))]
        {
            // SAFETY: NEON is baseline on AArch64, and the helper only reads within `haystack`.
            unsafe { self.scan_single_byte_literals_ascii_neon(haystack, on_value) }
        }

        #[cfg(all(feature = "simd_runtime_dispatch", target_arch = "x86_64"))]
        {
            if is_x86_feature_detected!("avx512vbmi") {
                // SAFETY: AVX512-VBMI support was confirmed at runtime, and the helper is bounds-safe.
                return unsafe { self.scan_single_byte_literals_ascii_avx512(haystack, on_value) };
            }
            self.scan_single_byte_literals_ascii_scalar(haystack, on_value)
        }

        #[cfg(not(any(
            all(feature = "simd_runtime_dispatch", target_arch = "aarch64"),
            all(feature = "simd_runtime_dispatch", target_arch = "x86_64")
        )))]
        self.scan_single_byte_literals_ascii_scalar(haystack, on_value)
    }

    #[cfg(not(all(feature = "simd_runtime_dispatch", target_arch = "aarch64")))]
    #[inline(always)]
    fn scan_single_byte_literals_ascii_scalar(
        &self,
        haystack: &[u8],
        on_value: &mut impl FnMut(u32) -> bool,
    ) -> bool {
        for &byte in haystack {
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

    /// Routes multi-byte scanning to the best available kernel.
    #[inline(always)]
    fn scan_multi_dispatch(&self, haystack: &[u8], on_value: &mut impl FnMut(u32) -> bool) -> bool {
        if haystack.len() < 2 {
            return false;
        }

        // Use the ASCII-skip kernel only when patterns are ASCII AND the text
        // starts with a non-ASCII byte (likely CJK/non-Latin text). On
        // ASCII-dominated text the skip check adds overhead with no benefit.
        if self.all_patterns_ascii && haystack[0] >= 0x80 {
            #[cfg(all(feature = "simd_runtime_dispatch", target_arch = "aarch64"))]
            // SAFETY: NEON is baseline on AArch64.
            return unsafe { self.scan_neon_ascii(haystack, on_value) };

            #[cfg(all(feature = "simd_runtime_dispatch", target_arch = "x86_64"))]
            if is_x86_feature_detected!("avx512vbmi") {
                // SAFETY: AVX512-VBMI support was confirmed at runtime.
                return unsafe { self.scan_avx512vbmi_ascii(haystack, on_value) };
            }

            #[cfg(not(all(feature = "simd_runtime_dispatch", target_arch = "aarch64")))]
            return self.scan_scalar_range_ascii(haystack, 0, haystack.len() - 1, on_value);
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
    fn scan_multi_dispatch_any(&self, haystack: &[u8]) -> bool {
        if self.has_single_byte && haystack.len() == 1 {
            return self.single_byte_contains(haystack[0]);
        }

        if haystack.len() < 2 {
            return false;
        }

        if self.all_patterns_ascii && haystack[0] >= 0x80 {
            #[cfg(all(feature = "simd_runtime_dispatch", target_arch = "aarch64"))]
            // SAFETY: NEON is baseline on AArch64.
            return unsafe { self.scan_neon_ascii_any(haystack) };

            #[cfg(all(feature = "simd_runtime_dispatch", target_arch = "x86_64"))]
            if is_x86_feature_detected!("avx512vbmi") {
                // SAFETY: AVX512-VBMI support was confirmed at runtime.
                return unsafe { self.scan_avx512vbmi_ascii_any(haystack) };
            }

            #[cfg(not(any(
                all(feature = "simd_runtime_dispatch", target_arch = "aarch64"),
                all(feature = "simd_runtime_dispatch", target_arch = "x86_64")
            )))]
            return self.scan_scalar_range_any_ascii(haystack, 0, haystack.len() - 1);
        }

        #[cfg(all(feature = "simd_runtime_dispatch", target_arch = "aarch64"))]
        // SAFETY: NEON is baseline on AArch64.
        return unsafe { self.scan_neon_any(haystack) };

        #[cfg(all(feature = "simd_runtime_dispatch", target_arch = "x86_64"))]
        if is_x86_feature_detected!("avx512vbmi") {
            // SAFETY: AVX512-VBMI support was confirmed at runtime.
            return unsafe { self.scan_avx512vbmi_any(haystack) };
        }

        #[cfg(not(any(
            all(feature = "simd_runtime_dispatch", target_arch = "aarch64"),
            all(feature = "simd_runtime_dispatch", target_arch = "x86_64")
        )))]
        return self.scan_scalar_range_any(haystack, 0, haystack.len() - 1);

        #[allow(unreachable_code)]
        self.scan_scalar_range_any(haystack, 0, haystack.len() - 1)
    }

    #[inline(always)]
    fn scan_multi_dispatch_any_ascii_lead(&self, haystack: &[u8]) -> bool {
        debug_assert!(self.all_patterns_ascii);
        debug_assert!(!haystack.is_empty() && haystack[0] < 0x80);

        if haystack.len() < 2 {
            return false;
        }

        #[cfg(all(feature = "simd_runtime_dispatch", target_arch = "aarch64"))]
        // SAFETY: NEON is baseline on AArch64.
        return unsafe { self.scan_neon_ascii_lead_any(haystack) };

        #[cfg(all(feature = "simd_runtime_dispatch", target_arch = "x86_64"))]
        if is_x86_feature_detected!("avx512vbmi") {
            // SAFETY: AVX512-VBMI support was confirmed at runtime.
            return unsafe { self.scan_avx512vbmi_ascii_lead_any(haystack) };
        }

        #[cfg(not(any(
            all(feature = "simd_runtime_dispatch", target_arch = "aarch64"),
            all(feature = "simd_runtime_dispatch", target_arch = "x86_64")
        )))]
        return self.scan_scalar_range_any_no_single_byte(haystack, 0, haystack.len() - 1);

        #[allow(unreachable_code)]
        self.scan_scalar_range_any_no_single_byte(haystack, 0, haystack.len() - 1)
    }

    /// Scalar fallback: scans positions `start..=end` through column mask tables.
    ///
    /// When patterns contain non-ASCII bytes, skips UTF-8 continuation bytes (0x80-0xBF)
    /// since no valid match can start there.
    #[inline(always)]
    fn scan_scalar_range(
        &self,
        haystack: &[u8],
        start: usize,
        end: usize,
        on_value: &mut impl FnMut(u32) -> bool,
    ) -> bool {
        for pos in start..=end {
            // Skip continuation bytes — matches can only start at lead or ASCII bytes.
            // When all patterns are ASCII, column scan self-filters non-ASCII bytes.
            if !self.all_patterns_ascii && (haystack[pos] & 0xC0) == 0x80 {
                continue;
            }
            let hit_mask = self.match_mask_at(haystack, pos);
            if hit_mask != 0 && self.verify_hits(haystack, pos, hit_mask, on_value) {
                return true;
            }
        }
        false
    }

    #[inline(always)]
    fn scan_scalar_range_any(&self, haystack: &[u8], start: usize, end: usize) -> bool {
        for pos in start..=end {
            let byte = haystack[pos];
            if self.single_byte_contains(byte) {
                return true;
            }
            if !self.all_patterns_ascii && (byte & 0xC0) == 0x80 {
                continue;
            }
            let hit_mask = self.match_mask_at(haystack, pos);
            if hit_mask != 0 && self.verify_hits_any(haystack, pos, hit_mask) {
                return true;
            }
        }
        false
    }

    #[inline(always)]
    fn scan_scalar_range_any_no_single_byte(
        &self,
        haystack: &[u8],
        start: usize,
        end: usize,
    ) -> bool {
        for pos in start..=end {
            let byte = haystack[pos];
            if !self.all_patterns_ascii && (byte & 0xC0) == 0x80 {
                continue;
            }
            let hit_mask = self.match_mask_at(haystack, pos);
            if hit_mask != 0 && self.verify_hits_any(haystack, pos, hit_mask) {
                return true;
            }
        }
        false
    }

    /// Scalar scan that skips non-ASCII haystack positions.
    #[inline(always)]
    fn scan_scalar_range_ascii(
        &self,
        haystack: &[u8],
        start: usize,
        end: usize,
        on_value: &mut impl FnMut(u32) -> bool,
    ) -> bool {
        for pos in start..=end {
            if haystack[pos] >= 0x80 {
                continue;
            }
            let hit_mask = self.match_mask_at(haystack, pos);
            if hit_mask != 0 && self.verify_hits(haystack, pos, hit_mask, on_value) {
                return true;
            }
        }

        false
    }

    #[inline(always)]
    fn scan_scalar_range_any_ascii(&self, haystack: &[u8], start: usize, end: usize) -> bool {
        for pos in start..=end {
            let byte = haystack[pos];
            if byte >= 0x80 {
                continue;
            }
            if self.single_byte_contains(byte) {
                return true;
            }
            let hit_mask = self.match_mask_at(haystack, pos);
            if hit_mask != 0 && self.verify_hits_any(haystack, pos, hit_mask) {
                return true;
            }
        }

        false
    }

    /// Computes the bucket hit bitmask at a single haystack position.
    #[inline(always)]
    fn match_mask_at(&self, haystack: &[u8], start: usize) -> u8 {
        // Clip to available bytes and to max_prefix_len.  Wildcarded columns
        // (bit already cleared for all rows) would contribute zero to state anyway,
        // so omitting them is equivalent.  Any resulting false positives for patterns
        // longer than `available` are filtered in verify_bucket.
        let available = (haystack.len() - start).min(self.max_prefix_len);
        let mut state = 0u8;

        for column in 0..available {
            let byte = haystack[start + column];
            state |= self.low_mask[column][(byte & 0x3F) as usize]
                | self.high_mask[column][((byte >> 1) & 0x3F) as usize];
        }

        !state
    }

    /// Iterates set bits in `hit_mask`, verifying each bucket.
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
    fn verify_hits_any(&self, haystack: &[u8], start: usize, mut hit_mask: u8) -> bool {
        while hit_mask != 0 {
            let bucket = hit_mask.trailing_zeros() as usize;
            hit_mask &= hit_mask - 1;

            if self.verify_bucket_any(haystack, start, bucket) {
                return true;
            }
        }

        false
    }

    /// Exact-match verification for all prefix lengths in one bucket.
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
            let Some(group) = bv.groups[len_idx].get(key) else {
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

    #[inline(always)]
    fn verify_bucket_any(&self, haystack: &[u8], start: usize, bucket: usize) -> bool {
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
            let Some(group) = bv.groups[len_idx].get(key) else {
                continue;
            };

            if !group.exact_values.is_empty() {
                return true;
            }

            for literal in &group.long_literals {
                let len = literal.bytes.len();
                if start + len > haystack.len() {
                    continue;
                }

                if haystack[start + prefix_len..start + len] == literal.bytes[prefix_len..] {
                    return true;
                }
            }
        }

        false
    }
}

/// Packs `bytes` (up to 8) into a little-endian `u64` for fast prefix comparison.
#[inline(always)]
fn prefix_key(bytes: &[u8]) -> u64 {
    const { assert!(cfg!(target_endian = "little")) };
    debug_assert!(!bytes.is_empty() && bytes.len() <= 8);
    let mut key = 0u64;
    // SAFETY: `bytes.len()` is 1..=8 (callers pass prefix slices of length 2..=8
    // for multi-byte patterns, or 1 for single-byte). The destination `key` is 8
    // bytes. On little-endian targets the raw copy produces the same value as the
    // manual shift-and-OR loop. The remaining upper bytes stay zero from init.
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), (&raw mut key).cast::<u8>(), bytes.len());
    }
    key
}
