//! Pre-compiled binary constants for text-transformation pipelines.
//!
//! All constants are binary artifacts embedded at build time by `build.rs`
//! and decoded lazily when the corresponding matcher is first requested.

// ── Normalize page tables ──────────────────────────────────────────────────

/// L1 index for the Normalize 2-stage page table (`u16[4352]`, little-endian).
pub(crate) const NORMALIZE_L1_BYTES: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/normalize_l1.bin"));

/// L2 data for the Normalize 2-stage page table (`u32[num_pages * 256]`, little-endian).
///
/// Each entry packs `(offset << 8) | length` into a `u32`, pointing into [`NORMALIZE_STR_BYTES`].
pub(crate) const NORMALIZE_L2_BYTES: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/normalize_l2.bin"));

/// Concatenated replacement strings referenced by [`NORMALIZE_L2_BYTES`].
pub(crate) const NORMALIZE_STR_BYTES: &str =
    include_str!(concat!(env!("OUT_DIR"), "/normalize_str.bin"));

// ── VariantNorm page tables ──────────────────────────────────────────────

/// L1 index for the VariantNorm 2-stage page table (`u16[4352]`, little-endian).
pub(crate) const VARIANT_NORM_L1_BYTES: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/variant_norm_l1.bin"));

/// L2 data for the VariantNorm 2-stage page table (`u32[num_pages * 256]`, little-endian).
pub(crate) const VARIANT_NORM_L2_BYTES: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/variant_norm_l2.bin"));

// ── Romanize page tables ─────────────────────────────────────────────────

/// L1 index for the Romanize 2-stage page table (`u16[4352]`, little-endian).
pub(crate) const ROMANIZE_L1_BYTES: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/romanize_l1.bin"));

/// L2 data for the Romanize 2-stage page table (`u32[num_pages * 256]`, little-endian).
///
/// Each entry packs `(offset << 8) | length` into a `u32`, pointing into [`ROMANIZE_STR_BYTES`].
pub(crate) const ROMANIZE_L2_BYTES: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/romanize_l2.bin"));

/// Concatenated romanization strings referenced by [`ROMANIZE_L2_BYTES`].
pub(crate) const ROMANIZE_STR_BYTES: &str =
    include_str!(concat!(env!("OUT_DIR"), "/romanize_str.bin"));

// ── Delete BitSet ──────────────────────────────────────────────────────────

/// Flat 139 KB bitset (`u8[139264]`) covering all Unicode codepoints 0x0–0x10FFFF.
///
/// Bit `cp % 8` of byte `cp / 8` is set when codepoint `cp` should be removed by the
/// Delete step.
pub(crate) const DELETE_BITSET_BYTES: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/delete_bitset.bin"));
