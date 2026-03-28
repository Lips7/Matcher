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
#[cfg(feature = "runtime_build")]
use std::collections::HashSet;

use crate::process::transform::simd::skip_ascii_non_delete_simd;
use crate::process::variant::get_string_from_pool;

/// Number of bytes needed to represent all Unicode codepoints (0x0–0x10FFFF) in
/// a flat bitset: one bit per codepoint, packed 8 per byte.
#[cfg(feature = "runtime_build")]
const UNICODE_BITSET_SIZE: usize = 0x110000 / 8;

/// Decodes one non-ASCII UTF-8 codepoint from `bytes[offset..]`.
///
/// Returns `(codepoint, byte_length)` where `byte_length` is 2, 3, or 4.
/// Functionally identical to `charwise::decode_utf8_raw` but kept as a
/// separate copy to avoid cross-module coupling in this hot path.
///
/// # Safety (internal)
///
/// Uses `get_unchecked` to read continuation bytes without bounds checks.
/// This is safe because:
/// - `offset` always points at a non-ASCII lead byte (`>= 0x80`) inside a
///   valid `&str`, guaranteeing the continuation bytes exist within the slice.
/// - The lead byte's high bits determine the sequence length, and valid UTF-8
///   guarantees that many continuation bytes follow.
#[inline(always)]
fn decode_utf8(bytes: &[u8], offset: usize) -> (u32, usize) {
    let b0 = unsafe { *bytes.get_unchecked(offset) };
    if b0 < 0xE0 {
        let b1 = unsafe { *bytes.get_unchecked(offset + 1) };
        (((b0 as u32 & 0x1F) << 6) | (b1 as u32 & 0x3F), 2)
    } else if b0 < 0xF0 {
        let b1 = unsafe { *bytes.get_unchecked(offset + 1) };
        let b2 = unsafe { *bytes.get_unchecked(offset + 2) };
        (
            ((b0 as u32 & 0x0F) << 12) | ((b1 as u32 & 0x3F) << 6) | (b2 as u32 & 0x3F),
            3,
        )
    } else {
        let b1 = unsafe { *bytes.get_unchecked(offset + 1) };
        let b2 = unsafe { *bytes.get_unchecked(offset + 2) };
        let b3 = unsafe { *bytes.get_unchecked(offset + 3) };
        (
            ((b0 as u32 & 0x07) << 18)
                | ((b1 as u32 & 0x3F) << 12)
                | ((b2 as u32 & 0x3F) << 6)
                | (b3 as u32 & 0x3F),
            4,
        )
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
/// - **Default**: [`DeleteMatcher::new`] borrows the pre-compiled bitset from
///   `constants::DELETE_BITSET_BYTES`.
/// - **`runtime_build`**: `DeleteMatcher::from_sources` builds the bitset
///   from source text and whitespace lists.
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
    /// Returns `Some((result, is_ascii))` where `result` is the text with all
    /// deletable codepoints stripped, and `is_ascii` indicates whether the
    /// result is pure ASCII. Returns `None` when nothing was deleted, allowing
    /// callers to keep borrowing the original `&str`.
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

        loop {
            if offset >= len {
                return None;
            }
            let byte = unsafe { *bytes.get_unchecked(offset) };
            if byte < 0x80 {
                if (self.ascii_lut[(byte as usize) >> 3] & (1 << (byte & 7))) != 0 {
                    break;
                }
                offset += 1;
                offset = skip_ascii_non_delete_simd(bytes, offset, &self.ascii_lut);
            } else {
                let (cp, char_len) = decode_utf8(bytes, offset);
                let cp = cp as usize;
                if cp / 8 < self.bitset.len() && (self.bitset[cp / 8] & (1 << (cp % 8))) != 0 {
                    break;
                }
                offset += char_len;
            }
        }

        let mut result = get_string_from_pool(text.len());
        result.push_str(&text[..offset]);

        let byte = unsafe { *bytes.get_unchecked(offset) };
        if byte < 0x80 {
            offset += 1;
        } else {
            let (_, char_len) = decode_utf8(bytes, offset);
            offset += char_len;
        }

        let mut gap_start = offset;
        while offset < len {
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
                let (cp, char_len) = decode_utf8(bytes, offset);
                let cp = cp as usize;
                if cp / 8 < self.bitset.len() && (self.bitset[cp / 8] & (1 << (cp % 8))) != 0 {
                    result.push_str(&text[gap_start..offset]);
                    offset += char_len;
                    gap_start = offset;
                } else {
                    offset += char_len;
                }
            }
        }

        result.push_str(&text[gap_start..]);
        let is_ascii = result.is_ascii();
        Some((result, is_ascii))
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

    /// Builds a matcher from the raw delete-source files and whitespace list.
    ///
    /// Parses `text_delete` (one line of characters per entry, from
    /// `TEXT-DELETE.txt`) and `white_space` (individual whitespace strings from
    /// [`constants::WHITE_SPACE`]), collecting every unique `char` into a
    /// `HashSet` and setting the corresponding bits in a freshly allocated
    /// [`UNICODE_BITSET_SIZE`]-byte bitset.
    #[cfg(feature = "runtime_build")]
    pub(crate) fn from_sources(text_delete: &str, white_space: &[&str]) -> Self {
        let mut bitset = vec![0u8; UNICODE_BITSET_SIZE];
        let mut chars = HashSet::new();
        for line in text_delete.trim().lines() {
            chars.extend(line.chars());
        }
        for ws in white_space {
            chars.extend(ws.chars());
        }
        for c in chars {
            let cp = c as usize;
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
