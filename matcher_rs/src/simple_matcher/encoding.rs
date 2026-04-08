//! Bit-packing constants for direct-rule encoding and capacity limits.
//!
//! The Aho-Corasick automaton stores a `u32` value for each deduplicated
//! pattern. When a pattern is attached to exactly one
//! [`PatternKind::Simple`](super::pattern::PatternKind::Simple) rule, the value
//! is bit-packed to encode the rule index, process-type index, and
//! word-boundary flags directly — avoiding indirection through the entry table
//! on the hot path.
//!
//! ```text
//! Bit 31:     DIRECT_RULE_BIT flag
//! Bits 28-30: pt_index (3 bits, max 7)
//! Bits 26-27: boundary (2 bits: bit 26 = left, bit 27 = right)
//! Bits 0-25:  rule_idx (26 bits, max ~67M rules)
//! ```

/// High bit used to encode the direct-rule fast path in raw scan values.
///
/// When a deduplicated pattern is attached to exactly one
/// [`PatternKind::Simple`](super::pattern::PatternKind::Simple)
/// rule, the automaton stores an encoded value with this bit set so that
/// callers can extract `rule_idx`, `pt_index`, and `boundary` inline without
/// the entry table indirection.
pub(super) const DIRECT_RULE_BIT: u32 = 1 << 31;

/// Bit shift for the process-type index inside a direct-rule encoded value.
pub(super) const DIRECT_PT_SHIFT: u32 = 28;

/// Mask for extracting the process-type index from a direct-rule encoded value.
pub(super) const DIRECT_PT_MASK: u32 = 0x07 << DIRECT_PT_SHIFT;

/// Bit shift for boundary flags inside a direct-rule encoded value.
pub(super) const DIRECT_BOUNDARY_SHIFT: u32 = 26;

/// Mask for extracting boundary flags from a direct-rule encoded value.
pub(super) const DIRECT_BOUNDARY_MASK: u32 = 0x03 << DIRECT_BOUNDARY_SHIFT;

/// Mask for extracting the rule index from a direct-rule encoded value.
pub(super) const DIRECT_RULE_MASK: u32 = (1 << DIRECT_BOUNDARY_SHIFT) - 1;

/// Maximum number of segments handled by the bitmask fast path.
///
/// Rules with up to 64 AND/NOT segments track per-segment satisfaction in a
/// single `u64` bitmask (`WordState::satisfied_mask`). Rules exceeding this
/// threshold fall back to the per-variant counter matrix
/// (`SimpleMatchState::matrix`).
pub(super) const BITMASK_CAPACITY: usize = 64;

/// Size of the compact process-type lookup table indexed by raw
/// [`ProcessType`](crate::process::ProcessType) bits.
///
/// [`ProcessType`](crate::process::ProcessType) is a 7-bit bitflag, so `2^7 =
/// 128` covers every possible combination. The table maps each bitflag value to
/// a dense sequential index used in the scan masks.
pub(super) const PROCESS_TYPE_TABLE_SIZE: usize = 128;
