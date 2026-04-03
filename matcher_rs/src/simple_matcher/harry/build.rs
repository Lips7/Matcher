use super::{
    BucketLiteral, BucketVerify, HARRY_MIN_PATTERN_COUNT, HarryMatcher, MASK_ROWS, MAX_SCAN_LEN,
    N_BUCKETS, prefix_key,
};

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
            has_single_byte,
            low_mask,
            high_mask,
            bucket_verify,
            all_patterns_ascii,
            max_prefix_len,
        })
    }
}
