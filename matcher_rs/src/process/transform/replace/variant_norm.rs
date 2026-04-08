//! CJK variant normalization via page-table lookup.
//!
//! Merges Chinese Traditional→Simplified (OpenCC), Japanese Kyūjitai→Shinjitai,
//! and half-width katakana→full-width into a single
//! two-stage page table. Each mapped codepoint is stored as a `u32` in L2.
//! Since all keys are non-ASCII CJK characters, [`skip_ascii_simd`] bypasses
//! ASCII runs in O(1) per SIMD chunk. Output is always non-ASCII (CJK→CJK).

use super::{decode_page_table, decode_utf8_raw, page_table_lookup, replace_scan, skip_ascii_simd};

// ---------------------------------------------------------------------------
// Streaming byte iterator (for fused variant-norm-scan)
// ---------------------------------------------------------------------------

/// Streaming byte iterator that yields variant-normalized bytes from a UTF-8
/// string.
///
/// Created by [`VariantNormMatcher::filter_bytes`]. Unmapped codepoints pass
/// through byte-for-byte; mapped codepoints emit their normalized replacement's
/// UTF-8 bytes. Output is valid UTF-8, satisfying `daachorse`'s
/// `find_overlapping_iter_from_iter` safety requirement.
///
/// ASCII bytes always pass through unchanged (VariantNorm only maps non-ASCII
/// CJK codepoints), so `skip_ascii_simd` is not used here — the iterator yields
/// ASCII bytes directly for maximum throughput in the fused scan path.
pub(crate) struct VariantNormFilterIterator<'a> {
    bytes: &'a [u8],
    offset: usize,
    /// Remaining bytes to yield from the current kept (unmapped) multi-byte
    /// character.
    char_remaining: u8,
    /// UTF-8 encoding of the current replacement character.
    replace_buf: [u8; 4],
    replace_len: u8,
    replace_pos: u8,
    l1: &'a [u16],
    l2: &'a [u32],
}

impl Iterator for VariantNormFilterIterator<'_> {
    type Item = u8;

    #[inline(always)]
    fn next(&mut self) -> Option<u8> {
        // Fast path: mid-replacement character bytes.
        if self.replace_pos < self.replace_len {
            let byte = self.replace_buf[self.replace_pos as usize];
            self.replace_pos += 1;
            return Some(byte);
        }

        // Fast path: mid-character continuation bytes (unmapped passthrough).
        if self.char_remaining > 0 {
            // SAFETY: within a kept multi-byte character; offset is in bounds.
            let byte = unsafe { *self.bytes.get_unchecked(self.offset) };
            self.offset += 1;
            self.char_remaining -= 1;
            return Some(byte);
        }

        if self.offset >= self.bytes.len() {
            return None;
        }

        // SAFETY: offset < len checked above.
        let byte = unsafe { *self.bytes.get_unchecked(self.offset) };

        if byte < 0x80 {
            // ASCII: always passthrough (VariantNorm only maps non-ASCII).
            self.offset += 1;
            return Some(byte);
        }

        // Non-ASCII: decode codepoint, check page table.
        // SAFETY: byte >= 0x80 in a valid UTF-8 &str means multi-byte lead byte.
        let (cp, char_len) = unsafe { decode_utf8_raw(self.bytes, self.offset) };

        if let Some(mapped_cp) = page_table_lookup(cp, self.l1, self.l2)
            && mapped_cp != cp
        {
            // Mapped: encode replacement char and yield first byte.
            self.offset += char_len;
            // SAFETY: page table values are valid Unicode codepoints assigned at build
            // time.
            let mapped = unsafe { char::from_u32_unchecked(mapped_cp) };
            let len = mapped.len_utf8();
            mapped.encode_utf8(&mut self.replace_buf);
            self.replace_len = len as u8;
            self.replace_pos = 1;
            return Some(self.replace_buf[0]);
        }

        // Unmapped: yield first byte, set remaining for continuation bytes.
        self.offset += 1;
        self.char_remaining = (char_len - 1) as u8;
        Some(byte)
    }
}

struct VariantNormFindIter<'a> {
    l1: &'a [u16],
    l2: &'a [u32],
    text: &'a str,
    byte_offset: usize,
}

impl<'a> Iterator for VariantNormFindIter<'a> {
    type Item = (usize, usize, char);

    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        let bytes = self.text.as_bytes();
        let len = bytes.len();

        loop {
            self.byte_offset = skip_ascii_simd(bytes, self.byte_offset);
            if self.byte_offset >= len {
                return None;
            }

            let start = self.byte_offset;
            // SAFETY: SIMD skip positioned `start` at a non-ASCII byte in a valid UTF-8
            // `&str`.
            let (cp, char_len) = unsafe { decode_utf8_raw(bytes, start) };
            self.byte_offset += char_len;

            if let Some(mapped_cp) = page_table_lookup(cp, self.l1, self.l2)
                && mapped_cp != cp
            {
                debug_assert!(char::from_u32(mapped_cp).is_some());
                // SAFETY: Page table values are valid Unicode codepoints assigned at build
                // time.
                let mapped = unsafe { char::from_u32_unchecked(mapped_cp) };
                return Some((start, self.byte_offset, mapped));
            }
        }
    }
}

/// Two-stage page-table matcher for CJK variant normalization.
///
/// Covers Chinese Traditional→Simplified, Japanese Kyūjitai→Shinjitai,
/// and half-width katakana→full-width. Each non-zero L2
/// entry is the Unicode codepoint of the normalized equivalent. Constructed
/// once from build-time binary artifacts via [`VariantNormMatcher::new`].
#[derive(Clone)]
pub(crate) struct VariantNormMatcher {
    l1: Box<[u16]>,
    l2: Box<[u32]>,
}

impl VariantNormMatcher {
    #[inline(always)]
    fn iter<'a>(&'a self, text: &'a str) -> VariantNormFindIter<'a> {
        VariantNormFindIter {
            l1: &self.l1,
            l2: &self.l2,
            text,
            byte_offset: 0,
        }
    }

    /// Replaces CJK variant codepoints with their normalized equivalents.
    ///
    /// Returns `None` when `text` contains no variant characters (zero-alloc
    /// fast path).
    ///
    /// ```ignore
    /// let matcher = VariantNormMatcher::new(VARIANT_NORM_L1_BYTES, VARIANT_NORM_L2_BYTES);
    /// assert_eq!(matcher.replace("國語"), Some("国语".to_string()));
    /// assert!(matcher.replace("hello").is_none()); // no variant mapping
    /// ```
    pub(crate) fn replace(&self, text: &str) -> Option<String> {
        replace_scan(text, self.iter(text))
    }

    /// Returns a streaming byte iterator over the variant-normalized form of
    /// `text`.
    ///
    /// Used by the fused variant-norm-scan path to feed transformed bytes
    /// directly into the Aho-Corasick automaton without materializing the
    /// full string.
    #[inline(always)]
    pub(crate) fn filter_bytes<'a>(&'a self, text: &'a str) -> VariantNormFilterIterator<'a> {
        VariantNormFilterIterator {
            bytes: text.as_bytes(),
            offset: 0,
            char_remaining: 0,
            replace_buf: [0; 4],
            replace_len: 0,
            replace_pos: 0,
            l1: &self.l1,
            l2: &self.l2,
        }
    }

    /// Decodes L1/L2 page tables from build-time binary artifacts.
    pub(crate) fn new(l1: &'static [u8], l2: &'static [u8]) -> Self {
        let (l1, l2) = decode_page_table(l1, l2);
        Self { l1, l2 }
    }
}
