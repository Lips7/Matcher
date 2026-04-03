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

#[cfg(feature = "runtime_build")]
use ahash::AHashSet;
use std::borrow::Cow;

use crate::process::string_pool::get_string_from_pool;
use crate::process::transform::simd::skip_ascii_non_delete_simd;
use crate::process::transform::utf8::decode_utf8_raw;

/// Number of bytes needed to represent all Unicode codepoints (0x0–0x10FFFF) in
/// a flat bitset: one bit per codepoint, packed 8 per byte.
#[cfg(feature = "runtime_build")]
const UNICODE_BITSET_SIZE: usize = 0x110000 / 8;

/// Byte-by-byte iterator over delete-transformed text.
///
/// Yields the UTF-8 bytes of `text` with all deletable codepoints removed.
/// Uses the same bitset + ASCII LUT as [`DeleteMatcher::delete`], but yields
/// bytes one at a time instead of building a `String`.
///
/// For multi-byte non-deleted codepoints, the first byte is returned directly
/// from `next()` and the remaining continuation bytes are buffered in a small
/// stack array.
pub(crate) struct DeleteByteIter<'a> {
    source: &'a [u8],
    bitset: &'a [u8],
    ascii_lut: &'a [u8; 16],
    pos: usize,
    /// Continuation bytes of a non-deleted multi-byte char being yielded.
    buf: [u8; 3],
    buf_pos: u8,
    buf_len: u8,
}

impl<'a> Iterator for DeleteByteIter<'a> {
    type Item = u8;

    #[inline(always)]
    fn next(&mut self) -> Option<u8> {
        // Drain continuation buffer for multi-byte chars
        if self.buf_pos < self.buf_len {
            let b = self.buf[self.buf_pos as usize];
            self.buf_pos += 1;
            return Some(b);
        }

        loop {
            if self.pos >= self.source.len() {
                return None;
            }

            // SAFETY: pos < source.len() checked above.
            let b = unsafe { *self.source.get_unchecked(self.pos) };

            if b < 0x80 {
                // ASCII fast path
                self.pos += 1;
                if (self.ascii_lut[(b as usize) >> 3] & (1 << (b & 7))) != 0 {
                    continue; // deleted
                }
                return Some(b);
            }

            // Multi-byte: decode codepoint and check bitset
            // SAFETY: `b >= 0x80` means non-ASCII in a valid UTF-8 `&str`, so `pos` is a
            // valid multi-byte lead byte.
            let (cp, char_len) = unsafe { decode_utf8_raw(self.source, self.pos) };
            let cp_usize = cp as usize;
            if cp_usize / 8 < self.bitset.len()
                && (self.bitset[cp_usize / 8] & (1 << (cp_usize % 8))) != 0
            {
                // Deleted codepoint — skip all its bytes
                self.pos += char_len;
                continue;
            }

            // Non-deleted multi-byte: yield first byte, buffer rest
            let first = b;
            self.pos += 1;
            let rest = char_len - 1;
            for i in 0..rest {
                // SAFETY: valid UTF-8 guarantees continuation bytes exist.
                self.buf[i] = unsafe { *self.source.get_unchecked(self.pos) };
                self.pos += 1;
            }
            self.buf_pos = 0;
            self.buf_len = rest as u8;
            return Some(first);
        }
    }
}

/// Bitset-backed matcher for the delete transform.
///
/// The bitset covers all Unicode scalar values (0x0 through 0x10FFFF). Bit
/// `cp % 8` of byte `cp / 8` is set when codepoint `cp` should be deleted.
///
/// For ASCII bytes (0x00–0x7F), the first 16 bytes of the bitset are cached
/// in `ascii_lut` so that the SIMD skip helpers can probe the delete set
/// without touching the full bitset.
///
/// Construction is feature-gated:
/// - **Default**: `DeleteMatcher` borrows the pre-compiled bitset from
///   `constants::DELETE_BITSET_BYTES`.
/// - **`runtime_build`**: `DeleteMatcher::from_sources` builds the bitset
///   from the source delete table.
#[derive(Clone)]
pub(crate) struct DeleteMatcher {
    /// Full Unicode bitset (one bit per codepoint). Borrowed from a `&'static`
    /// constant in the default build; owned when using `runtime_build`.
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

    /// Returns a byte-by-byte iterator over delete-transformed text.
    ///
    /// Equivalent output to `delete()` followed by iterating the result's
    /// bytes, but without allocating an intermediate `String`.
    #[inline(always)]
    pub(crate) fn byte_iter<'a>(&'a self, text: &'a str) -> DeleteByteIter<'a> {
        DeleteByteIter {
            source: text.as_bytes(),
            bitset: &self.bitset,
            ascii_lut: &self.ascii_lut,
            pos: 0,
            buf: [0; 3],
            buf_pos: 0,
            buf_len: 0,
        }
    }

    /// Builds a matcher from the precompiled delete bitset.
    ///
    /// `bitset` is the raw byte slice from `constants::DELETE_BITSET_BYTES`,
    /// embedded at compile time by `build.rs`. The first 16 bytes are copied
    /// into `ascii_lut` for SIMD-friendly ASCII probing.
    #[cfg(not(feature = "runtime_build"))]
    pub(crate) fn new(bitset: &'static [u8]) -> Self {
        let mut ascii_lut = [0u8; 16];
        let copy_len = bitset.len().min(16);
        ascii_lut[..copy_len].copy_from_slice(&bitset[..copy_len]);
        Self {
            bitset: Cow::Borrowed(bitset),
            ascii_lut,
        }
    }

    /// Builds a matcher from the raw delete-source file.
    ///
    /// Parses `text_delete` (`U+XXXX` codepoint tokens, one per line, from
    /// `TEXT-DELETE.txt`), collecting every unique codepoint into a `HashSet`
    /// and setting the corresponding bits in a freshly allocated
    /// [`UNICODE_BITSET_SIZE`]-byte bitset.
    #[cfg(feature = "runtime_build")]
    pub(crate) fn from_sources(text_delete: &str) -> Self {
        let mut bitset = vec![0u8; UNICODE_BITSET_SIZE];
        let mut codepoints = AHashSet::new();
        for token in text_delete.trim().lines() {
            codepoints.insert(parse_delete_codepoint(token));
        }
        for cp in codepoints {
            let cp = cp as usize;
            bitset[cp / 8] |= 1 << (cp % 8);
        }
        let mut ascii_lut = [0u8; 16];
        ascii_lut.copy_from_slice(&bitset[..16]);
        Self {
            bitset: Cow::Owned(bitset),
            ascii_lut,
        }
    }
}

#[cfg(feature = "runtime_build")]
fn parse_delete_codepoint(token: &str) -> u32 {
    u32::from_str_radix(
        token
            .strip_prefix("U+")
            .expect("TEXT-DELETE entries must use U+XXXX format"),
        16,
    )
    .expect("TEXT-DELETE entry must contain a valid hexadecimal codepoint")
}

#[cfg(all(test, not(feature = "runtime_build")))]
mod tests {
    use super::*;

    use super::super::constants;

    fn delete_matcher() -> DeleteMatcher {
        DeleteMatcher::new(constants::DELETE_BITSET_BYTES)
    }

    fn assert_byte_iter_eq_delete(matcher: &DeleteMatcher, text: &str) {
        let materialized: Vec<u8> = match matcher.delete(text) {
            Some((s, _)) => s.into_bytes(),
            None => text.as_bytes().to_vec(),
        };
        let streamed: Vec<u8> = matcher.byte_iter(text).collect();
        assert_eq!(materialized, streamed, "delete mismatch for: {:?}", text);
    }

    #[test]
    fn delete_byte_iter_matches_delete() {
        let m = delete_matcher();
        for text in ["", "hello", "hello world", "a b c", "\t\n", "中 文"] {
            assert_byte_iter_eq_delete(&m, text);
        }
    }

    proptest::proptest! {
        #![proptest_config(proptest::prelude::ProptestConfig::with_cases(500))]

        #[test]
        fn prop_delete_byte_iter(text in "\\PC{0,200}") {
            let m = delete_matcher();
            assert_byte_iter_eq_delete(&m, &text);
        }
    }
}
