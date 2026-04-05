//! Construction of [`HarryMatcher`] from raw pattern–value pairs.
//!
//! Patterns are grouped into 64 buckets (low nibble × high nibble of the first
//! byte), with prefix keys up to [`MAX_SCAN_LEN`] bytes used for multi-byte
//! verification. Single-byte patterns are separated into a dedicated lookup
//! table. Mask columns are wildcarded per bucket so the unified automaton
//! covers all prefix lengths in a single scan pass.
//!
//! See the [parent module](super) for the algorithm overview.

use std::collections::HashMap;

use super::{
    BucketLiteral, BucketVerify, HARRY_MIN_PATTERN_COUNT, HarryMatcher, MASK_ROWS, MAX_SCAN_LEN,
    N_BUCKETS, PrefixGroup, PrefixMap, prefix_key,
};

impl HarryMatcher {
    /// Build a [`HarryMatcher`] from a slice of `(pattern, value)` pairs.
    ///
    /// Accepts both ASCII and non-ASCII (CJK) patterns.
    ///
    /// # Returns
    ///
    /// `None` when any of these conditions hold:
    /// - `patterns.len()` < `HARRY_MIN_PATTERN_COUNT` (too few patterns for SIMD payoff)
    /// - Any pattern is empty
    /// - No pattern has length ≥ 2 (single-byte-only sets lack multi-column coverage)
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
        let mut single_byte_keys = Vec::new();
        let mut single_byte_match_mask = [0u64; 2];
        let mut has_single_byte = false;
        let mut low_mask = Box::new([[0xFFu8; MASK_ROWS]; MAX_SCAN_LEN]);
        let mut high_mask = Box::new([[0xFFu8; MASK_ROWS]; MAX_SCAN_LEN]);

        // Use a temporary HashMap during build, then convert to sorted PrefixMap.
        let mut build_groups: [_; N_BUCKETS] = std::array::from_fn(|_| {
            std::array::from_fn::<HashMap<u64, PrefixGroup>, { MAX_SCAN_LEN - 1 }, _>(|_| {
                HashMap::new()
            })
        });
        let mut build_length_masks = [0u8; N_BUCKETS];

        for &(pattern, value) in patterns {
            let bytes = pattern.as_bytes();
            if bytes.len() == 1 {
                single_byte_values[bytes[0] as usize].push(value);
                single_byte_keys.push(bytes[0]);
                single_byte_match_mask[(bytes[0] >> 6) as usize] |= 1u64 << (bytes[0] & 0x3F);
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

            let len_idx = actual_prefix_len - 2;
            build_length_masks[bucket] |= 1u8 << len_idx;
            let key = prefix_key(&bytes[..actual_prefix_len]);
            let group = build_groups[bucket][len_idx].entry(key).or_default();
            if bytes.len() == actual_prefix_len {
                group.exact_values.push(value);
            } else {
                group.long_literals.push(BucketLiteral {
                    bytes: bytes.to_vec().into_boxed_slice(),
                    value,
                });
            }
        }

        // Convert temporary HashMaps to sorted PrefixMaps.
        let bucket_verify: [BucketVerify; N_BUCKETS] = std::array::from_fn(|bucket| {
            let groups = std::array::from_fn(|len_idx| {
                let map = &mut build_groups[bucket][len_idx];
                PrefixMap::from_unsorted(map.drain())
            });
            BucketVerify {
                length_mask: build_length_masks[bucket],
                groups,
            }
        });

        // Wildcard each bucket's columns beyond its shortest pattern length.
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

        let all_patterns_ascii = patterns
            .iter()
            .all(|(p, _)| p.as_bytes().iter().all(|&b| b < 0x80));

        let max_prefix_len = patterns
            .iter()
            .filter(|(p, _)| p.len() >= 2)
            .map(|(p, _)| p.len().min(MAX_SCAN_LEN))
            .max()
            .unwrap_or(MAX_SCAN_LEN);

        Some(Self {
            single_byte_values,
            single_byte_keys: single_byte_keys.into_boxed_slice(),
            single_byte_match_mask,
            has_single_byte,
            low_mask,
            high_mask,
            bucket_verify,
            all_patterns_ascii,
            max_prefix_len,
        })
    }
}
