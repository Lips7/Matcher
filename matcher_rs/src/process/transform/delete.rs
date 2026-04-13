//! Delete engine backed by a Unicode bitset.
//!
//! [`DeleteMatcher`] stores a flat bitset covering all Unicode codepoints
//! (0x0 through 0x10FFFF, ~139 KB) plus a 16-byte `ascii_lut` cache of the
//! first 128 bits for fast ASCII probing.
//!
//! The [`DeleteMatcher::delete`] method scans the input in two phases:
//!
//! 1. **Seek phase** -- Finds the first deletable codepoint in the text, using
//!    [`skip_ascii_non_delete_simd`] to quickly advance past long runs of
//!    non-deletable ASCII bytes.
//! 2. **Copy-skip phase** -- After the first hit, continues scanning and copies
//!    non-deleted spans into a pooled output `String`, skipping deleted
//!    codepoints.
//!
//! Returns `None` when no codepoint was deleted, allowing callers to keep
//! borrowing the original `&str` without allocation.

use std::borrow::Cow;

use crate::process::transform::{
    filter::{CodepointFilter, FilterAction, FilterIterator},
    simd::skip_ascii_non_delete_simd,
    utf8::decode_utf8_raw,
};

/// Bitset-backed matcher for the delete transform.
///
/// The bitset covers all Unicode scalar values (0x0 through 0x10FFFF). Bit
/// `cp % 8` of byte `cp / 8` is set when codepoint `cp` should be deleted.
///
/// For ASCII bytes (0x00–0x7F), the first 16 bytes of the bitset are cached
/// in `ascii_lut` so that the SIMD skip helpers can probe the delete set
/// without touching the full bitset.
///
/// # Performance
///
/// - **O(1) per codepoint** via flat bitset lookup.
/// - **SIMD ASCII skip**: [`skip_ascii_non_delete_simd`] advances past runs of
///   non-deletable ASCII bytes in bulk (16–32 bytes per iteration).
/// - **Two-phase scan**: first seeks to a deletable byte, then copies
///   non-deleted spans in bulk — zero allocation when no deletions are found.
#[derive(Clone)]
pub(crate) struct DeleteMatcher {
    bitset: Cow<'static, [u8]>,
    /// First 16 bytes of `bitset`, covering ASCII codepoints 0x00–0x7F.
    /// Passed to [`skip_ascii_non_delete_simd`] for fast SIMD probing.
    ascii_lut: [u8; 16],
}

impl DeleteMatcher {
    /// Scans `bytes` for the first deletable codepoint starting at `offset`.
    ///
    /// Returns the byte offset of the first deletable codepoint, or `len` if
    /// none found. Uses SIMD-accelerated ASCII skip for bulk scanning.
    #[inline(always)]
    fn seek_first_deletable(&self, bytes: &[u8], mut offset: usize) -> usize {
        let len = bytes.len();
        loop {
            if offset >= len {
                return len;
            }
            // SAFETY: offset < len per guard above.
            unsafe { core::hint::assert_unchecked(offset < len) };
            let byte = bytes[offset];
            if byte < 0x80 {
                if (self.ascii_lut[(byte as usize) >> 3] & (1 << (byte & 7))) != 0 {
                    return offset;
                }
                offset += 1;
                offset = skip_ascii_non_delete_simd(bytes, offset, &self.ascii_lut);
            } else {
                // SAFETY: `byte >= 0x80` in a valid UTF-8 `&str`; offset in bounds per guard.
                let (cp, char_len) = unsafe { decode_utf8_raw(bytes, offset) };
                let cp = cp as usize;
                // SAFETY: Valid UTF-8 codepoints ≤ 0x10FFFF; bitset covers 0x0–0x10FFFF.
                unsafe { core::hint::assert_unchecked(cp / 8 < self.bitset.len()) };
                if (self.bitset[cp / 8] & (1 << (cp % 8))) != 0 {
                    return offset;
                }
                offset += char_len;
            }
        }
    }

    /// Returns `true` if `text` contains any deletable codepoint.
    ///
    /// Runs only the seek phase (SIMD-accelerated ASCII skip), never
    /// allocates. Use as a cheap probe before deciding whether to scan
    /// the delete-transformed text.
    #[inline(always)]
    pub(crate) fn has_deletable(&self, text: &str) -> bool {
        self.seek_first_deletable(text.as_bytes(), 0) < text.len()
    }

    /// Removes every configured codepoint from `text`.
    ///
    /// Returns `Some(result)` where `result` is the text with all deletable
    /// codepoints stripped. Returns `None` when nothing was deleted, allowing
    /// callers to keep borrowing the original `&str`.
    ///
    /// # Algorithm
    ///
    /// 1. **Seek phase**: Scans forward via
    ///    [`seek_first_deletable`](Self::seek_first_deletable), using SIMD for
    ///    fast ASCII skipping, until the first deletable codepoint is found.
    ///    Returns `None` immediately if the entire text is clean.
    /// 2. **Build phase**: Allocates a pooled `String`, copies the clean
    ///    prefix, skips the first deleted codepoint, then enters the copy-skip
    ///    loop.
    /// 3. **Copy-skip loop**: Tracks a `gap_start` cursor. Non-deleted bytes
    ///    advance `offset`; deleted codepoints flush `text[gap_start..offset]`
    ///    to the result, skip the deleted bytes, and reset `gap_start`.
    ///
    /// ```ignore
    /// let matcher = DeleteMatcher::new(DELETE_BITSET_BYTES);
    /// let result = matcher.delete("hello, world!").unwrap();
    /// assert_eq!(result, "hello world"); // commas and exclamation deleted
    /// assert!(matcher.delete("helloworld").is_none()); // nothing to delete
    /// ```
    ///
    /// # Safety (internal)
    ///
    /// Uses `get_unchecked` to read the current byte at `offset` after the
    /// `offset < len` / `offset >= len` guards have confirmed it is in bounds.
    /// The byte value is then used to branch into the ASCII or multi-byte path.
    pub(crate) fn delete(&self, text: &str) -> Option<String> {
        let bytes = text.as_bytes();
        let len = bytes.len();
        let mut offset = self.seek_first_deletable(bytes, 0);
        if offset >= len {
            return None;
        }

        let mut result = String::with_capacity(text.len());
        result.push_str(&text[..offset]);

        // SAFETY: seek_first_deletable returned offset < len.
        unsafe { core::hint::assert_unchecked(offset < len) };
        let byte = bytes[offset];
        if byte < 0x80 {
            offset += 1;
        } else {
            // SAFETY: `byte >= 0x80` means non-ASCII in a valid UTF-8 `&str`.
            let (_, char_len) = unsafe { decode_utf8_raw(bytes, offset) };
            offset += char_len;
        }

        let mut gap_start = offset;
        while offset < len {
            // SAFETY: The `while offset < len` guard ensures `offset < len`.
            unsafe { core::hint::assert_unchecked(offset < len) };
            let byte = bytes[offset];
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
                // SAFETY: same invariant as seek phase.
                unsafe { core::hint::assert_unchecked(cp / 8 < self.bitset.len()) };
                if (self.bitset[cp / 8] & (1 << (cp % 8))) != 0 {
                    result.push_str(&text[gap_start..offset]);
                    offset += char_len;
                    gap_start = offset;
                } else {
                    offset += char_len;
                }
            }
        }

        result.push_str(&text[gap_start..]);
        Some(result)
    }

    /// Returns a byte iterator that yields only non-deleted bytes from `text`.
    ///
    /// Used by the fused delete-scan path to stream bytes directly into the AC
    /// automaton without materializing an intermediate `String`. The iterator
    /// decides keep/skip at the codepoint level, then yields all bytes of kept
    /// characters one at a time.
    #[inline(always)]
    pub(crate) fn filter_bytes<'a>(
        &'a self,
        text: &'a str,
    ) -> FilterIterator<'a, DeleteFilter<'a>> {
        FilterIterator::new(
            text,
            DeleteFilter {
                ascii_lut: &self.ascii_lut,
                bitset: &self.bitset,
            },
        )
    }

    /// Constructs a [`DeleteMatcher`] from a static bitset.
    ///
    /// Copies the first 16 bytes of `bitset` into the `ascii_lut` for fast
    /// SIMD-accelerated ASCII probing.
    pub(crate) fn new(bitset: &'static [u8]) -> Self {
        debug_assert!(
            bitset.len() >= 0x110000 / 8,
            "delete bitset must cover all Unicode codepoints"
        );
        let mut ascii_lut = [0u8; 16];
        let copy_len = bitset.len().min(16);
        ascii_lut[..copy_len].copy_from_slice(&bitset[..copy_len]);
        Self {
            bitset: Cow::Borrowed(bitset),
            ascii_lut,
        }
    }
}

/// [`CodepointFilter`] implementation for the delete transform.
///
/// Checks `ascii_lut` for ASCII bytes and the full `bitset` for non-ASCII
/// codepoints. Used by [`DeleteMatcher::filter_bytes`] to produce a
/// streaming byte iterator.
pub(crate) struct DeleteFilter<'a> {
    ascii_lut: &'a [u8; 16],
    bitset: &'a [u8],
}

impl<'a> CodepointFilter<'a> for DeleteFilter<'a> {
    #[inline(always)]
    fn filter_ascii(&self, byte: u8) -> FilterAction<'a> {
        if (self.ascii_lut[(byte as usize) >> 3] & (1 << (byte & 7))) != 0 {
            FilterAction::Delete
        } else {
            FilterAction::Keep
        }
    }

    #[inline(always)]
    fn filter_codepoint(&self, cp: u32) -> FilterAction<'a> {
        let cp = cp as usize;
        // SAFETY: `cp` comes from `decode_utf8_raw` on valid UTF-8, so cp ≤ 0x10FFFF.
        // Bitset covers the full Unicode range (139,264 bytes).
        unsafe { core::hint::assert_unchecked(cp / 8 < self.bitset.len()) };
        if (self.bitset[cp / 8] & (1 << (cp % 8))) != 0 {
            FilterAction::Delete
        } else {
            FilterAction::Keep
        }
    }
}
