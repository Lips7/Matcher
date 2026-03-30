//! SIMD-accelerated skip helpers for the text transformation pipeline.
//!
//! Provides three public functions used by the charwise and delete engines to
//! fast-forward over irrelevant ASCII byte runs without per-byte branching:
//!
//! - [`skip_ascii_simd`] -- skips all ASCII bytes (`< 0x80`). Used by
//!   `FanjianFindIter` (in [`super::charwise`]).
//! - [`skip_non_digit_ascii_simd`] -- skips ASCII bytes that are not digits
//!   (`'0'..='9'`). Used by `PinyinFindIter` (in [`super::charwise`]).
//! - [`skip_ascii_non_delete_simd`] -- skips ASCII bytes that are not in the
//!   delete bitset. Used by [`super::delete::DeleteMatcher`].
//!
//! # Dispatch strategy
//!
//! Controlled by the `simd_runtime_dispatch` feature flag:
//!
//! | Platform | Dispatch | Primary kernel | Fallback |
//! |----------|----------|---------------|----------|
//! | x86_64 + `simd_runtime_dispatch` | Runtime (`OnceLock<SimdDispatch>`) | AVX2 intrinsics | Portable `std::simd` (32-lane) |
//! | aarch64 + `simd_runtime_dispatch` | Compile-time | NEON intrinsics (16-lane) | -- |
//! | Everything else | Compile-time | Portable `std::simd` (32-lane) | -- |
//!
//! # Delete-mask algorithm
//!
//! The "non-delete" skip functions probe a 128-bit ASCII bitset (`ascii_lut`,
//! 16 bytes) inside the SIMD loop using a shuffle-based lookup:
//!
//! 1. `byte_idx = byte >> 3` -- selects which of the 16 LUT bytes to read.
//! 2. `lut_byte = shuffle(ascii_lut, byte_idx)` -- SIMD table lookup.
//! 3. `bit_pos = byte & 7` -- selects the bit within the LUT byte.
//! 4. `bit_mask = shuffle(SHIFT_TABLE, bit_pos)` -- converts bit position to
//!    a single-bit mask (1, 2, 4, ..., 128).
//! 5. `deleted = lut_byte & bit_mask` -- non-zero means the byte is deletable.
//!
//! This is combined (OR) with the non-ASCII mask to produce a stop mask; the
//! first set bit (via `trailing_zeros`) gives the exact stop offset.

#[cfg(not(all(feature = "simd_runtime_dispatch", target_arch = "aarch64")))]
use std::simd::Simd;
#[cfg(not(all(feature = "simd_runtime_dispatch", target_arch = "aarch64")))]
use std::simd::cmp::{SimdPartialEq, SimdPartialOrd};

#[cfg(all(feature = "simd_runtime_dispatch", target_arch = "aarch64"))]
use std::arch::aarch64::*;
#[cfg(all(feature = "simd_runtime_dispatch", target_arch = "x86_64"))]
use std::arch::x86_64::*;
#[cfg(all(feature = "simd_runtime_dispatch", target_arch = "x86_64"))]
use std::sync::OnceLock;

/// Function-pointer signature for the two-argument skip helpers
/// ([`skip_ascii_simd`] and [`skip_non_digit_ascii_simd`]).
#[cfg(all(feature = "simd_runtime_dispatch", target_arch = "x86_64"))]
type SkipFn = fn(&[u8], usize) -> usize;

/// Function-pointer signature for the three-argument delete-aware skip helper
/// ([`skip_ascii_non_delete_simd`]).
#[cfg(all(feature = "simd_runtime_dispatch", target_arch = "x86_64"))]
type SkipDeleteFn = fn(&[u8], usize, &[u8; 16]) -> usize;

/// 16-byte lookup table mapping bit positions (0--7) to single-bit masks.
///
/// Entry `i % 8` equals `1 << (i % 8)`: `[1, 2, 4, 8, 16, 32, 64, 128]`,
/// repeated once to fill the 16-lane SIMD register. Used by SIMD shuffle
/// instructions (`swizzle_dyn` / `vqtbl1q_u8`) to convert the low 3 bits of
/// each input byte into the bitmask needed to probe the delete bitset.
const SHIFT_TABLE_16: [u8; 16] = [1, 2, 4, 8, 16, 32, 64, 128, 1, 2, 4, 8, 16, 32, 64, 128];

/// 32-byte version of [`SHIFT_TABLE_16`] for 32-lane SIMD paths (AVX2 and
/// portable 32-wide). Same pattern repeated four times.
#[cfg(not(all(feature = "simd_runtime_dispatch", target_arch = "aarch64")))]
const SHIFT_TABLE_32: [u8; 32] = [
    1, 2, 4, 8, 16, 32, 64, 128, 1, 2, 4, 8, 16, 32, 64, 128, 1, 2, 4, 8, 16, 32, 64, 128, 1, 2, 4,
    8, 16, 32, 64, 128,
];

/// Cached function-pointer table for x86-64 runtime SIMD dispatch.
///
/// Populated once (via [`OnceLock`]) on first use by [`SimdDispatch::detect`].
/// After initialization, each public skip function resolves to a single
/// indirect call through the stored function pointer.
#[cfg(all(feature = "simd_runtime_dispatch", target_arch = "x86_64"))]
struct SimdDispatch {
    /// Best available implementation of [`skip_ascii_simd`].
    skip_ascii: SkipFn,
    /// Best available implementation of [`skip_non_digit_ascii_simd`].
    skip_non_digit_ascii: SkipFn,
    /// Best available implementation of [`skip_ascii_non_delete_simd`].
    skip_ascii_non_delete: SkipDeleteFn,
}

#[cfg(all(feature = "simd_runtime_dispatch", target_arch = "x86_64"))]
impl SimdDispatch {
    /// Probes `is_x86_feature_detected!("avx2")` and fills the dispatch table
    /// with either AVX2 or portable function pointers accordingly.
    fn detect() -> Self {
        if std::arch::is_x86_feature_detected!("avx2") {
            return Self {
                skip_ascii: skip_ascii_avx2,
                skip_non_digit_ascii: skip_non_digit_ascii_avx2,
                skip_ascii_non_delete: skip_ascii_non_delete_avx2,
            };
        }
        Self {
            skip_ascii: skip_ascii_portable,
            skip_non_digit_ascii: skip_non_digit_ascii_portable,
            skip_ascii_non_delete: skip_ascii_non_delete_portable,
        }
    }
}

/// Returns the lazily-initialized `&'static SimdDispatch` for x86-64 runtime dispatch.
///
/// The [`OnceLock`] guarantees thread-safe one-time initialization: the first
/// caller runs [`SimdDispatch::detect`]; all subsequent callers get the cached
/// result with no synchronization overhead beyond a single atomic load.
#[cfg(all(feature = "simd_runtime_dispatch", target_arch = "x86_64"))]
#[inline(always)]
fn dispatch() -> &'static SimdDispatch {
    static DISPATCH: OnceLock<SimdDispatch> = OnceLock::new();
    DISPATCH.get_or_init(SimdDispatch::detect)
}

/// Tests whether `byte` is marked for deletion in the 128-bit `ascii_lut` bitset.
///
/// The LUT packs 128 bits (one per ASCII codepoint 0x00--0x7F) into 16 bytes:
/// byte index = `byte >> 3`, bit position = `byte & 7`. This is the scalar
/// equivalent of the SIMD delete-mask algorithm described in the module docs.
///
/// Callers must only pass ASCII bytes (`< 128`); for non-ASCII bytes the index
/// would be in range (0--15) but the result is meaningless.
#[inline(always)]
fn ascii_delete_contains(byte: u8, ascii_lut: &[u8; 16]) -> bool {
    let idx = byte as usize;
    (ascii_lut[idx >> 3] & (1 << (idx & 7))) != 0
}

/// Scalar tail: returns the first offset where `bytes[offset] >= 0x80`, or `bytes.len()`.
///
/// Used after the SIMD loop to handle the remaining bytes that do not fill a
/// full SIMD lane width.
#[inline(always)]
fn find_non_ascii_scalar(bytes: &[u8], offset: usize) -> usize {
    let mut offset = offset;
    while offset < bytes.len() && bytes[offset] < 0x80 {
        offset += 1;
    }
    offset
}

/// Scalar tail: returns the first offset where the byte is non-ASCII (`>= 0x80`)
/// or an ASCII digit (`'0'..='9'`), or `bytes.len()` if no such byte exists.
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

/// Scalar tail: returns the first offset where the byte is non-ASCII or is
/// marked deletable in `ascii_lut`, or `bytes.len()` if no such byte exists.
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

/// 16-lane portable SIMD helper: probes the 128-bit delete bitset for each byte in `chunk`.
///
/// Returns a bitmask where bit `i` is set iff `chunk[i]` is marked deletable
/// in `ascii_lut`. Implements the shuffle-based delete-mask algorithm described
/// in the [module documentation](self) using `swizzle_dyn` for table lookup.
///
/// Only meaningful for ASCII bytes; non-ASCII bytes may produce spurious
/// results, so callers must OR this with a separate non-ASCII mask.
#[cfg(not(all(feature = "simd_runtime_dispatch", target_arch = "aarch64")))]
#[inline(always)]
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

/// 32-lane version of [`portable_ascii_delete_mask_16`] using [`SHIFT_TABLE_32`].
///
/// Same algorithm, wider register. The `ascii_lut` must be a 32-byte vector
/// with the 16-byte LUT duplicated in both halves (required by `swizzle_dyn`
/// on 32-lane vectors, which treats each 16-byte half independently).
#[cfg(not(all(feature = "simd_runtime_dispatch", target_arch = "aarch64")))]
#[inline(always)]
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

/// Portable 32-lane `std::simd` implementation of ASCII skip.
///
/// Loads 32-byte chunks and compares each lane against `0x80`. The first lane
/// with a set bit (via `to_bitmask` + `trailing_zeros`) gives the exact stop
/// offset. Falls back to [`find_non_ascii_scalar`] for the tail.
#[cfg(not(all(feature = "simd_runtime_dispatch", target_arch = "aarch64")))]
#[inline(always)]
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

/// Portable 32-lane `std::simd` implementation of non-digit-ASCII skip.
///
/// For each 32-byte chunk, computes two lane masks: `is_non_ascii` (byte >=
/// 0x80) and `is_digit` (byte in `'0'..='9'`). ORs them into a single stop
/// mask; the first set bit gives the exact stop offset. Falls back to
/// [`find_non_digit_ascii_scalar`] for the tail.
#[cfg(not(all(feature = "simd_runtime_dispatch", target_arch = "aarch64")))]
#[inline(always)]
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

/// Portable SIMD implementation of ASCII-non-delete skip.
///
/// Uses 32-lane chunks with [`portable_ascii_delete_mask_32`] ORed with the
/// non-ASCII mask. When fewer than 32 bytes remain, drops to a 16-lane loop
/// with [`portable_ascii_delete_mask_16`], then to
/// [`find_ascii_non_delete_scalar`] for the final tail.
///
/// The `ascii_lut` is expanded to a 32-byte vector (duplicated halves) once
/// before the main loop to match the 32-lane `swizzle_dyn` requirement.
#[cfg(not(all(feature = "simd_runtime_dispatch", target_arch = "aarch64")))]
#[inline(always)]
fn skip_ascii_non_delete_portable(bytes: &[u8], offset: usize, ascii_lut: &[u8; 16]) -> usize {
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

/// AVX2 inner loop for ASCII skip.
///
/// Loads 32-byte chunks via `_mm256_loadu_si256` (unaligned) and uses
/// `_mm256_movemask_epi8` to extract the high bit of each lane into a `u32`
/// bitmask. Any set bit indicates a non-ASCII byte. Falls back to
/// [`find_non_ascii_scalar`] for the tail.
///
/// # Safety
///
/// - Requires AVX2 support (enforced by `#[target_feature(enable = "avx2")]`).
/// - The `_mm256_loadu_si256` load is unaligned and reads exactly 32 bytes
///   starting at `bytes.as_ptr().add(offset)`. The `offset + 32 <= bytes.len()`
///   loop guard ensures the read is within bounds.
#[cfg(all(feature = "simd_runtime_dispatch", target_arch = "x86_64"))]
#[target_feature(enable = "avx2")]
unsafe fn skip_ascii_avx2_impl(bytes: &[u8], mut offset: usize) -> usize {
    while offset + 32 <= bytes.len() {
        // SAFETY: `offset + 32 <= bytes.len()` guard ensures the 32-byte read is within bounds.
        let chunk = unsafe { _mm256_loadu_si256(bytes.as_ptr().add(offset) as *const __m256i) };
        let mask = _mm256_movemask_epi8(chunk) as u32;
        if mask != 0 {
            return offset + mask.trailing_zeros() as usize;
        }
        offset += 32;
    }
    find_non_ascii_scalar(bytes, offset)
}

/// AVX2 entry point for ASCII skip.
///
/// Performs a cheap scalar check on `bytes[offset]` before entering the unsafe
/// AVX2 loop, avoiding overhead when the very first byte is already non-ASCII
/// or the offset is past the end.
#[cfg(all(feature = "simd_runtime_dispatch", target_arch = "x86_64"))]
fn skip_ascii_avx2(bytes: &[u8], offset: usize) -> usize {
    if offset >= bytes.len() || bytes[offset] >= 0x80 {
        return offset;
    }
    // SAFETY: AVX2 support verified by `SimdDispatch::detect` before this function pointer is stored.
    unsafe { skip_ascii_avx2_impl(bytes, offset) }
}

/// AVX2 inner loop for non-digit-ASCII skip.
///
/// Computes `stop_mask = non_ascii_mask | digit_mask` per 32-byte chunk.
/// Digit detection uses signed comparisons: `chunk > ('0' - 1)` AND
/// `('9' + 1) > chunk`. Falls back to [`find_non_digit_ascii_scalar`] for the
/// tail.
///
/// # Safety
///
/// Same as [`skip_ascii_avx2_impl`]: requires AVX2 and the loop guard ensures
/// all 32-byte loads are within bounds.
#[cfg(all(feature = "simd_runtime_dispatch", target_arch = "x86_64"))]
#[target_feature(enable = "avx2")]
unsafe fn skip_non_digit_ascii_avx2_impl(bytes: &[u8], mut offset: usize) -> usize {
    let digit_lo = _mm256_set1_epi8((b'0' - 1) as i8);
    let digit_hi = _mm256_set1_epi8((b'9' + 1) as i8);
    while offset + 32 <= bytes.len() {
        // SAFETY: `offset + 32 <= bytes.len()` guard ensures the 32-byte read is within bounds.
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

/// AVX2 entry point for non-digit-ASCII skip.
///
/// Performs a cheap scalar guard before delegating to the unsafe AVX2 loop.
#[cfg(all(feature = "simd_runtime_dispatch", target_arch = "x86_64"))]
fn skip_non_digit_ascii_avx2(bytes: &[u8], offset: usize) -> usize {
    if offset >= bytes.len() {
        return offset;
    }
    let b0 = bytes[offset];
    if b0 >= 0x80 || b0.is_ascii_digit() {
        return offset;
    }
    // SAFETY: AVX2 support verified by `SimdDispatch::detect` before this function pointer is stored.
    unsafe { skip_non_digit_ascii_avx2_impl(bytes, offset) }
}

/// AVX2 inner loop for ASCII-non-delete skip.
///
/// Implements the same shuffle-based delete-mask algorithm as the portable
/// version, but using AVX2 intrinsics:
///
/// - `_mm256_shuffle_epi8` replaces `swizzle_dyn` for both the LUT lookup
///   and the shift-table lookup.
/// - `_mm256_srli_epi16` with a mask extracts `byte >> 3` (the LUT byte index).
/// - `_mm256_cmpeq_epi8` + inverted `_mm256_movemask_epi8` produces the delete
///   bitmask.
///
/// The `ascii_lut` is expanded to 32 bytes (duplicated halves) to match the
/// AVX2 lane structure where `_mm256_shuffle_epi8` operates on each 128-bit
/// half independently.
///
/// # Safety
///
/// - Requires AVX2 (enforced by `#[target_feature]`).
/// - All `_mm256_loadu_si256` loads are guarded by `offset + 32 <= bytes.len()`.
/// - `SHIFT_TABLE_32` and `lut32` are stack-allocated 32-byte arrays loaded
///   via `_mm256_loadu_si256`; both are always 32 bytes, satisfying the read size.
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

    // SAFETY: `lut32` is a local [u8; 32] on the stack, valid for a 32-byte read.
    let shuffle_lut = unsafe { _mm256_loadu_si256(lut32.as_ptr() as *const __m256i) };
    // SAFETY: `SHIFT_TABLE_32` is a static [u8; 32], valid for a 32-byte read.
    let shift_table = unsafe { _mm256_loadu_si256(SHIFT_TABLE_32.as_ptr() as *const __m256i) };
    let low_nibble_mask = _mm256_set1_epi8(0x0f);
    let bit_pos_mask = _mm256_set1_epi8(0x07);
    let zero = _mm256_setzero_si256();

    while offset + 32 <= bytes.len() {
        // SAFETY: `offset + 32 <= bytes.len()` guard ensures the 32-byte read is within bounds.
        let chunk = unsafe { _mm256_loadu_si256(bytes.as_ptr().add(offset) as *const __m256i) };
        let non_ascii_mask = _mm256_movemask_epi8(chunk) as u32;

        let byte_idx = _mm256_and_si256(_mm256_srli_epi16(chunk, 3), low_nibble_mask);
        let lut_byte = _mm256_shuffle_epi8(shuffle_lut, byte_idx);
        let bit_pos = _mm256_and_si256(chunk, bit_pos_mask);
        let bit_mask = _mm256_shuffle_epi8(shift_table, bit_pos);
        let deleted = _mm256_and_si256(lut_byte, bit_mask);
        let delete_mask = !_mm256_movemask_epi8(_mm256_cmpeq_epi8(deleted, zero)) as u32;

        let stop_mask = non_ascii_mask | delete_mask;
        if stop_mask != 0 {
            return offset + stop_mask.trailing_zeros() as usize;
        }
        offset += 32;
    }

    find_ascii_non_delete_scalar(bytes, offset, ascii_lut)
}

/// AVX2 entry point for ASCII-non-delete skip.
///
/// Performs a cheap scalar guard before delegating to the unsafe AVX2 loop.
#[cfg(all(feature = "simd_runtime_dispatch", target_arch = "x86_64"))]
fn skip_ascii_non_delete_avx2(bytes: &[u8], offset: usize, ascii_lut: &[u8; 16]) -> usize {
    if offset >= bytes.len() {
        return offset;
    }
    let b0 = bytes[offset];
    if b0 >= 0x80 || ascii_delete_contains(b0, ascii_lut) {
        return offset;
    }
    // SAFETY: AVX2 support verified by `SimdDispatch::detect` before this function pointer is stored.
    unsafe { skip_ascii_non_delete_avx2_impl(bytes, offset, ascii_lut) }
}

/// NEON helper: finds the exact lane index of the first non-ASCII byte in a
/// 16-byte chunk.
///
/// Called after `vmaxvq_u8` confirmed at least one lane is `>= 0x80`. Stores
/// the vector to a stack scratch buffer and scans it byte-by-byte to find the
/// exact position. This avoids the need for a NEON horizontal-scan intrinsic
/// that does not exist on aarch64.
///
/// # Safety
///
/// - `bytes.add(offset)` must point to a valid 16-byte region (guaranteed by
///   the caller's `offset + 16 <= bytes.len()` guard).
/// - NEON intrinsics (`vld1q_u8`, `vst1q_u8`) require aarch64 (enforced by
///   `cfg`).
#[cfg(all(feature = "simd_runtime_dispatch", target_arch = "aarch64"))]
#[inline(always)]
unsafe fn first_non_ascii_in_neon(bytes: *const u8, offset: usize) -> usize {
    // SAFETY: caller guarantees `offset + 16 <= bytes.len()`, so `bytes.add(offset)` is valid for a 16-byte read.
    let chunk = unsafe { vld1q_u8(bytes.add(offset)) };
    let mut scratch = [0u8; 16];
    // SAFETY: `scratch` is a local [u8; 16] on the stack, so the pointer is valid for a 16-byte store.
    unsafe { vst1q_u8(scratch.as_mut_ptr(), chunk) };
    scratch
        .iter()
        .position(|&b| b >= 0x80)
        .map_or(offset + 16, |idx| offset + idx)
}

/// NEON 16-byte-at-a-time ASCII skip.
///
/// Uses `vmaxvq_u8` (horizontal max across all 16 lanes) as a fast
/// any-non-ASCII test: if the max is `< 0x80`, the entire chunk is ASCII.
/// When a non-ASCII chunk is found, delegates to [`first_non_ascii_in_neon`]
/// for the exact lane position. Scalar tail via [`find_non_ascii_scalar`].
///
/// # Safety (internal)
///
/// All NEON loads (`vld1q_u8`) are guarded by `offset + 16 <= bytes.len()`.
#[cfg(all(feature = "simd_runtime_dispatch", target_arch = "aarch64"))]
#[inline(always)]
fn skip_ascii_neon(bytes: &[u8], offset: usize) -> usize {
    if offset >= bytes.len() || bytes[offset] >= 0x80 {
        return offset;
    }

    let mut offset = offset;
    // SAFETY: all `vld1q_u8` loads are guarded by `offset + 16 <= bytes.len()`, ensuring valid 16-byte reads.
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

/// NEON 16-byte-at-a-time non-digit-ASCII skip.
///
/// Combines the `vmaxvq_u8` non-ASCII test with a digit-range test per chunk:
/// `is_digit = vcgeq_u8(chunk, '0') & vcleq_u8(chunk, '9')`. If either
/// condition fires, stores the chunk to a scratch buffer for scalar
/// position-finding. Falls back to [`find_non_digit_ascii_scalar`] for the
/// tail.
///
/// # Safety (internal)
///
/// All NEON loads (`vld1q_u8`) are guarded by `offset + 16 <= bytes.len()`.
#[cfg(all(feature = "simd_runtime_dispatch", target_arch = "aarch64"))]
#[inline(always)]
fn skip_non_digit_ascii_neon(bytes: &[u8], offset: usize) -> usize {
    if offset >= bytes.len() {
        return offset;
    }
    let b0 = bytes[offset];
    if b0 >= 0x80 || b0.is_ascii_digit() {
        return offset;
    }

    let mut offset = offset;
    // SAFETY: all NEON loads are guarded by `offset + 16 <= bytes.len()`; stores target local stack buffers.
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

/// NEON 16-byte-at-a-time ASCII-non-delete skip.
///
/// Implements the shuffle-based delete-mask algorithm using NEON intrinsics:
/// `vqtbl1q_u8` for table lookup (replaces `swizzle_dyn`), `vshrq_n_u8` for
/// the `byte >> 3` shift, and `vandq_u8` for bitwise AND. Combined with the
/// `vmaxvq_u8` non-ASCII check. Falls back to
/// [`find_ascii_non_delete_scalar`] for the tail.
///
/// # Safety (internal)
///
/// - All NEON loads are guarded by `offset + 16 <= bytes.len()`.
/// - `ascii_lut` and [`SHIFT_TABLE_16`] are both exactly 16 bytes, matching
///   the `vld1q_u8` / `vqtbl1q_u8` requirement.
#[cfg(all(feature = "simd_runtime_dispatch", target_arch = "aarch64"))]
#[inline(always)]
fn skip_ascii_non_delete_neon(bytes: &[u8], offset: usize, ascii_lut: &[u8; 16]) -> usize {
    if offset >= bytes.len() {
        return offset;
    }
    let b0 = bytes[offset];
    if b0 >= 0x80 || ascii_delete_contains(b0, ascii_lut) {
        return offset;
    }

    let mut offset = offset;
    // SAFETY: all NEON loads are guarded by `offset + 16 <= bytes.len()`; `ascii_lut` and `SHIFT_TABLE_16` are [u8; 16].
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

/// Advances `offset` past all ASCII bytes (`< 0x80`) using the best available SIMD kernel.
///
/// Returns the first offset where `bytes[offset] >= 0x80`, or `bytes.len()` if
/// the remaining bytes are all ASCII. If `offset >= bytes.len()` or
/// `bytes[offset]` is already non-ASCII, returns `offset` unchanged.
///
/// Dispatch: AVX2 (x86_64 runtime), NEON (aarch64 compile-time), or portable
/// `std::simd` fallback.
#[inline(always)]
pub(crate) fn skip_ascii_simd(bytes: &[u8], offset: usize) -> usize {
    #[cfg(all(feature = "simd_runtime_dispatch", target_arch = "x86_64"))]
    return (dispatch().skip_ascii)(bytes, offset);

    #[cfg(all(feature = "simd_runtime_dispatch", target_arch = "aarch64"))]
    return skip_ascii_neon(bytes, offset);

    #[cfg(not(all(
        feature = "simd_runtime_dispatch",
        any(target_arch = "x86_64", target_arch = "aarch64")
    )))]
    skip_ascii_portable(bytes, offset)
}

/// Advances `offset` past ASCII non-digit bytes, stopping at the first
/// non-ASCII byte or ASCII digit (`'0'..='9'`).
///
/// Returns the first offset where `bytes[offset] >= 0x80` or
/// `bytes[offset].is_ascii_digit()`, or `bytes.len()` if no such byte exists.
/// If `offset >= bytes.len()` or the byte at `offset` already matches the stop
/// condition, returns `offset` unchanged.
///
/// Dispatch: AVX2 (x86_64 runtime), NEON (aarch64 compile-time), or portable
/// `std::simd` fallback.
#[inline(always)]
pub(crate) fn skip_non_digit_ascii_simd(bytes: &[u8], offset: usize) -> usize {
    #[cfg(all(feature = "simd_runtime_dispatch", target_arch = "x86_64"))]
    return (dispatch().skip_non_digit_ascii)(bytes, offset);

    #[cfg(all(feature = "simd_runtime_dispatch", target_arch = "aarch64"))]
    return skip_non_digit_ascii_neon(bytes, offset);

    #[cfg(not(all(
        feature = "simd_runtime_dispatch",
        any(target_arch = "x86_64", target_arch = "aarch64")
    )))]
    skip_non_digit_ascii_portable(bytes, offset)
}

/// Advances `offset` past ASCII bytes that are not in the delete bitset,
/// stopping at the first non-ASCII byte or deletable ASCII byte.
///
/// `ascii_lut` is the 16-byte bitset covering ASCII codepoints 0x00--0x7F
/// (from [`super::delete::DeleteMatcher::ascii_lut`]).
///
/// Returns the first offset where `bytes[offset] >= 0x80` or the byte is
/// marked in `ascii_lut`, or `bytes.len()` if no such byte exists. If
/// `offset >= bytes.len()` or the byte at `offset` already matches the stop
/// condition, returns `offset` unchanged.
///
/// Dispatch: AVX2 (x86_64 runtime), NEON (aarch64 compile-time), or portable
/// `std::simd` fallback.
#[inline(always)]
pub(crate) fn skip_ascii_non_delete_simd(
    bytes: &[u8],
    offset: usize,
    ascii_lut: &[u8; 16],
) -> usize {
    #[cfg(all(feature = "simd_runtime_dispatch", target_arch = "x86_64"))]
    return (dispatch().skip_ascii_non_delete)(bytes, offset, ascii_lut);

    #[cfg(all(feature = "simd_runtime_dispatch", target_arch = "aarch64"))]
    return skip_ascii_non_delete_neon(bytes, offset, ascii_lut);

    #[cfg(not(all(
        feature = "simd_runtime_dispatch",
        any(target_arch = "x86_64", target_arch = "aarch64")
    )))]
    skip_ascii_non_delete_portable(bytes, offset, ascii_lut)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Confirms the ASCII skip helper agrees with the scalar baseline.
    #[test]
    fn skip_ascii_matches_scalar_behavior() {
        let text = "plain ascii 123".as_bytes();
        assert_eq!(skip_ascii_simd(text, 0), text.len());

        let mixed = "hello世界".as_bytes();
        assert_eq!(skip_ascii_simd(mixed, 0), 5);
        assert_eq!(skip_ascii_simd(mixed, 5), 5);
    }

    /// Confirms the digit-aware ASCII skip helper agrees with the scalar baseline.
    #[test]
    fn skip_non_digit_ascii_matches_scalar_behavior() {
        let text = "abcdefXYZ".as_bytes();
        assert_eq!(skip_non_digit_ascii_simd(text, 0), text.len());

        let mixed = "abc9def".as_bytes();
        assert_eq!(skip_non_digit_ascii_simd(mixed, 0), 3);

        let unicode = "abc你".as_bytes();
        assert_eq!(skip_non_digit_ascii_simd(unicode, 0), 3);
    }

    /// Confirms the delete-aware ASCII skip helper stops at either deletable ASCII or Unicode.
    #[test]
    fn skip_ascii_non_delete_stops_on_delete_and_unicode() {
        let mut ascii_lut = [0u8; 16];
        ascii_lut[(b'!' as usize) >> 3] |= 1 << (b'!' & 7);

        let text = "abc!def".as_bytes();
        assert_eq!(skip_ascii_non_delete_simd(text, 0, &ascii_lut), 3);

        let unicode = "abcdef你".as_bytes();
        assert_eq!(skip_ascii_non_delete_simd(unicode, 0, &ascii_lut), 6);
    }
}
