//! Unicode normalization replacement via page-table lookup + fused streaming scan.
//!
//! All 8,633 normalize keys are single Unicode codepoints (verified at build time).
//! Cannot use [`skip_ascii_simd`](super::skip_ascii_simd) because A-Z have
//! normalize mappings (casefold); ASCII bytes are checked inline instead.

#[cfg(feature = "runtime_build")]
use ahash::AHashMap;
use std::borrow::Cow;

#[cfg(feature = "runtime_build")]
use super::build_2_stage_table;
#[cfg(not(feature = "runtime_build"))]
use super::decode_page_table;
use super::{decode_utf8_raw, page_table_lookup, replace_spans_tracking_ascii, unpack_str_ref};

// ---------------------------------------------------------------------------
// Find iterator (for materialized replace)
// ---------------------------------------------------------------------------

struct NormalizeFindIter<'a> {
    l1: &'a [u16],
    l2: &'a [u32],
    strings: &'a str,
    text: &'a str,
    byte_offset: usize,
}

impl<'a> Iterator for NormalizeFindIter<'a> {
    type Item = (usize, usize, &'a str);

    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        let bytes = self.text.as_bytes();
        let len = bytes.len();

        loop {
            if self.byte_offset >= len {
                return None;
            }

            let b = bytes[self.byte_offset];
            if b < 0x80 {
                let start = self.byte_offset;
                self.byte_offset += 1;
                if b.is_ascii_uppercase()
                    && let Some(value) = page_table_lookup(b as u32, self.l1, self.l2)
                    && let Some(s) = unpack_str_ref(value, self.strings)
                {
                    return Some((start, start + 1, s));
                }
                continue;
            }

            let start = self.byte_offset;
            // SAFETY: `b >= 0x80` in a valid UTF-8 `&str` means multi-byte lead byte.
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

// ---------------------------------------------------------------------------
// Streaming byte iterator (for fused normalize-scan)
// ---------------------------------------------------------------------------

/// Streaming byte iterator that yields normalized bytes from a UTF-8 string.
///
/// Created by [`NormalizeMatcher::filter_bytes`]. Unmapped codepoints pass
/// through byte-for-byte; mapped codepoints emit their replacement string's
/// bytes. Output is valid UTF-8, satisfying `daachorse`'s
/// `find_overlapping_iter_from_iter` safety requirement.
pub(crate) struct NormalizeFilterIterator<'a> {
    bytes: &'a [u8],
    offset: usize,
    char_remaining: u8,
    replace_bytes: &'a [u8],
    replace_pos: usize,
    l1: &'a [u16],
    l2: &'a [u32],
    strings: &'a str,
}

impl Iterator for NormalizeFilterIterator<'_> {
    type Item = u8;

    #[inline(always)]
    fn next(&mut self) -> Option<u8> {
        if self.replace_pos < self.replace_bytes.len() {
            let byte = self.replace_bytes[self.replace_pos];
            self.replace_pos += 1;
            return Some(byte);
        }

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
            if byte.is_ascii_uppercase()
                && let Some(value) = page_table_lookup(byte as u32, self.l1, self.l2)
                && let Some(s) = unpack_str_ref(value, self.strings)
            {
                self.offset += 1;
                self.replace_bytes = s.as_bytes();
                self.replace_pos = 1;
                return Some(self.replace_bytes[0]);
            }
            self.offset += 1;
            return Some(byte);
        }

        // SAFETY: byte >= 0x80 in a valid UTF-8 &str means multi-byte lead byte.
        let (cp, char_len) = unsafe { decode_utf8_raw(self.bytes, self.offset) };

        if let Some(value) = page_table_lookup(cp, self.l1, self.l2)
            && let Some(s) = unpack_str_ref(value, self.strings)
        {
            self.offset += char_len;
            self.replace_bytes = s.as_bytes();
            self.replace_pos = 1;
            return Some(self.replace_bytes[0]);
        }

        self.offset += 1;
        self.char_remaining = (char_len - 1) as u8;
        Some(byte)
    }
}

// ---------------------------------------------------------------------------
// Matcher
// ---------------------------------------------------------------------------

/// Two-stage page-table matcher for Unicode normalization replacement.
///
/// L2 entries encode `(byte_offset << 8) | byte_length` into a shared string
/// buffer, same layout as [`super::pinyin::PinyinMatcher`].
#[derive(Clone)]
pub(crate) struct NormalizeMatcher {
    l1: Box<[u16]>,
    l2: Box<[u32]>,
    strings: Cow<'static, str>,
}

impl NormalizeMatcher {
    #[inline(always)]
    fn iter<'a>(&'a self, text: &'a str) -> NormalizeFindIter<'a> {
        NormalizeFindIter {
            l1: &self.l1,
            l2: &self.l2,
            strings: self.strings.as_ref(),
            text,
            byte_offset: 0,
        }
    }

    pub(crate) fn replace(&self, text: &str) -> Option<(String, bool)> {
        replace_spans_tracking_ascii(text, self.iter(text))
    }

    #[cfg(not(feature = "runtime_build"))]
    pub(crate) fn new(l1: &'static [u8], l2: &'static [u8], strings: &'static str) -> Self {
        let (l1, l2) = decode_page_table(l1, l2);
        Self {
            l1,
            l2,
            strings: Cow::Borrowed(strings),
        }
    }

    #[inline(always)]
    pub(crate) fn filter_bytes<'a>(&'a self, text: &'a str) -> NormalizeFilterIterator<'a> {
        NormalizeFilterIterator {
            bytes: text.as_bytes(),
            offset: 0,
            char_remaining: 0,
            replace_bytes: &[],
            replace_pos: 0,
            l1: &self.l1,
            l2: &self.l2,
            strings: self.strings.as_ref(),
        }
    }

    #[cfg(feature = "runtime_build")]
    pub(crate) fn from_dict(dict: AHashMap<&'static str, &'static str>) -> Self {
        let mut strings = String::new();
        let packed: AHashMap<u32, u32> = dict
            .into_iter()
            .map(|(key, value)| {
                assert!(
                    key.chars().count() == 1,
                    "Normalize key must be exactly one codepoint: {key:?}"
                );
                let cp = key.chars().next().unwrap() as u32;
                let offset = strings.len() as u32;
                let length = value.len() as u32;
                strings.push_str(value);
                (cp, (offset << 8) | length)
            })
            .collect();
        let (l1, l2) = build_2_stage_table(&packed);
        Self {
            l1: l1.into_boxed_slice(),
            l2: l2.into_boxed_slice(),
            strings: Cow::Owned(strings),
        }
    }
}
