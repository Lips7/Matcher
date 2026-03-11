use std::simd::{
    Simd,
    cmp::{SimdPartialEq, SimdPartialOrd},
};
use std::sync::OnceLock;

#[cfg(all(feature = "simd_runtime_dispatch", target_arch = "aarch64"))]
use std::arch::aarch64::*;
#[cfg(all(feature = "simd_runtime_dispatch", target_arch = "x86_64"))]
use std::arch::x86_64::*;

type SkipFn = fn(&[u8], usize) -> usize;
type SkipDeleteFn = fn(&[u8], usize, &[u8; 16], Simd<u8, 16>) -> usize;

const SHIFT_TABLE_16: [u8; 16] = [1, 2, 4, 8, 16, 32, 64, 128, 1, 2, 4, 8, 16, 32, 64, 128];
#[allow(dead_code)]
const SHIFT_TABLE_32: [u8; 32] = [
    1, 2, 4, 8, 16, 32, 64, 128, 1, 2, 4, 8, 16, 32, 64, 128, 1, 2, 4, 8, 16, 32, 64, 128, 1, 2, 4,
    8, 16, 32, 64, 128,
];

struct SimdDispatch {
    skip_ascii: SkipFn,
    skip_non_digit_ascii: SkipFn,
    skip_ascii_non_delete: SkipDeleteFn,
}

impl SimdDispatch {
    fn detect() -> Self {
        #[cfg(feature = "simd_runtime_dispatch")]
        {
            #[cfg(target_arch = "x86_64")]
            {
                if std::arch::is_x86_feature_detected!("avx2") {
                    return Self {
                        skip_ascii: skip_ascii_avx2,
                        skip_non_digit_ascii: skip_non_digit_ascii_avx2,
                        skip_ascii_non_delete: skip_ascii_non_delete_avx2,
                    };
                }

                return Self {
                    skip_ascii: skip_ascii_portable,
                    skip_non_digit_ascii: skip_non_digit_ascii_portable,
                    skip_ascii_non_delete: skip_ascii_non_delete_portable,
                };
            }

            #[cfg(target_arch = "aarch64")]
            {
                Self {
                    skip_ascii: skip_ascii_neon,
                    skip_non_digit_ascii: skip_non_digit_ascii_neon,
                    skip_ascii_non_delete: skip_ascii_non_delete_neon,
                }
            }

            #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
            {
                return Self {
                    skip_ascii: skip_ascii_portable,
                    skip_non_digit_ascii: skip_non_digit_ascii_portable,
                    skip_ascii_non_delete: skip_ascii_non_delete_portable,
                };
            }
        }

        #[cfg(not(feature = "simd_runtime_dispatch"))]
        Self {
            skip_ascii: skip_ascii_portable,
            skip_non_digit_ascii: skip_non_digit_ascii_portable,
            skip_ascii_non_delete: skip_ascii_non_delete_portable,
        }
    }
}

#[inline(always)]
fn dispatch() -> &'static SimdDispatch {
    static DISPATCH: OnceLock<SimdDispatch> = OnceLock::new();
    DISPATCH.get_or_init(SimdDispatch::detect)
}

#[inline(always)]
fn ascii_delete_contains(byte: u8, ascii_lut: &[u8; 16]) -> bool {
    let idx = byte as usize;
    (ascii_lut[idx >> 3] & (1 << (idx & 7))) != 0
}

#[inline(always)]
fn find_non_ascii_scalar(bytes: &[u8], offset: usize) -> usize {
    let mut offset = offset;
    while offset < bytes.len() && bytes[offset] < 0x80 {
        offset += 1;
    }
    offset
}

#[inline(always)]
fn find_non_digit_ascii_scalar(bytes: &[u8], offset: usize) -> usize {
    let mut offset = offset;
    while offset < bytes.len() {
        let b = bytes[offset];
        if b >= 0x80 || b.is_ascii_digit() {
            break;
        }
        offset += 1;
    }
    offset
}

#[inline(always)]
fn find_ascii_non_delete_scalar(bytes: &[u8], offset: usize, ascii_lut: &[u8; 16]) -> usize {
    let mut offset = offset;
    while offset < bytes.len() {
        let b = bytes[offset];
        if b >= 0x80 || ascii_delete_contains(b, ascii_lut) {
            break;
        }
        offset += 1;
    }
    offset
}

#[inline(always)]
#[allow(dead_code)]
fn portable_ascii_delete_mask_16(chunk: Simd<u8, 16>, ascii_lut: Simd<u8, 16>) -> u64 {
    let byte_idx = chunk >> Simd::<u8, 16>::splat(3);
    let lut_byte = ascii_lut.swizzle_dyn(byte_idx);

    let shift_table = Simd::<u8, 16>::from_array(SHIFT_TABLE_16);
    let bit_pos = chunk & Simd::<u8, 16>::splat(7);
    let bit_mask = shift_table.swizzle_dyn(bit_pos);

    (lut_byte & bit_mask)
        .simd_ne(Simd::<u8, 16>::splat(0))
        .to_bitmask()
}

#[inline(always)]
#[allow(dead_code)]
fn portable_ascii_delete_mask_32(chunk: Simd<u8, 32>, ascii_lut: Simd<u8, 32>) -> u64 {
    let byte_idx = chunk >> Simd::<u8, 32>::splat(3);
    let lut_byte = ascii_lut.swizzle_dyn(byte_idx);

    let shift_table = Simd::<u8, 32>::from_array(SHIFT_TABLE_32);
    let bit_pos = chunk & Simd::<u8, 32>::splat(7);
    let bit_mask = shift_table.swizzle_dyn(bit_pos);

    (lut_byte & bit_mask)
        .simd_ne(Simd::<u8, 32>::splat(0))
        .to_bitmask()
}

#[inline(always)]
#[allow(dead_code)]
fn skip_ascii_portable(bytes: &[u8], offset: usize) -> usize {
    if offset >= bytes.len() || bytes[offset] >= 0x80 {
        return offset;
    }
    let mut offset = offset;
    const LANES: usize = 32;
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
    find_non_ascii_scalar(bytes, offset)
}

#[inline(always)]
#[allow(dead_code)]
fn skip_non_digit_ascii_portable(bytes: &[u8], offset: usize) -> usize {
    if offset >= bytes.len() {
        return offset;
    }
    let b0 = bytes[offset];
    if b0 >= 0x80 || b0.is_ascii_digit() {
        return offset;
    }
    let mut offset = offset;
    const LANES: usize = 32;
    let non_ascii = Simd::<u8, LANES>::splat(0x80u8);
    let digit_lo = Simd::<u8, LANES>::splat(b'0');
    let digit_hi = Simd::<u8, LANES>::splat(b'9' + 1);

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
    find_non_digit_ascii_scalar(bytes, offset)
}

#[inline(always)]
#[allow(dead_code)]
fn skip_ascii_non_delete_portable(
    bytes: &[u8],
    offset: usize,
    ascii_lut: &[u8; 16],
    _ascii_lut_simd: Simd<u8, 16>,
) -> usize {
    if offset >= bytes.len() {
        return offset;
    }
    let b0 = bytes[offset];
    if b0 >= 0x80 || ascii_delete_contains(b0, ascii_lut) {
        return offset;
    }

    let mut lut32 = [0u8; 32];
    lut32[..16].copy_from_slice(ascii_lut);
    lut32[16..].copy_from_slice(ascii_lut);
    let ascii_lut_simd32 = Simd::<u8, 32>::from_array(lut32);

    let mut offset = offset;
    const LANES: usize = 32;
    let non_ascii = Simd::<u8, LANES>::splat(0x80u8);
    while offset + LANES <= bytes.len() {
        let chunk = Simd::<u8, LANES>::from_slice(&bytes[offset..]);
        let non_ascii_mask = chunk.simd_ge(non_ascii).to_bitmask();
        let delete_mask = portable_ascii_delete_mask_32(chunk, ascii_lut_simd32);
        let stop_mask = non_ascii_mask | delete_mask;
        if stop_mask != 0 {
            offset += stop_mask.trailing_zeros() as usize;
            return offset;
        }
        offset += LANES;
    }

    while offset + 16 <= bytes.len() {
        let chunk = Simd::<u8, 16>::from_slice(&bytes[offset..]);
        let non_ascii_mask = chunk.simd_ge(Simd::<u8, 16>::splat(0x80u8)).to_bitmask();
        let delete_mask =
            portable_ascii_delete_mask_16(chunk, Simd::<u8, 16>::from_array(*ascii_lut));
        let stop_mask = non_ascii_mask | delete_mask;
        if stop_mask != 0 {
            offset += stop_mask.trailing_zeros() as usize;
            return offset;
        }
        offset += 16;
    }

    find_ascii_non_delete_scalar(bytes, offset, ascii_lut)
}

#[cfg(all(feature = "simd_runtime_dispatch", target_arch = "x86_64"))]
#[target_feature(enable = "avx2")]
unsafe fn skip_ascii_avx2_impl(bytes: &[u8], mut offset: usize) -> usize {
    while offset + 32 <= bytes.len() {
        let chunk = unsafe { _mm256_loadu_si256(bytes.as_ptr().add(offset) as *const __m256i) };
        let mask = _mm256_movemask_epi8(chunk) as u32;
        if mask != 0 {
            return offset + mask.trailing_zeros() as usize;
        }
        offset += 32;
    }
    find_non_ascii_scalar(bytes, offset)
}

#[cfg(all(feature = "simd_runtime_dispatch", target_arch = "x86_64"))]
fn skip_ascii_avx2(bytes: &[u8], offset: usize) -> usize {
    if offset >= bytes.len() || bytes[offset] >= 0x80 {
        return offset;
    }
    unsafe { skip_ascii_avx2_impl(bytes, offset) }
}

#[cfg(all(feature = "simd_runtime_dispatch", target_arch = "x86_64"))]
#[target_feature(enable = "avx2")]
unsafe fn skip_non_digit_ascii_avx2_impl(bytes: &[u8], mut offset: usize) -> usize {
    let digit_lo = _mm256_set1_epi8((b'0' - 1) as i8);
    let digit_hi = _mm256_set1_epi8((b'9' + 1) as i8);
    while offset + 32 <= bytes.len() {
        let chunk = unsafe { _mm256_loadu_si256(bytes.as_ptr().add(offset) as *const __m256i) };
        let non_ascii_mask = _mm256_movemask_epi8(chunk) as u32;
        let ge_lo = _mm256_cmpgt_epi8(chunk, digit_lo);
        let lt_hi = _mm256_cmpgt_epi8(digit_hi, chunk);
        let digit_mask = _mm256_movemask_epi8(_mm256_and_si256(ge_lo, lt_hi)) as u32;
        let stop_mask = non_ascii_mask | digit_mask;
        if stop_mask != 0 {
            return offset + stop_mask.trailing_zeros() as usize;
        }
        offset += 32;
    }
    find_non_digit_ascii_scalar(bytes, offset)
}

#[cfg(all(feature = "simd_runtime_dispatch", target_arch = "x86_64"))]
fn skip_non_digit_ascii_avx2(bytes: &[u8], offset: usize) -> usize {
    if offset >= bytes.len() {
        return offset;
    }
    let b0 = bytes[offset];
    if b0 >= 0x80 || b0.is_ascii_digit() {
        return offset;
    }
    unsafe { skip_non_digit_ascii_avx2_impl(bytes, offset) }
}

#[cfg(all(feature = "simd_runtime_dispatch", target_arch = "x86_64"))]
#[target_feature(enable = "avx2")]
unsafe fn skip_ascii_non_delete_avx2_impl(
    bytes: &[u8],
    mut offset: usize,
    ascii_lut: &[u8; 16],
) -> usize {
    let mut lut32 = [0u8; 32];
    lut32[..16].copy_from_slice(ascii_lut);
    lut32[16..].copy_from_slice(ascii_lut);

    let shuffle_lut = unsafe { _mm256_loadu_si256(lut32.as_ptr() as *const __m256i) };
    let shift_table = unsafe { _mm256_loadu_si256(SHIFT_TABLE_32.as_ptr() as *const __m256i) };
    let low_nibble_mask = _mm256_set1_epi8(0x0f);
    let bit_pos_mask = _mm256_set1_epi8(0x07);
    let zero = _mm256_setzero_si256();

    while offset + 32 <= bytes.len() {
        let chunk = unsafe { _mm256_loadu_si256(bytes.as_ptr().add(offset) as *const __m256i) };
        let non_ascii_mask = _mm256_movemask_epi8(chunk) as u32;

        let byte_idx = _mm256_and_si256(_mm256_srli_epi16(chunk, 3), low_nibble_mask);
        let lut_byte = _mm256_shuffle_epi8(shuffle_lut, byte_idx);
        let bit_pos = _mm256_and_si256(chunk, bit_pos_mask);
        let bit_mask = _mm256_shuffle_epi8(shift_table, bit_pos);
        let deleted = _mm256_and_si256(lut_byte, bit_mask);
        let delete_mask =
            (!_mm256_movemask_epi8(_mm256_cmpeq_epi8(deleted, zero)) as u32) & u32::MAX;

        let stop_mask = non_ascii_mask | delete_mask;
        if stop_mask != 0 {
            return offset + stop_mask.trailing_zeros() as usize;
        }
        offset += 32;
    }

    find_ascii_non_delete_scalar(bytes, offset, ascii_lut)
}

#[cfg(all(feature = "simd_runtime_dispatch", target_arch = "x86_64"))]
fn skip_ascii_non_delete_avx2(
    bytes: &[u8],
    offset: usize,
    ascii_lut: &[u8; 16],
    _ascii_lut_simd: Simd<u8, 16>,
) -> usize {
    if offset >= bytes.len() {
        return offset;
    }
    let b0 = bytes[offset];
    if b0 >= 0x80 || ascii_delete_contains(b0, ascii_lut) {
        return offset;
    }
    unsafe { skip_ascii_non_delete_avx2_impl(bytes, offset, ascii_lut) }
}

#[cfg(all(feature = "simd_runtime_dispatch", target_arch = "aarch64"))]
#[inline(always)]
unsafe fn first_non_ascii_in_neon(bytes: *const u8, offset: usize) -> usize {
    let chunk = unsafe { vld1q_u8(bytes.add(offset)) };
    let mut scratch = [0u8; 16];
    unsafe { vst1q_u8(scratch.as_mut_ptr(), chunk) };
    scratch
        .iter()
        .position(|&b| b >= 0x80)
        .map_or(offset + 16, |idx| offset + idx)
}

#[cfg(all(feature = "simd_runtime_dispatch", target_arch = "aarch64"))]
fn skip_ascii_neon(bytes: &[u8], offset: usize) -> usize {
    if offset >= bytes.len() || bytes[offset] >= 0x80 {
        return offset;
    }

    let mut offset = offset;
    unsafe {
        while offset + 16 <= bytes.len() {
            let chunk = vld1q_u8(bytes.as_ptr().add(offset));
            if vmaxvq_u8(chunk) >= 0x80 {
                return first_non_ascii_in_neon(bytes.as_ptr(), offset);
            }
            offset += 16;
        }
    }

    find_non_ascii_scalar(bytes, offset)
}

#[cfg(all(feature = "simd_runtime_dispatch", target_arch = "aarch64"))]
fn skip_non_digit_ascii_neon(bytes: &[u8], offset: usize) -> usize {
    if offset >= bytes.len() {
        return offset;
    }
    let b0 = bytes[offset];
    if b0 >= 0x80 || b0.is_ascii_digit() {
        return offset;
    }

    let mut offset = offset;
    unsafe {
        let digit_lo = vdupq_n_u8(b'0');
        let digit_hi = vdupq_n_u8(b'9');
        while offset + 16 <= bytes.len() {
            let chunk = vld1q_u8(bytes.as_ptr().add(offset));
            let has_non_ascii = vmaxvq_u8(chunk) >= 0x80;
            let is_digit = vandq_u8(vcgeq_u8(chunk, digit_lo), vcleq_u8(chunk, digit_hi));
            if has_non_ascii || vmaxvq_u8(is_digit) != 0 {
                let mut scratch = [0u8; 16];
                vst1q_u8(scratch.as_mut_ptr(), chunk);
                return scratch
                    .iter()
                    .position(|&b| b >= 0x80 || b.is_ascii_digit())
                    .map_or(offset + 16, |idx| offset + idx);
            }
            offset += 16;
        }
    }

    find_non_digit_ascii_scalar(bytes, offset)
}

#[cfg(all(feature = "simd_runtime_dispatch", target_arch = "aarch64"))]
fn skip_ascii_non_delete_neon(
    bytes: &[u8],
    offset: usize,
    ascii_lut: &[u8; 16],
    _ascii_lut_simd: Simd<u8, 16>,
) -> usize {
    if offset >= bytes.len() {
        return offset;
    }
    let b0 = bytes[offset];
    if b0 >= 0x80 || ascii_delete_contains(b0, ascii_lut) {
        return offset;
    }

    let mut offset = offset;
    unsafe {
        let lut = vld1q_u8(ascii_lut.as_ptr());
        let shift = vld1q_u8(SHIFT_TABLE_16.as_ptr());
        let seven = vdupq_n_u8(7);

        while offset + 16 <= bytes.len() {
            let chunk = vld1q_u8(bytes.as_ptr().add(offset));
            let has_non_ascii = vmaxvq_u8(chunk) >= 0x80;

            let byte_idx = vshrq_n_u8(chunk, 3);
            let lut_byte = vqtbl1q_u8(lut, byte_idx);
            let bit_pos = vandq_u8(chunk, seven);
            let bit_mask = vqtbl1q_u8(shift, bit_pos);
            let deleted = vandq_u8(lut_byte, bit_mask);

            if has_non_ascii || vmaxvq_u8(deleted) != 0 {
                let mut scratch = [0u8; 16];
                vst1q_u8(scratch.as_mut_ptr(), chunk);
                return scratch
                    .iter()
                    .position(|&b| b >= 0x80 || ascii_delete_contains(b, ascii_lut))
                    .map_or(offset + 16, |idx| offset + idx);
            }
            offset += 16;
        }
    }

    find_ascii_non_delete_scalar(bytes, offset, ascii_lut)
}

/// Advances `offset` past all ASCII bytes (`< 0x80`) using the best available kernel.
#[inline(always)]
pub fn skip_ascii_simd(bytes: &[u8], offset: usize) -> usize {
    (dispatch().skip_ascii)(bytes, offset)
}

/// Advances `offset` past non-digit, non-ASCII-stop bytes using the best available kernel.
#[inline(always)]
pub fn skip_non_digit_ascii_simd(bytes: &[u8], offset: usize) -> usize {
    (dispatch().skip_non_digit_ascii)(bytes, offset)
}

/// Advances `offset` past ASCII bytes that are neither deletable nor non-ASCII.
#[inline(always)]
pub fn skip_ascii_non_delete_simd(
    bytes: &[u8],
    offset: usize,
    ascii_lut: &[u8; 16],
    ascii_lut_simd: Simd<u8, 16>,
) -> usize {
    (dispatch().skip_ascii_non_delete)(bytes, offset, ascii_lut, ascii_lut_simd)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skip_ascii_matches_scalar_behavior() {
        let text = "plain ascii 123".as_bytes();
        assert_eq!(skip_ascii_simd(text, 0), text.len());

        let mixed = "hello世界".as_bytes();
        assert_eq!(skip_ascii_simd(mixed, 0), 5);
        assert_eq!(skip_ascii_simd(mixed, 5), 5);
    }

    #[test]
    fn skip_non_digit_ascii_matches_scalar_behavior() {
        let text = "abcdefXYZ".as_bytes();
        assert_eq!(skip_non_digit_ascii_simd(text, 0), text.len());

        let mixed = "abc9def".as_bytes();
        assert_eq!(skip_non_digit_ascii_simd(mixed, 0), 3);

        let unicode = "abc你".as_bytes();
        assert_eq!(skip_non_digit_ascii_simd(unicode, 0), 3);
    }

    #[test]
    fn skip_ascii_non_delete_stops_on_delete_and_unicode() {
        let mut ascii_lut = [0u8; 16];
        ascii_lut[(b'!' as usize) >> 3] |= 1 << (b'!' & 7);
        let ascii_lut_simd = Simd::<u8, 16>::from_array(ascii_lut);

        let text = "abc!def".as_bytes();
        assert_eq!(
            skip_ascii_non_delete_simd(text, 0, &ascii_lut, ascii_lut_simd),
            3
        );

        let unicode = "abcdef你".as_bytes();
        assert_eq!(
            skip_ascii_non_delete_simd(unicode, 0, &ascii_lut, ascii_lut_simd),
            6
        );
    }
}
