use std::simd::{
    Simd,
    cmp::{SimdPartialEq, SimdPartialOrd},
};

/// Advances `offset` past all ASCII bytes (< 0x80), using 16-byte SIMD chunks.
/// Returns the new offset pointing at the first non-ASCII byte or end of slice.
#[inline(always)]
pub fn skip_ascii_simd(bytes: &[u8], offset: usize) -> usize {
    // Fast path: already at end or at a non-ASCII byte — nothing to skip.
    if offset >= bytes.len() || bytes[offset] >= 0x80 {
        return offset;
    }
    let mut offset = offset;
    const LANES: usize = 16;
    let threshold = Simd::<u8, LANES>::splat(0x80u8);
    while offset + LANES <= bytes.len() {
        let chunk = Simd::<u8, LANES>::from_slice(&bytes[offset..]);
        let mask = chunk.simd_ge(threshold).to_bitmask();
        if mask != 0 {
            offset += mask.trailing_zeros() as usize;
            return offset;
        }
        offset += LANES;
    }
    // Scalar tail for < 16 remaining bytes.
    while offset < bytes.len() && bytes[offset] < 0x80 {
        offset += 1;
    }
    offset
}

/// Returns a bitmask of deletable ASCII bytes in `chunk` using the 16-byte `ascii_lut`.
///
/// Bit `i` is set if `chunk[i]` is a deletable character per `ascii_lut`.
/// Caller must ensure all bytes in `chunk` are ASCII (< 0x80).
///
/// Two `swizzle_dyn` calls (compiling to `pshufb`/`tbl`) perform the parallel
/// 16-way LUT lookup without any scalar branching.
#[inline(always)]
pub fn simd_ascii_delete_mask(chunk: Simd<u8, 16>, ascii_lut: Simd<u8, 16>) -> u64 {
    // byte_idx = b >> 3  (which byte of ascii_lut, range 0..15)
    let byte_idx = chunk >> Simd::<u8, 16>::splat(3);
    let lut_byte = ascii_lut.swizzle_dyn(byte_idx);

    // bit_pos = b & 7  (which bit within that byte, range 0..7)
    // shift_table[i] = 1 << i for i in 0..8, repeated to fill 16 lanes.
    const SHIFT_TABLE: [u8; 16] = [1, 2, 4, 8, 16, 32, 64, 128, 1, 2, 4, 8, 16, 32, 64, 128];
    let shift_table = Simd::<u8, 16>::from_array(SHIFT_TABLE);
    let bit_pos = chunk & Simd::<u8, 16>::splat(7);
    let bit_mask = shift_table.swizzle_dyn(bit_pos);

    (lut_byte & bit_mask)
        .simd_ne(Simd::<u8, 16>::splat(0))
        .to_bitmask()
}

/// Advances `offset` past non-digit ASCII bytes, using 16-byte SIMD chunks.
/// Stops at bytes >= 0x80 (non-ASCII) or 0x30–0x39 (ASCII digits).
#[inline(always)]
pub fn skip_non_digit_ascii_simd(bytes: &[u8], offset: usize) -> usize {
    // Fast path: already at a stop byte (non-ASCII or digit) — nothing to skip.
    if offset >= bytes.len() {
        return offset;
    }
    let b0 = bytes[offset];
    if b0 >= 0x80 || (0x30..=0x39).contains(&b0) {
        return offset;
    }
    let mut offset = offset;
    const LANES: usize = 16;
    let non_ascii = Simd::<u8, LANES>::splat(0x80u8);
    let digit_lo = Simd::<u8, LANES>::splat(0x30u8);
    let digit_hi = Simd::<u8, LANES>::splat(0x3Au8); // exclusive ('9' + 1)

    while offset + LANES <= bytes.len() {
        let chunk = Simd::<u8, LANES>::from_slice(&bytes[offset..]);
        let is_non_ascii = chunk.simd_ge(non_ascii);
        let is_digit = chunk.simd_ge(digit_lo) & chunk.simd_lt(digit_hi);
        let stop_mask = (is_non_ascii | is_digit).to_bitmask();
        if stop_mask != 0 {
            offset += stop_mask.trailing_zeros() as usize;
            return offset;
        }
        offset += LANES;
    }
    // Scalar tail for < 16 remaining bytes.
    while offset < bytes.len() {
        let b = bytes[offset];
        if b >= 0x80 || (0x30..=0x39).contains(&b) {
            break;
        }
        offset += 1;
    }
    offset
}
