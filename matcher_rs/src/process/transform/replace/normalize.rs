//! Unicode normalization replacement via page-table lookup + fused streaming
//! scan.
//!
//! Data sourced from `unicodedata.normalize("NFKC", ch).casefold()`. All 8,633
//! keys are single Unicode codepoints (verified at build time). Cannot use
//! [`skip_ascii_simd`](super::skip_ascii_simd) because A–Z have casefold
//! mappings; ASCII bytes are checked inline instead.
//!
//! Provides two consumption modes: materialized
//! [`replace`](NormalizeMatcher::replace) (allocates a `String`) and streaming
//! [`filter_bytes`](NormalizeMatcher::filter_bytes) (yields bytes one at a time
//! for fused normalize-scan without allocation).

use std::borrow::Cow;

use super::{decode_page_table, decode_utf8_raw, page_table_lookup, replace_spans, unpack_str_ref};

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
///
/// Uses a unified `remaining` slice to handle both replacement-string bytes
/// and multi-byte continuation bytes, reducing the per-byte branch count
/// from 3 to 2 on the hot path.
pub(crate) struct NormalizeFilterIterator<'a> {
    bytes: &'a [u8],
    offset: usize,
    /// Pending bytes to yield before decoding the next codepoint.
    ///
    /// Points into either `self.strings` (replacement bytes) or `self.bytes`
    /// (continuation bytes of an unmapped multi-byte character). Empty when
    /// at a codepoint boundary ready for fresh decode.
    remaining: &'a [u8],
    l1: &'a [u16],
    l2: &'a [u32],
    strings: &'a str,
}

impl Iterator for NormalizeFilterIterator<'_> {
    type Item = u8;

    #[inline(always)]
    fn next(&mut self) -> Option<u8> {
        // Fast path: yield from pending slice (replacement OR continuation bytes).
        if let Some((&byte, rest)) = self.remaining.split_first() {
            self.remaining = rest;
            return Some(byte);
        }

        if self.offset >= self.bytes.len() {
            return None;
        }

        // SAFETY: offset < len checked above.
        let byte = unsafe { *self.bytes.get_unchecked(self.offset) };

        if byte < 0x80 {
            self.offset += 1;
            if byte.is_ascii_uppercase()
                && let Some(value) = page_table_lookup(byte as u32, self.l1, self.l2)
                && let Some(s) = unpack_str_ref(value, self.strings)
            {
                let s_bytes = s.as_bytes();
                self.remaining = &s_bytes[1..];
                return Some(s_bytes[0]);
            }
            return Some(byte);
        }

        // SAFETY: byte >= 0x80 in a valid UTF-8 &str means multi-byte lead byte.
        let (cp, char_len) = unsafe { decode_utf8_raw(self.bytes, self.offset) };

        if let Some(value) = page_table_lookup(cp, self.l1, self.l2)
            && let Some(s) = unpack_str_ref(value, self.strings)
        {
            self.offset += char_len;
            let s_bytes = s.as_bytes();
            self.remaining = &s_bytes[1..];
            return Some(s_bytes[0]);
        }

        // Unmapped multi-byte: yield lead byte, buffer continuation bytes.
        let cont_start = self.offset + 1;
        self.offset += char_len;
        self.remaining = &self.bytes[cont_start..self.offset];
        Some(byte)
    }
}

// ---------------------------------------------------------------------------
// Matcher
// ---------------------------------------------------------------------------

/// Two-stage page-table matcher for Unicode normalization replacement.
///
/// L2 entries encode `(byte_offset << 8) | byte_length` into a shared string
/// buffer, same layout as [`super::romanize::RomanizeMatcher`].
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

    /// Replaces normalizable codepoints (including ASCII uppercase A–Z).
    ///
    /// Returns `None` when `text` contains no normalizable characters. The
    /// `bool` in the return tuple indicates whether the output is entirely
    /// ASCII.
    ///
    /// ```ignore
    /// let matcher = NormalizeMatcher::new(NORMALIZE_L1_BYTES, NORMALIZE_L2_BYTES, NORMALIZE_STR_BYTES);
    /// let result = matcher.replace("Hello WORLD").unwrap();
    /// assert_eq!(result, "hello world"); // casefold
    /// ```
    pub(crate) fn replace(&self, text: &str) -> Option<String> {
        replace_spans(text, self.iter(text))
    }

    /// Decodes L1/L2 page tables from build-time binary artifacts.
    pub(crate) fn new(l1: &'static [u8], l2: &'static [u8], strings: &'static str) -> Self {
        let (l1, l2) = decode_page_table(l1, l2);
        Self {
            l1,
            l2,
            strings: Cow::Borrowed(strings),
        }
    }

    /// Returns a streaming byte iterator over the normalized form of `text`.
    ///
    /// Used by the fused normalize-scan path to feed normalized bytes directly
    /// into the Aho-Corasick automaton without materializing the full string.
    #[inline(always)]
    pub(crate) fn filter_bytes<'a>(&'a self, text: &'a str) -> NormalizeFilterIterator<'a> {
        NormalizeFilterIterator {
            bytes: text.as_bytes(),
            offset: 0,
            remaining: &[],
            l1: &self.l1,
            l2: &self.l2,
            strings: self.strings.as_ref(),
        }
    }
}
