//! SIMD-accelerated non-ASCII byte density estimation for engine dispatch.
//!
//! Provides [`count_non_ascii_simd`] which counts bytes ≥ 0x80 in a byte slice,
//! used by [`super::engine::text_non_ascii_density`] to decide between the
//! bytewise and charwise scan engines.
//!
//! # Dispatch strategy
//!
//! Same model as [`crate::process::transform::simd`]:
//!
//! | Platform | Primary kernel | Fallback |
//! |----------|---------------|----------|
//! | aarch64 + `simd_runtime_dispatch` | NEON `vaddvq_u8(vshrq_n_u8(chunk, 7))` | — |
//! | x86_64 + `simd_runtime_dispatch` | AVX2 `_mm256_movemask_epi8` + `count_ones` | Portable `std::simd` (32-lane) |
//! | Everything else | Portable `std::simd` (32-lane) | — |

#[cfg(all(feature = "simd_runtime_dispatch", target_arch = "aarch64"))]
use std::arch::aarch64::*;
#[cfg(all(feature = "simd_runtime_dispatch", target_arch = "x86_64"))]
use std::arch::x86_64::*;
#[cfg(not(all(feature = "simd_runtime_dispatch", target_arch = "aarch64")))]
use std::simd::Simd;
#[cfg(not(all(feature = "simd_runtime_dispatch", target_arch = "aarch64")))]
use std::simd::cmp::SimdPartialOrd;
#[cfg(all(feature = "simd_runtime_dispatch", target_arch = "x86_64"))]
use std::sync::OnceLock;

// ── x86_64 runtime dispatch ──────────────────────────────────────────────────

#[cfg(all(feature = "simd_runtime_dispatch", target_arch = "x86_64"))]
type CountFn = fn(&[u8]) -> usize;

#[cfg(all(feature = "simd_runtime_dispatch", target_arch = "x86_64"))]
struct SimdDispatch {
    count_non_ascii: CountFn,
}

#[cfg(all(feature = "simd_runtime_dispatch", target_arch = "x86_64"))]
impl SimdDispatch {
    fn detect() -> Self {
        if std::arch::is_x86_feature_detected!("avx2") {
            return Self {
                count_non_ascii: count_non_ascii_avx2,
            };
        }
        Self {
            count_non_ascii: count_non_ascii_portable,
        }
    }
}

#[cfg(all(feature = "simd_runtime_dispatch", target_arch = "x86_64"))]
fn dispatch() -> &'static SimdDispatch {
    static DISPATCH: OnceLock<SimdDispatch> = OnceLock::new();
    DISPATCH.get_or_init(SimdDispatch::detect)
}

// ── Portable implementation (std::simd, 32-lane) ────────────────────────────

/// Counts bytes ≥ 0x80 using portable 32-lane `std::simd`.
///
/// Each 32-byte chunk is compared against 0x80; the resulting bitmask has one
/// bit per non-ASCII byte. `count_ones()` gives the count per chunk.
#[cfg(not(all(feature = "simd_runtime_dispatch", target_arch = "aarch64")))]
fn count_non_ascii_portable(bytes: &[u8]) -> usize {
    let mut count = 0u32;
    let mut offset = 0;
    const LANES: usize = 32;
    let threshold = Simd::<u8, LANES>::splat(0x80);
    while offset + LANES <= bytes.len() {
        let chunk = Simd::<u8, LANES>::from_slice(&bytes[offset..]);
        count += chunk.simd_ge(threshold).to_bitmask().count_ones();
        offset += LANES;
    }
    for &b in &bytes[offset..] {
        count += (b >> 7) as u32;
    }
    count as usize
}

// ── AVX2 implementation ─────────────────────────────────────────────────────

/// AVX2 inner loop: `_mm256_movemask_epi8` extracts the high bit of each byte
/// into a 32-bit mask. `count_ones()` gives the non-ASCII count per chunk.
///
/// # Safety
///
/// Requires AVX2 (enforced by `#[target_feature]`). All loads are guarded by
/// `offset + 32 <= bytes.len()`.
#[cfg(all(feature = "simd_runtime_dispatch", target_arch = "x86_64"))]
#[target_feature(enable = "avx2")]
unsafe fn count_non_ascii_avx2_impl(bytes: &[u8]) -> usize {
    let mut count = 0u32;
    let mut offset = 0;
    while offset + 32 <= bytes.len() {
        // SAFETY: `offset + 32 <= bytes.len()` guard ensures valid 32-byte read.
        let chunk = unsafe { _mm256_loadu_si256(bytes.as_ptr().add(offset) as *const __m256i) };
        count += (_mm256_movemask_epi8(chunk) as u32).count_ones();
        offset += 32;
    }
    for &b in &bytes[offset..] {
        count += (b >> 7) as u32;
    }
    count as usize
}

#[cfg(all(feature = "simd_runtime_dispatch", target_arch = "x86_64"))]
fn count_non_ascii_avx2(bytes: &[u8]) -> usize {
    // SAFETY: AVX2 support verified by `SimdDispatch::detect`.
    unsafe { count_non_ascii_avx2_impl(bytes) }
}

// ── NEON implementation ─────────────────────────────────────────────────────

/// NEON 16-byte-at-a-time count. `vshrq_n_u8(chunk, 7)` converts each byte to
/// 0 (ASCII) or 1 (non-ASCII). `vaddvq_u8` sums the 16 lanes horizontally.
///
/// # Safety (internal)
///
/// All `vld1q_u8` loads are guarded by `offset + 16 <= bytes.len()`.
#[cfg(all(feature = "simd_runtime_dispatch", target_arch = "aarch64"))]
fn count_non_ascii_neon(bytes: &[u8]) -> usize {
    let mut count = 0u32;
    let mut offset = 0;
    // SAFETY: all `vld1q_u8` loads are guarded by `offset + 16 <= bytes.len()`.
    unsafe {
        while offset + 16 <= bytes.len() {
            let chunk = vld1q_u8(bytes.as_ptr().add(offset));
            count += vaddvq_u8(vshrq_n_u8(chunk, 7)) as u32;
            offset += 16;
        }
    }
    for &b in &bytes[offset..] {
        count += (b >> 7) as u32;
    }
    count as usize
}

// ── Public API ──────────────────────────────────────────────────────────────

/// Counts the number of non-ASCII bytes (`≥ 0x80`) in `bytes` using the best
/// available SIMD kernel.
///
/// Used by [`super::engine::text_non_ascii_density`] for engine dispatch.
#[inline(always)]
pub(super) fn count_non_ascii_simd(bytes: &[u8]) -> usize {
    #[cfg(all(feature = "simd_runtime_dispatch", target_arch = "x86_64"))]
    return (dispatch().count_non_ascii)(bytes);

    #[cfg(all(feature = "simd_runtime_dispatch", target_arch = "aarch64"))]
    return count_non_ascii_neon(bytes);

    #[cfg(not(all(
        feature = "simd_runtime_dispatch",
        any(target_arch = "x86_64", target_arch = "aarch64")
    )))]
    count_non_ascii_portable(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pure_ascii() {
        assert_eq!(count_non_ascii_simd(b"hello world 123"), 0);
    }

    #[test]
    fn pure_non_ascii() {
        let text = "你好世界".as_bytes(); // 12 bytes, all ≥ 0x80
        assert_eq!(count_non_ascii_simd(text), text.len());
    }

    #[test]
    fn mixed() {
        let text = "hello世界".as_bytes(); // 5 ASCII + 6 non-ASCII
        assert_eq!(count_non_ascii_simd(text), 6);
    }

    #[test]
    fn empty() {
        assert_eq!(count_non_ascii_simd(b""), 0);
    }

    #[test]
    fn single_byte() {
        assert_eq!(count_non_ascii_simd(b"a"), 0);
        assert_eq!(count_non_ascii_simd(&[0x80]), 1);
        assert_eq!(count_non_ascii_simd(&[0xFF]), 1);
    }

    #[test]
    fn boundary_at_simd_width() {
        // Exactly 32 bytes (one SIMD chunk on portable/AVX2)
        let ascii_32 = b"abcdefghijklmnopqrstuvwxyz012345";
        assert_eq!(count_non_ascii_simd(ascii_32), 0);

        // 33 bytes — one full chunk + scalar tail
        let mut buf = vec![0x80u8; 33];
        assert_eq!(count_non_ascii_simd(&buf), 33);
        buf[32] = b'a';
        assert_eq!(count_non_ascii_simd(&buf), 32);
    }
}
