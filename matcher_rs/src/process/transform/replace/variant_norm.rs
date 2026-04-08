//! CJK variant normalization via page-table lookup.
//!
//! Merges Chinese Traditional→Simplified (OpenCC), Japanese Kyūjitai→Shinjitai,
//! and half-width katakana→full-width into a single
//! two-stage page table. Each mapped codepoint is stored as a `u32` in L2.
//! Since all keys are non-ASCII CJK characters, [`skip_ascii_simd`] bypasses
//! ASCII runs in O(1) per SIMD chunk. Output is always non-ASCII (CJK→CJK).

use super::{decode_page_table, decode_utf8_raw, page_table_lookup, replace_scan, skip_ascii_simd};
use crate::process::transform::filter::{CodepointFilter, FilterAction, FilterIterator};

// ---------------------------------------------------------------------------
// Streaming filter (for fused variant-norm-scan)
// ---------------------------------------------------------------------------

/// [`CodepointFilter`] for CJK variant normalization.
///
/// Keeps ASCII bytes unchanged; replaces mapped non-ASCII CJK codepoints with
/// their normalized equivalents via page-table lookup.
pub(crate) struct VariantNormFilter<'a> {
    l1: &'a [u16],
    l2: &'a [u32],
}

impl<'a> CodepointFilter<'a> for VariantNormFilter<'a> {
    #[inline(always)]
    fn filter_ascii(&self, _byte: u8) -> FilterAction<'a> {
        FilterAction::Keep
    }

    #[inline(always)]
    fn filter_codepoint(&self, cp: u32) -> FilterAction<'a> {
        if let Some(mapped_cp) = page_table_lookup(cp, self.l1, self.l2)
            && mapped_cp != cp
        {
            FilterAction::ReplaceCodepoint(mapped_cp)
        } else {
            FilterAction::Keep
        }
    }
}

/// Materialized find iterator for variant normalization.
///
/// Yields `(byte_start, byte_end, replacement_char)` tuples for each CJK
/// codepoint that has a variant mapping. Uses [`skip_ascii_simd`] to jump
/// over ASCII runs.
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
                // SAFETY: Page table values are valid Unicode codepoints assigned at build
                // time.
                let mapped = unsafe {
                    core::hint::assert_unchecked(mapped_cp <= 0x10FFFF);
                    char::from_u32_unchecked(mapped_cp)
                };
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
    /// Returns a find iterator over variant-normalizable codepoints in `text`.
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
    pub(crate) fn filter_bytes<'a>(
        &'a self,
        text: &'a str,
    ) -> FilterIterator<'a, VariantNormFilter<'a>> {
        FilterIterator::new(
            text,
            VariantNormFilter {
                l1: &self.l1,
                l2: &self.l2,
            },
        )
    }

    /// Decodes L1/L2 page tables from build-time binary artifacts.
    pub(crate) fn new(l1: &'static [u8], l2: &'static [u8]) -> Self {
        let (l1, l2) = decode_page_table(l1, l2);
        Self { l1, l2 }
    }
}
