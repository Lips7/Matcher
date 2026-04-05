//! Traditional-to-Simplified Chinese replacement via page-table lookup.

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

    pub(crate) fn replace(&self, text: &str) -> Option<String> {
        replace_scan(text, self.iter(text))
    }

    pub(crate) fn new(l1: &'static [u8], l2: &'static [u8]) -> Self {
        let (l1, l2) = decode_page_table(l1, l2);
        Self { l1, l2 }
    }
}
