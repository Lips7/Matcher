//! Delete engine backed by a Unicode bitset.

use std::borrow::Cow;
#[cfg(feature = "runtime_build")]
use std::collections::HashSet;

use crate::process::transform::simd::skip_ascii_non_delete_simd;
use crate::process::variant::get_string_from_pool;

#[cfg(feature = "runtime_build")]
const UNICODE_BITSET_SIZE: usize = 0x110000 / 8;

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

#[derive(Clone)]
pub(crate) struct DeleteMatcher {
    bitset: Cow<'static, [u8]>,
    ascii_lut: [u8; 16],
}

impl DeleteMatcher {
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
