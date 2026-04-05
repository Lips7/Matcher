//! Delete engine backed by a Unicode bitset.
//!
//! [`DeleteMatcher`] stores a flat bitset covering all Unicode codepoints
//! (0x0 through 0x10FFFF, ~139 KB) plus a 16-byte `ascii_lut` cache of the
//! first 128 bits for fast ASCII probing.
//!
//! The [`DeleteMatcher::delete`] method scans the input in two phases:
//!
//! 1. **Seek phase** -- Finds the first deletable codepoint in the text,
//!    using [`skip_ascii_non_delete_simd`] to quickly advance past long runs
//!    of non-deletable ASCII bytes.
//! 2. **Copy-skip phase** -- After the first hit, continues scanning and
//!    copies non-deleted spans into a pooled output `String`, skipping
//!    deleted codepoints.
//!
//! Returns `None` when no codepoint was deleted, allowing callers to keep
//! borrowing the original `&str` without allocation.

use std::borrow::Cow;

use crate::process::string_pool::get_string_from_pool;
use crate::process::transform::simd::skip_ascii_non_delete_simd;
use crate::process::transform::utf8::decode_utf8_raw;

/// Bitset-backed matcher for the delete transform.
///
/// The bitset covers all Unicode scalar values (0x0 through 0x10FFFF). Bit
/// `cp % 8` of byte `cp / 8` is set when codepoint `cp` should be deleted.
///
/// For ASCII bytes (0x00–0x7F), the first 16 bytes of the bitset are cached
/// in `ascii_lut` so that the SIMD skip helpers can probe the delete set
/// without touching the full bitset.
#[derive(Clone)]
pub(crate) struct DeleteMatcher {
    bitset: Cow<'static, [u8]>,
    /// First 16 bytes of `bitset`, covering ASCII codepoints 0x00–0x7F.
    /// Passed to [`skip_ascii_non_delete_simd`] for fast SIMD probing.
    ascii_lut: [u8; 16],
}

impl DeleteMatcher {
    /// Removes every configured codepoint from `text`.
    ///
    /// Returns `Some((result, is_ascii))` where `result` is the text with all deletable
    /// codepoints stripped, and `is_ascii` indicates whether the result is pure ASCII,
    /// tracked incrementally (if no non-ASCII char was kept, `is_ascii` is `true`).
    /// Returns `None` when nothing was deleted, allowing callers to keep borrowing the
    /// original `&str`.
    ///
    /// # Algorithm
    ///
    /// 1. **Seek phase**: Scans forward, using [`skip_ascii_non_delete_simd`]
    ///    for fast ASCII skipping, until the first deletable codepoint is found.
    ///    Returns `None` immediately if the entire text is clean.
    /// 2. **Build phase**: Allocates a pooled `String`, copies the clean prefix,
    ///    skips the first deleted codepoint, then enters the copy-skip loop.
    /// 3. **Copy-skip loop**: Tracks a `gap_start` cursor. Non-deleted bytes
    ///    advance `offset`; deleted codepoints flush `text[gap_start..offset]`
    ///    to the result, skip the deleted bytes, and reset `gap_start`.
    ///
    /// # Safety (internal)
    ///
    /// Uses `get_unchecked` to read the current byte at `offset` after the
    /// `offset < len` / `offset >= len` guards have confirmed it is in bounds.
    /// The byte value is then used to branch into the ASCII or multi-byte path.
    pub(crate) fn delete(&self, text: &str) -> Option<(String, bool)> {
        let bytes = text.as_bytes();
        let len = bytes.len();
        let mut offset = 0usize;
        // Continuation bytes kept in output — accumulated during seek and copy-skip.
        // For pure-ASCII input, the non-ASCII branch never executes, so cont_kept stays
        // 0 and output_density is correctly 0.0 without any special-casing.
        let mut cont_kept: usize = 0;

        loop {
            if offset >= len {
                return None;
            }
            // SAFETY: `offset < len` is checked by the guard above.
            let byte = unsafe { *bytes.get_unchecked(offset) };
            if byte < 0x80 {
                if (self.ascii_lut[(byte as usize) >> 3] & (1 << (byte & 7))) != 0 {
                    break;
                }
                offset += 1;
                offset = skip_ascii_non_delete_simd(bytes, offset, &self.ascii_lut);
            } else {
                // SAFETY: `byte >= 0x80` means non-ASCII in a valid UTF-8 `&str`.
                let (cp, char_len) = unsafe { decode_utf8_raw(bytes, offset) };
                let cp = cp as usize;
                if cp / 8 < self.bitset.len() && (self.bitset[cp / 8] & (1 << (cp % 8))) != 0 {
                    break;
                }
                cont_kept += char_len - 1; // kept non-ASCII char
                offset += char_len;
            }
        }

        let mut result = get_string_from_pool(text.len());
        result.push_str(&text[..offset]);

        // SAFETY: The seek loop above broke on a match at `offset`, so `offset < len` still holds.
        let byte = unsafe { *bytes.get_unchecked(offset) };
        if byte < 0x80 {
            offset += 1;
        } else {
            // SAFETY: `byte >= 0x80` means non-ASCII in a valid UTF-8 `&str`.
            let (_, char_len) = unsafe { decode_utf8_raw(bytes, offset) };
            offset += char_len;
            // Deleted non-ASCII char: does not contribute to cont_kept.
        }

        let mut gap_start = offset;
        while offset < len {
            // SAFETY: `offset < len` is checked by the while condition.
            let byte = unsafe { *bytes.get_unchecked(offset) };
            if byte < 0x80 {
                if (self.ascii_lut[(byte as usize) >> 3] & (1 << (byte & 7))) != 0 {
                    result.push_str(&text[gap_start..offset]);
                    offset += 1;
                    gap_start = offset;
                } else {
                    offset += 1;
                    offset = skip_ascii_non_delete_simd(bytes, offset, &self.ascii_lut);
                }
            } else {
                // SAFETY: `byte >= 0x80` means non-ASCII in a valid UTF-8 `&str`.
                let (cp, char_len) = unsafe { decode_utf8_raw(bytes, offset) };
                let cp = cp as usize;
                if cp / 8 < self.bitset.len() && (self.bitset[cp / 8] & (1 << (cp % 8))) != 0 {
                    result.push_str(&text[gap_start..offset]);
                    offset += char_len;
                    gap_start = offset;
                    // Deleted non-ASCII char: does not contribute to cont_kept.
                } else {
                    cont_kept += char_len - 1; // kept non-ASCII char
                    offset += char_len;
                }
            }
        }

        result.push_str(&text[gap_start..]);
        // `cont_kept == 0` means no multi-byte chars were kept → result is pure ASCII.
        Some((result, cont_kept == 0))
    }

    /// Returns a byte iterator that yields only non-deleted bytes from `text`.
    ///
    /// Used by the fused delete-scan path to stream bytes directly into the AC
    /// automaton without materializing an intermediate `String`. The iterator
    /// decides keep/skip at the codepoint level, then yields all bytes of kept
    /// characters one at a time.
    ///
    /// Multi-byte UTF-8 characters are decoded once to check the bitset, then
    /// their individual bytes are yielded via the `char_remaining` counter.
    #[inline(always)]
    pub(crate) fn filter_bytes<'a>(&'a self, text: &'a str) -> DeleteFilterIterator<'a> {
        DeleteFilterIterator {
            bytes: text.as_bytes(),
            offset: 0,
            char_remaining: 0,
            ascii_lut: &self.ascii_lut,
            bitset: &self.bitset,
        }
    }

    pub(crate) fn new(bitset: &'static [u8]) -> Self {
        let mut ascii_lut = [0u8; 16];
        let copy_len = bitset.len().min(16);
        ascii_lut[..copy_len].copy_from_slice(&bitset[..copy_len]);
        Self {
            bitset: Cow::Borrowed(bitset),
            ascii_lut,
        }
    }
}

/// Streaming byte iterator that yields non-deleted bytes from a UTF-8 string.
///
/// Created by [`DeleteMatcher::filter_bytes`]. Walks the source bytes, checking
/// each codepoint against the delete bitset. Kept codepoints have their bytes
/// yielded one at a time; deleted codepoints are silently skipped.
///
/// The output byte stream is valid UTF-8 (a subsequence of complete codepoints
/// from the input), which satisfies the safety requirement of `daachorse`'s
/// `find_overlapping_iter_from_iter`.
pub(crate) struct DeleteFilterIterator<'a> {
    bytes: &'a [u8],
    offset: usize,
    /// Remaining bytes to yield from the current kept multi-byte character.
    /// 0 when at a codepoint boundary (need to decode next codepoint).
    char_remaining: u8,
    ascii_lut: &'a [u8; 16],
    bitset: &'a [u8],
}

impl Iterator for DeleteFilterIterator<'_> {
    type Item = u8;

    #[inline(always)]
    fn next(&mut self) -> Option<u8> {
        // Fast path: mid-character continuation bytes (no decode needed).
        if self.char_remaining > 0 {
            // SAFETY: we're within a kept multi-byte character; offset is in bounds.
            let byte = unsafe { *self.bytes.get_unchecked(self.offset) };
            self.offset += 1;
            self.char_remaining -= 1;
            return Some(byte);
        }

        loop {
            if self.offset >= self.bytes.len() {
                return None;
            }
            // SAFETY: offset < len checked above.
            let byte = unsafe { *self.bytes.get_unchecked(self.offset) };
            if byte < 0x80 {
                // ASCII: check delete LUT inline.
                if (self.ascii_lut[(byte as usize) >> 3] & (1 << (byte & 7))) != 0 {
                    self.offset += 1;
                    continue;
                }
                self.offset += 1;
                return Some(byte);
            }
            // Non-ASCII: decode codepoint, check bitset.
            // SAFETY: byte >= 0x80 in a valid UTF-8 &str means multi-byte lead byte.
            let (cp, char_len) = unsafe { decode_utf8_raw(self.bytes, self.offset) };
            let cp = cp as usize;
            if cp / 8 < self.bitset.len() && (self.bitset[cp / 8] & (1 << (cp % 8))) != 0 {
                self.offset += char_len;
                continue;
            }
            // Kept: yield first byte, set remaining for continuation bytes.
            let first_byte = byte;
            self.offset += 1;
            self.char_remaining = (char_len - 1) as u8;
            return Some(first_byte);
        }
    }
}
