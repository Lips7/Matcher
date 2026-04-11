//! Shared UTF-8 decoding helper for the transform engines.

/// Decodes one non-ASCII UTF-8 codepoint from `bytes[offset..]`.
///
/// This function handles only multi-byte sequences (lead byte `>= 0xC0`); it
/// must not be called on ASCII bytes (`< 0x80`). All callers must first
/// advance past ASCII bytes (e.g., via [`super::simd::skip_ascii_simd`])
/// before invoking this function.
///
/// # Returns
///
/// `(codepoint, byte_length)` where:
/// - `codepoint` is the decoded Unicode scalar value.
/// - `byte_length` is the number of bytes consumed: 2 for U+0080–U+07FF, 3 for
///   U+0800–U+FFFF (includes all CJK Unified Ideographs), 4 for
///   U+10000–U+10FFFF (supplementary planes).
///
/// ```text
/// // Decoding '中' (U+4E2D, 3-byte UTF-8: E4 B8 AD):
/// let text = "abc中def";
/// let bytes = text.as_bytes();
/// let (cp, len) = unsafe { decode_utf8_raw(bytes, 3) };
/// assert_eq!(cp, 0x4E2D);  // '中'
/// assert_eq!(len, 3);       // 3-byte sequence
/// ```
///
/// # Safety
///
/// - `offset` must point at a valid UTF-8 continuation-sequence start (lead
///   byte `>= 0xC0`). Callers guarantee this by only invoking after confirming
///   `bytes[offset] >= 0x80`, inside a `&str` (which is always valid UTF-8).
/// - `bytes[offset .. offset + byte_length]` must be in bounds. This is
///   guaranteed because the input originates from a `&str` whose total length
///   covers the full multi-byte sequence.
/// - Each `get_unchecked` reads a continuation byte at a known offset (1, 2, or
///   3 past the lead byte). The lead byte's high bits determine how many
///   continuation bytes exist, and valid UTF-8 guarantees they are present.
#[inline(always)]
pub(crate) unsafe fn decode_utf8_raw(bytes: &[u8], offset: usize) -> (u32, usize) {
    // SAFETY: Caller guarantees `offset` points at a valid UTF-8 lead byte within
    // `bytes`.
    unsafe { core::hint::assert_unchecked(offset < bytes.len()) };
    let b0 = bytes[offset];
    if b0 < 0xE0 {
        // SAFETY: 2-byte sequence; valid UTF-8 guarantees continuation byte exists.
        unsafe { core::hint::assert_unchecked(offset + 1 < bytes.len()) };
        let b1 = bytes[offset + 1];
        (((b0 as u32 & 0x1F) << 6) | (b1 as u32 & 0x3F), 2)
    } else if b0 < 0xF0 {
        // SAFETY: 3-byte sequence; valid UTF-8 guarantees 2 continuation bytes exist.
        unsafe { core::hint::assert_unchecked(offset + 2 < bytes.len()) };
        let b1 = bytes[offset + 1];
        let b2 = bytes[offset + 2];
        (
            ((b0 as u32 & 0x0F) << 12) | ((b1 as u32 & 0x3F) << 6) | (b2 as u32 & 0x3F),
            3,
        )
    } else {
        // SAFETY: 4-byte sequence; valid UTF-8 guarantees 3 continuation bytes exist.
        unsafe { core::hint::assert_unchecked(offset + 3 < bytes.len()) };
        let b1 = bytes[offset + 1];
        let b2 = bytes[offset + 2];
        let b3 = bytes[offset + 3];
        (
            ((b0 as u32 & 0x07) << 18)
                | ((b1 as u32 & 0x3F) << 12)
                | ((b2 as u32 & 0x3F) << 6)
                | (b3 as u32 & 0x3F),
            4,
        )
    }
}
