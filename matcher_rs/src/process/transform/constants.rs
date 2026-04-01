//! Pre-compiled and source-text constants for text-transformation pipelines.
//!
//! All items are conditional on feature flags:
//!
//! - `runtime_build` — exposes raw text-map string constants (`FANJIAN`, `TEXT_DELETE`, etc.)
//!   that are parsed on first use to build transformation tables dynamically.
//! - default (`not(runtime_build)`) — exposes pre-compiled binary constants (`*_L1_BYTES`,
//!   `*_L2_BYTES`, `*_BYTES`, `*_STR`) embedded at build time by `build.rs` and decoded
//!   lazily when the corresponding matcher is first requested.

// ── runtime_build: source text maps ─────────────────────────────────────────

/// Tab-separated `(traditional, simplified)` codepoint pairs, one per line.
///
/// Used by the step registry under `runtime_build` to
/// build the Fanjian 2-stage page table at startup.
#[cfg(feature = "runtime_build")]
pub(crate) const FANJIAN: &str = include_str!("../../../process_map/FANJIAN.txt");

/// Newline-separated `U+XXXX` codepoint tokens that should be removed by the Delete step.
///
/// Used under `runtime_build` to populate the Delete BitSet.
#[cfg(feature = "runtime_build")]
pub(crate) const TEXT_DELETE: &str = include_str!("../../../process_map/TEXT-DELETE.txt");

/// Tab-separated `(source, normalized)` pairs for digit/number normalization.
///
/// Merged with [`NORM`] to build the Normalize Aho-Corasick automaton under `runtime_build`.
#[cfg(feature = "runtime_build")]
pub(crate) const NUM_NORM: &str = include_str!("../../../process_map/NUM-NORM.txt");

/// Tab-separated `(source, normalized)` pairs for general Unicode normalization
/// (full-width→half-width, variant forms, etc.).
///
/// Merged with [`NUM_NORM`] to build the Normalize automaton under `runtime_build`.
#[cfg(feature = "runtime_build")]
pub(crate) const NORM: &str = include_str!("../../../process_map/NORM.txt");

/// Tab-separated `(character, pinyin_with_spaces)` pairs covering CJK codepoints.
///
/// Used under `runtime_build` to build the Pinyin 2-stage page table and string buffer.
#[cfg(feature = "runtime_build")]
pub(crate) const PINYIN: &str = include_str!("../../../process_map/PINYIN.txt");

// ── default build: pre-compiled normalization automaton ──────────────────────

/// Newline-separated source patterns for the Normalize matcher.
///
/// Loaded via `include_str!` from the `OUT_DIR` artifact produced by `build.rs`.
/// Used when `runtime_build` is disabled.
#[cfg(not(feature = "runtime_build"))]
pub(crate) const NORMALIZE_PROCESS_LIST_STR: &str =
    include_str!(concat!(env!("OUT_DIR"), "/normalize_process_list.bin"));

/// Newline-separated replacement strings parallel to the Normalize pattern list.
///
/// Index `i` is the replacement for pattern `i` in `NORMALIZE_PROCESS_LIST_STR`. Loaded
/// from `OUT_DIR`.
#[cfg(not(feature = "runtime_build"))]
pub(crate) const NORMALIZE_PROCESS_REPLACE_LIST_STR: &str = include_str!(concat!(
    env!("OUT_DIR"),
    "/normalize_process_replace_list.bin"
));

// ── default build: Fanjian page tables ──────────────────────────────────────

/// L1 index for the Fanjian 2-stage page table (`u16[4352]`, little-endian).
///
/// See [`crate::process::transform::replace::FanjianMatcher`]
/// for the table layout.
#[cfg(not(feature = "runtime_build"))]
pub(crate) const FANJIAN_L1_BYTES: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/fanjian_l1.bin"));

/// L2 data for the Fanjian 2-stage page table (`u32[num_pages * 256]`, little-endian).
#[cfg(not(feature = "runtime_build"))]
pub(crate) const FANJIAN_L2_BYTES: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/fanjian_l2.bin"));

// ── default build: Pinyin page tables ───────────────────────────────────────

/// L1 index for the Pinyin 2-stage page table (`u16[4352]`, little-endian).
#[cfg(not(feature = "runtime_build"))]
pub(crate) const PINYIN_L1_BYTES: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/pinyin_l1.bin"));

/// L2 data for the Pinyin 2-stage page table (`u32[num_pages * 256]`, little-endian).
///
/// Each entry packs `(offset << 8) | length` into a `u32`, pointing into [`PINYIN_STR_BYTES`].
#[cfg(not(feature = "runtime_build"))]
pub(crate) const PINYIN_L2_BYTES: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/pinyin_l2.bin"));

/// Concatenated Pinyin syllable strings referenced by [`PINYIN_L2_BYTES`].
///
/// Individual mappings may include surrounding spaces; `PinYinChar` trims those boundaries
/// after lookup.
#[cfg(not(feature = "runtime_build"))]
pub(crate) const PINYIN_STR_BYTES: &str = include_str!(concat!(env!("OUT_DIR"), "/pinyin_str.bin"));

// ── default build: Delete BitSet ─────────────────────────────────────────────

/// Flat 139 KB bitset (`u8[139264]`) covering all Unicode codepoints 0x0–0x10FFFF.
///
/// Bit `cp % 8` of byte `cp / 8` is set when codepoint `cp` should be removed by the
/// Delete step. Generated at build time from `TEXT-DELETE.txt`.
#[cfg(not(feature = "runtime_build"))]
pub(crate) const DELETE_BITSET_BYTES: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/delete_bitset.bin"));
