//! Traditional-to-Simplified Chinese (T2S) replacement via page-table lookup.
//!
//! Data sourced from OpenCC (`t2s`, `tw2s`, `hk2s`). The two-stage page table
//! maps each Traditional codepoint to its Simplified equivalent (stored as a
//! `u32` codepoint in L2). Since all keys are non-ASCII CJK characters,
//! [`skip_ascii_simd`] bypasses ASCII runs in O(1)
//! per SIMD chunk. Output is always non-ASCII (CJK→CJK).

use super::{decode_page_table, decode_utf8_raw, page_table_lookup, replace_scan, skip_ascii_simd};

struct FanjianFindIter<'a> {
    l1: &'a [u16],
    l2: &'a [u32],
    text: &'a str,
    byte_offset: usize,
}

impl<'a> Iterator for FanjianFindIter<'a> {
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
            // SAFETY: SIMD skip positioned `start` at a non-ASCII byte in a valid UTF-8 `&str`.
            let (cp, char_len) = unsafe { decode_utf8_raw(bytes, start) };
            self.byte_offset += char_len;

            if let Some(mapped_cp) = page_table_lookup(cp, self.l1, self.l2)
                && mapped_cp != cp
            {
                debug_assert!(char::from_u32(mapped_cp).is_some());
                // SAFETY: Page table values are valid Unicode codepoints assigned at build time.
                let mapped = unsafe { char::from_u32_unchecked(mapped_cp) };
                return Some((start, self.byte_offset, mapped));
            }
        }
    }
}

/// Two-stage page-table matcher for Traditional-to-Simplified Chinese replacement.
///
/// Each non-zero L2 entry is the Unicode codepoint of the Simplified equivalent.
/// Constructed once from build-time binary artifacts via [`FanjianMatcher::new`].
#[derive(Clone)]
pub(crate) struct FanjianMatcher {
    l1: Box<[u16]>,
    l2: Box<[u32]>,
}

impl FanjianMatcher {
    #[inline(always)]
    fn iter<'a>(&'a self, text: &'a str) -> FanjianFindIter<'a> {
        FanjianFindIter {
            l1: &self.l1,
            l2: &self.l2,
            text,
            byte_offset: 0,
        }
    }

    /// Replaces Traditional Chinese codepoints with their Simplified equivalents.
    ///
    /// Returns `None` when `text` contains no Traditional characters (zero-alloc
    /// fast path).
    ///
    /// ```ignore
    /// let matcher = FanjianMatcher::new(FANJIAN_L1_BYTES, FANJIAN_L2_BYTES);
    /// assert_eq!(matcher.replace("國語"), Some("国语".to_string()));
    /// assert!(matcher.replace("hello").is_none()); // no T→S mapping
    /// ```
    pub(crate) fn replace(&self, text: &str) -> Option<String> {
        replace_scan(text, self.iter(text))
    }

    /// Decodes L1/L2 page tables from build-time binary artifacts.
    pub(crate) fn new(l1: &'static [u8], l2: &'static [u8]) -> Self {
        let (l1, l2) = decode_page_table(l1, l2);
        Self { l1, l2 }
    }
}
