//! Direct-rule encoding for the Aho-Corasick automaton hot path.
//!
//! When a deduplicated pattern is attached to exactly one non-matrix rule, its
//! automaton value is bit-packed via [`encode_direct`] so the scan hot path can
//! skip the entry-table indirection entirely.
//!
//! ## Bit layout
//!
//! One uniform format for all non-matrix rules:
//!
//! ```text
//! [31: DIRECT] [30: kind(1)] [29-27: pt_index(3)] [26-25: boundary(2)] [24-19: offset(6)] [18-0: rule_idx(19)]
//! ```
//!
//! - `kind`: 0 = AND, 1 = NOT
//! - `pt_index`: compact process-type index (max 7)
//! - `boundary`: word boundary flags (bit 0 = left, bit 1 = right)
//! - `offset`: segment offset within the rule (max 63)
//! - `rule_idx`: rule index in [`RuleSet`](super::rule::RuleSet) (max 524287)

use super::pattern::PatternKind;

/// High bit flag marking a direct-encoded automaton value.
///
/// Callers check this before calling [`decode_direct`]. Values without
/// this bit are indirect indices into the pattern entry table.
pub(super) const DIRECT_RULE_BIT: u32 = 1 << 31;

/// Maximum number of segments handled by the bitmask fast path.
///
/// Rules with up to 64 AND/NOT segments track per-segment satisfaction in a
/// single `u64` bitmask. Rules exceeding this fall back to the per-variant
/// counter matrix.
pub(super) const BITMASK_CAPACITY: usize = 64;

/// Size of the compact process-type lookup table (2^7 = 128 covers all
/// 7-bit [`ProcessType`](crate::process::ProcessType) combinations).
pub(super) const PROCESS_TYPE_TABLE_SIZE: usize = 128;

// --- Unified bit layout constants ---

const KIND_SHIFT: u32 = 30;
const PT_SHIFT: u32 = 27;
const PT_MASK: u32 = 0x07 << PT_SHIFT;
const BOUNDARY_SHIFT: u32 = 25;
const BOUNDARY_MASK: u32 = 0x03 << BOUNDARY_SHIFT;
const OFFSET_SHIFT: u32 = 19;
const OFFSET_MASK: u32 = 0x3F << OFFSET_SHIFT;
const RULE_IDX_MASK: u32 = (1 << OFFSET_SHIFT) - 1; // 19 bits, max 524287

/// Packs a direct rule hit into a `u32` with [`DIRECT_RULE_BIT`] set.
///
/// Returns `None` if any field overflows its bit width:
/// - `pt_index` must be < 8
/// - `offset` must be < 64
/// - `rule_idx` must be < 524288
#[inline(always)]
pub(super) fn encode_direct(
    pt_index: u8,
    boundary: u8,
    kind: PatternKind,
    offset: u8,
    rule_idx: u32,
) -> Option<u32> {
    if pt_index >= 8 || offset >= 64 || rule_idx > RULE_IDX_MASK {
        return None;
    }
    let kind_bit = match kind {
        PatternKind::And => 0,
        PatternKind::Not => 1u32 << KIND_SHIFT,
    };
    Some(
        DIRECT_RULE_BIT
            | kind_bit
            | ((pt_index as u32) << PT_SHIFT)
            | ((boundary as u32) << BOUNDARY_SHIFT)
            | ((offset as u32) << OFFSET_SHIFT)
            | rule_idx,
    )
}

/// Unpacks a direct-encoded `u32` into `(pt_index, boundary, kind, offset,
/// rule_idx)`.
///
/// The caller **must** have verified `raw & DIRECT_RULE_BIT != 0`.
#[inline(always)]
pub(super) fn decode_direct(raw: u32) -> (u8, u8, PatternKind, usize, usize) {
    let pt_index = ((raw & PT_MASK) >> PT_SHIFT) as u8;
    let boundary = ((raw & BOUNDARY_MASK) >> BOUNDARY_SHIFT) as u8;
    let kind = if (raw >> KIND_SHIFT) & 1 == 0 {
        PatternKind::And
    } else {
        PatternKind::Not
    };
    let offset = ((raw & OFFSET_MASK) >> OFFSET_SHIFT) as usize;
    let rule_idx = (raw & RULE_IDX_MASK) as usize;
    (pt_index, boundary, kind, offset, rule_idx)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        // (pt_index, boundary, kind, offset, rule_idx)
        let cases: &[(u8, u8, PatternKind, u8, u32)] = &[
            (3, 1, PatternKind::And, 0, 12345),
            (1, 0, PatternKind::Not, 5, 500),
            (2, 0, PatternKind::And, 31, 42),
            // Max values
            (7, 3, PatternKind::Not, 63, RULE_IDX_MASK),
        ];
        for &(pt, bd, kind, off, idx) in cases {
            let raw = encode_direct(pt, bd, kind, off, idx).unwrap();
            assert!(raw & DIRECT_RULE_BIT != 0);
            let (dpt, dbd, dkind, doff, didx) = decode_direct(raw);
            assert_eq!(
                (dpt, dbd, dkind, doff, didx),
                (pt, bd, kind, off as usize, idx as usize)
            );
        }
    }

    #[test]
    fn overflow_returns_none() {
        assert!(encode_direct(8, 0, PatternKind::And, 0, 0).is_none());
        assert!(encode_direct(0, 0, PatternKind::And, 64, 0).is_none());
        assert!(encode_direct(0, 0, PatternKind::And, 0, RULE_IDX_MASK + 1).is_none());
    }
}
