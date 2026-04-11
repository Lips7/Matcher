//! Generic streaming byte iterator for codepoint-level text transformations.
//!
//! [`FilterIterator`] walks a UTF-8 byte slice, decodes each codepoint,
//! consults a [`CodepointFilter`] for the keep/delete/replace decision, and
//! yields output bytes one at a time. This provides the fused transform-scan
//! path used by the Aho-Corasick automaton without materializing an
//! intermediate `String`.
//!
//! Four transform engines implement [`CodepointFilter`]:
//! [`DeleteFilter`](super::delete), [`NormalizeFilter`](super::normalize),
//! [`RomanizeFilter`](super::romanize), and
//! [`VariantNormFilter`](super::variant_norm).

use super::utf8::decode_utf8_raw;

/// What to do with one codepoint during streaming iteration.
pub(crate) enum FilterAction<'a> {
    /// Yield the original bytes unchanged.
    Keep,
    /// Skip the codepoint entirely (delete).
    Delete,
    /// Replace with a borrowed byte slice (from a pre-encoded string buffer).
    ReplaceBytes(&'a [u8]),
    /// Replace with a single Unicode codepoint (encoded to UTF-8 at runtime).
    ReplaceCodepoint(u32),
}

/// Per-codepoint lookup strategy for a text transformation.
///
/// The lifetime `'a` ties returned [`FilterAction`] borrows to the underlying
/// data (page tables, string buffers) rather than to the `&self` reference,
/// allowing the iterator to store replacement slices across `next()` calls.
///
/// Implementations are monomorphized into [`FilterIterator`], so trait method
/// calls compile to direct inlined code with no virtual dispatch overhead.
pub(crate) trait CodepointFilter<'a> {
    /// Decides what to do with an ASCII byte (0x00–0x7F).
    fn filter_ascii(&self, byte: u8) -> FilterAction<'a>;

    /// Decides what to do with a non-ASCII codepoint.
    fn filter_codepoint(&self, cp: u32) -> FilterAction<'a>;
}

/// Streaming byte iterator that applies a [`CodepointFilter`] to a UTF-8
/// string.
///
/// Created by each matcher's `filter_bytes()` method. Output is valid UTF-8
/// (kept codepoints pass verbatim, replacements are valid strings or
/// codepoints), satisfying `daachorse`'s `find_overlapping_iter_from_iter`
/// safety requirement.
pub(crate) struct FilterIterator<'a, F> {
    bytes: &'a [u8],
    offset: usize,
    /// Borrowed pending bytes from a string-buffer replacement
    /// (`ReplaceBytes`). Empty when no replacement is pending.
    remaining: &'a [u8],
    /// Inline buffer for `ReplaceCodepoint` output or passthrough continuation
    /// bytes.
    buf: [u8; 4],
    buf_pos: u8,
    buf_len: u8,
    filter: F,
}

impl<'a, F> FilterIterator<'a, F> {
    #[inline(always)]
    pub(crate) fn new(text: &'a str, filter: F) -> Self {
        Self {
            bytes: text.as_bytes(),
            offset: 0,
            remaining: &[],
            buf: [0; 4],
            buf_pos: 0,
            buf_len: 0,
            filter,
        }
    }
}

impl<'a, F: CodepointFilter<'a>> Iterator for FilterIterator<'a, F> {
    type Item = u8;

    #[inline(always)]
    fn next(&mut self) -> Option<u8> {
        // Fast path 1: drain borrowed replacement slice.
        if let Some((&byte, rest)) = self.remaining.split_first() {
            self.remaining = rest;
            return Some(byte);
        }

        // Fast path 2: drain inline buffer (encoded codepoint or continuation).
        if self.buf_pos < self.buf_len {
            let byte = self.buf[self.buf_pos as usize];
            self.buf_pos += 1;
            return Some(byte);
        }

        loop {
            if self.offset >= self.bytes.len() {
                return None;
            }

            // SAFETY: The `offset >= len` guard above ensures `offset < len`.
            unsafe { core::hint::assert_unchecked(self.offset < self.bytes.len()) };
            let byte = self.bytes[self.offset];

            if byte < 0x80 {
                self.offset += 1;
                match self.filter.filter_ascii(byte) {
                    FilterAction::Keep => return Some(byte),
                    FilterAction::Delete => continue,
                    FilterAction::ReplaceBytes(s) => {
                        self.remaining = &s[1..];
                        return Some(s[0]);
                    }
                    FilterAction::ReplaceCodepoint(cp) => {
                        // SAFETY: filter implementations produce valid codepoints.
                        let ch = unsafe { char::from_u32_unchecked(cp) };
                        let len = ch.len_utf8();
                        ch.encode_utf8(&mut self.buf);
                        self.buf_len = len as u8;
                        self.buf_pos = 1;
                        return Some(self.buf[0]);
                    }
                }
            }

            // Non-ASCII: decode codepoint.
            // SAFETY: byte >= 0x80 in a valid UTF-8 &str means multi-byte lead.
            let (cp, char_len) = unsafe { decode_utf8_raw(self.bytes, self.offset) };

            match self.filter.filter_codepoint(cp) {
                FilterAction::Keep => {
                    self.offset += char_len;
                    // Point remaining at continuation bytes in source — no copy.
                    if char_len > 1 {
                        self.remaining = &self.bytes[self.offset - (char_len - 1)..self.offset];
                    }
                    return Some(byte);
                }
                FilterAction::Delete => {
                    self.offset += char_len;
                    continue;
                }
                FilterAction::ReplaceBytes(s) => {
                    self.offset += char_len;
                    self.remaining = &s[1..];
                    return Some(s[0]);
                }
                FilterAction::ReplaceCodepoint(mapped_cp) => {
                    self.offset += char_len;
                    // SAFETY: filter implementations produce valid codepoints.
                    let ch = unsafe { char::from_u32_unchecked(mapped_cp) };
                    let len = ch.len_utf8();
                    ch.encode_utf8(&mut self.buf);
                    self.buf_len = len as u8;
                    self.buf_pos = 1;
                    return Some(self.buf[0]);
                }
            }
        }
    }
}
