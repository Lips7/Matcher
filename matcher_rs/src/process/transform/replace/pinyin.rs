//! CJK-to-Pinyin romanization replacement via page-table lookup.
//!
//! Data sourced from `pypinyin` (no-tone, single-codepoint entries). L2 entries
//! are packed as `(byte_offset << 8) | byte_length` into a shared string buffer.
//! The `PinYinChar` variant trims inter-syllable spaces via
//! [`trim_pinyin_packed`] at construction time.
//! All keys are non-ASCII CJK, so [`skip_ascii_simd`]
//! bypasses ASCII runs.

use std::borrow::Cow;

use super::{
    decode_page_table, decode_utf8_raw, page_table_lookup, replace_spans_tracking_ascii,
    skip_ascii_simd, trim_pinyin_packed, unpack_str_ref,
};

struct PinyinFindIter<'a> {
    l1: &'a [u16],
    l2: &'a [u32],
    strings: &'a str,
    text: &'a str,
    byte_offset: usize,
}

impl<'a> Iterator for PinyinFindIter<'a> {
    type Item = (usize, usize, &'a str);

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
            // SAFETY: `skip_ascii_simd` positioned `start` at a non-ASCII byte in a valid UTF-8 `&str`.
            let (cp, char_len) = unsafe { decode_utf8_raw(bytes, start) };
            self.byte_offset += char_len;

            if let Some(value) = page_table_lookup(cp, self.l1, self.l2)
                && let Some(s) = unpack_str_ref(value, self.strings)
            {
                return Some((start, self.byte_offset, s));
            }
        }
    }
}

/// Two-stage page-table matcher for CJK-to-Pinyin replacement.
///
/// Each non-zero L2 entry encodes `(byte_offset << 8) | byte_length` into a
/// shared string buffer containing all Pinyin syllables concatenated.
///
/// When `trim_space` is `true` (used by the `PinYinChar` variant), each L2
/// entry is adjusted by [`trim_pinyin_packed`] to exclude surrounding spaces.
#[derive(Clone)]
pub(crate) struct PinyinMatcher {
    l1: Box<[u16]>,
    l2: Box<[u32]>,
    strings: Cow<'static, str>,
}

impl PinyinMatcher {
    #[inline(always)]
    fn iter<'a>(&'a self, text: &'a str) -> PinyinFindIter<'a> {
        PinyinFindIter {
            l1: &self.l1,
            l2: &self.l2,
            strings: self.strings.as_ref(),
            text,
            byte_offset: 0,
        }
    }

    /// Replaces CJK codepoints with their Pinyin romanization.
    ///
    /// Returns `None` when `text` contains no CJK characters. The `bool` in the
    /// return tuple indicates whether the output is entirely ASCII.
    ///
    /// ```ignore
    /// let matcher = PinyinMatcher::new(PINYIN_L1_BYTES, PINYIN_L2_BYTES, PINYIN_STR_BYTES, false);
    /// let (result, is_ascii) = matcher.replace("中国").unwrap();
    /// assert_eq!(result, " zhong  guo "); // space-separated syllables
    /// assert!(is_ascii);
    /// ```
    pub(crate) fn replace(&self, text: &str) -> Option<(String, bool)> {
        replace_spans_tracking_ascii(text, self.iter(text))
    }

    /// Decodes L1/L2 page tables from build-time binary artifacts.
    ///
    /// When `trim_space` is `true`, L2 entries are adjusted to exclude leading
    /// and trailing spaces from each syllable (used by `PinYinChar`).
    pub(crate) fn new(
        l1: &'static [u8],
        l2: &'static [u8],
        strings: &'static str,
        trim_space: bool,
    ) -> Self {
        let (l1, mut l2) = decode_page_table(l1, l2);
        if trim_space {
            for value in l2.iter_mut() {
                *value = trim_pinyin_packed(*value, strings);
            }
        }
        Self {
            l1,
            l2,
            strings: Cow::Borrowed(strings),
        }
    }
}
