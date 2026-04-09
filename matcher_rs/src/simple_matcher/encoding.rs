//! Direct-rule encoding for the Aho-Corasick automaton hot path.
//!
//! When a deduplicated pattern is attached to exactly one non-matrix rule, its
//! automaton value is bit-packed via [`DirectValue`] so the scan hot path can
//! skip the entry-table indirection entirely.
//!
//! Use [`DirectValue::encode`] to pack and [`DirectValue::decode`] to unpack.
//! The bit layout is internal — callers only interact with the enum variants.

/// High bit flag marking a direct-encoded automaton value.
///
/// Callers check this before calling [`DirectValue::decode`]. Values without
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

/// Decoded direct-rule value from the AC automaton.
///
/// Each variant corresponds to a different rule shape, carrying only the
/// fields needed for that shape's inline evaluation in `process_match`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DirectValue {
    /// Single-segment rule, no NOT. Just mark positive.
    SingleAnd { rule_idx: usize },
    /// AND entry of a single-segment rule with NOT. Mark positive, no
    /// early-exit.
    SingleAndNot { rule_idx: usize },
    /// AND entry of a multi-segment bitmask rule (with or without NOT).
    BitmaskAnd {
        rule_idx: usize,
        offset: usize,
        and_count: u16,
        has_not: bool,
    },
    /// NOT entry (non-matrix). Sets veto generation.
    Not { rule_idx: usize, and_count: u16 },
}

impl DirectValue {
    // --- Internal bit layout ---
    //
    // Common prefix (all kinds):
    //   [31: flag] [30-29: kind(2)] [28-26: pt(3)] [25-24: boundary(2)]
    //
    // Payload (bits 23-0, varies by kind):
    //   00 SingleAnd:    rule_idx (24b, max 16M)
    //   01 SingleAndNot: rule_idx (24b, max 16M)
    //   10 BitmaskAnd:   has_not(1) offset(5) and_count(4) rule_idx(14)
    //   11 Not:          and_count(5) rule_idx(19)

    const KIND_SHIFT: u32 = 29;
    const PT_SHIFT: u32 = 26;
    const PT_MASK: u32 = 0x07 << Self::PT_SHIFT;
    const BOUNDARY_SHIFT: u32 = 24;
    const BOUNDARY_MASK: u32 = 0x03 << Self::BOUNDARY_SHIFT;

    // Kind 00/01: SingleAnd / SingleAndNot
    const SINGLE_RULE_MAX: u32 = (1 << Self::BOUNDARY_SHIFT) - 1;

    // Kind 10: BitmaskAnd
    const BM_HAS_NOT_BIT: u32 = 1 << 23;
    const BM_OFFSET_SHIFT: u32 = 18;
    const BM_COUNT_SHIFT: u32 = 14;
    const BM_RULE_MAX: u32 = (1 << Self::BM_COUNT_SHIFT) - 1;

    // Kind 11: Not
    const NOT_COUNT_SHIFT: u32 = 19;
    const NOT_RULE_MAX: u32 = (1 << Self::NOT_COUNT_SHIFT) - 1;

    /// Packs this value into a `u32` with [`DIRECT_RULE_BIT`] set.
    ///
    /// Returns `None` if any field exceeds the bit-width limit for its kind,
    /// or if `pt_index >= 8`.
    #[inline(always)]
    pub(super) fn encode(self, pt_index: u8, boundary: u8) -> Option<u32> {
        if pt_index >= 8 {
            return None;
        }
        let prefix = DIRECT_RULE_BIT
            | ((pt_index as u32) << Self::PT_SHIFT)
            | ((boundary as u32) << Self::BOUNDARY_SHIFT);

        match self {
            Self::SingleAnd { rule_idx } => {
                if rule_idx as u32 > Self::SINGLE_RULE_MAX {
                    return None;
                }
                Some(prefix | (0 << Self::KIND_SHIFT) | rule_idx as u32)
            }
            Self::SingleAndNot { rule_idx } => {
                if rule_idx as u32 > Self::SINGLE_RULE_MAX {
                    return None;
                }
                Some(prefix | (1 << Self::KIND_SHIFT) | rule_idx as u32)
            }
            Self::BitmaskAnd {
                rule_idx,
                offset,
                and_count,
                has_not,
            } => {
                if rule_idx as u32 > Self::BM_RULE_MAX
                    || offset as u32 >= 32
                    || and_count as u32 >= 16
                {
                    return None;
                }
                let not_bit = if has_not { Self::BM_HAS_NOT_BIT } else { 0 };
                Some(
                    prefix
                        | (2 << Self::KIND_SHIFT)
                        | not_bit
                        | ((offset as u32) << Self::BM_OFFSET_SHIFT)
                        | ((and_count as u32) << Self::BM_COUNT_SHIFT)
                        | rule_idx as u32,
                )
            }
            Self::Not {
                rule_idx,
                and_count,
            } => {
                if rule_idx as u32 > Self::NOT_RULE_MAX || and_count as u32 >= 32 {
                    return None;
                }
                Some(
                    prefix
                        | (3 << Self::KIND_SHIFT)
                        | ((and_count as u32) << Self::NOT_COUNT_SHIFT)
                        | rule_idx as u32,
                )
            }
        }
    }

    /// Unpacks a raw `u32` into `(pt_index, boundary, DirectValue)`.
    ///
    /// The caller **must** have verified `raw & DIRECT_RULE_BIT != 0`.
    #[inline(always)]
    pub(super) fn decode(raw: u32) -> (u8, u8, Self) {
        let pt_index = ((raw & Self::PT_MASK) >> Self::PT_SHIFT) as u8;
        let boundary = ((raw & Self::BOUNDARY_MASK) >> Self::BOUNDARY_SHIFT) as u8;
        let kind = (raw >> Self::KIND_SHIFT) & 0x03;

        let value = match kind {
            0 => Self::SingleAnd {
                rule_idx: (raw & Self::SINGLE_RULE_MAX) as usize,
            },
            1 => Self::SingleAndNot {
                rule_idx: (raw & Self::SINGLE_RULE_MAX) as usize,
            },
            2 => Self::BitmaskAnd {
                rule_idx: (raw & Self::BM_RULE_MAX) as usize,
                offset: ((raw >> Self::BM_OFFSET_SHIFT) & 0x1F) as usize,
                and_count: ((raw >> Self::BM_COUNT_SHIFT) & 0x0F) as u16,
                has_not: raw & Self::BM_HAS_NOT_BIT != 0,
            },
            _ => Self::Not {
                rule_idx: (raw & Self::NOT_RULE_MAX) as usize,
                and_count: ((raw >> Self::NOT_COUNT_SHIFT) & 0x1F) as u16,
            },
        };

        (pt_index, boundary, value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_single_and() {
        let dv = DirectValue::SingleAnd { rule_idx: 12345 };
        let raw = dv.encode(3, 1).unwrap();
        assert!(raw & DIRECT_RULE_BIT != 0);
        let (pt, bd, decoded) = DirectValue::decode(raw);
        assert_eq!(pt, 3);
        assert_eq!(bd, 1);
        assert_eq!(decoded, dv);
    }

    #[test]
    fn roundtrip_single_and_not() {
        let dv = DirectValue::SingleAndNot { rule_idx: 99 };
        let raw = dv.encode(0, 2).unwrap();
        let (pt, bd, decoded) = DirectValue::decode(raw);
        assert_eq!(pt, 0);
        assert_eq!(bd, 2);
        assert_eq!(decoded, dv);
    }

    #[test]
    fn roundtrip_bitmask_and() {
        let dv = DirectValue::BitmaskAnd {
            rule_idx: 42,
            offset: 3,
            and_count: 5,
            has_not: true,
        };
        let raw = dv.encode(2, 0).unwrap();
        let (pt, bd, decoded) = DirectValue::decode(raw);
        assert_eq!(pt, 2);
        assert_eq!(bd, 0);
        assert_eq!(decoded, dv);
    }

    #[test]
    fn roundtrip_not() {
        let dv = DirectValue::Not {
            rule_idx: 500,
            and_count: 3,
        };
        let raw = dv.encode(1, 0).unwrap();
        let (pt, bd, decoded) = DirectValue::decode(raw);
        assert_eq!(pt, 1);
        assert_eq!(bd, 0);
        assert_eq!(decoded, dv);
    }

    #[test]
    fn overflow_returns_none() {
        assert!(
            DirectValue::SingleAnd {
                rule_idx: usize::MAX
            }
            .encode(0, 0)
            .is_none()
        );
        assert!(
            DirectValue::SingleAnd { rule_idx: 0 }
                .encode(8, 0)
                .is_none()
        );
        assert!(
            DirectValue::BitmaskAnd {
                rule_idx: 20000,
                offset: 0,
                and_count: 0,
                has_not: false,
            }
            .encode(0, 0)
            .is_none()
        );
        assert!(
            DirectValue::BitmaskAnd {
                rule_idx: 0,
                offset: 32,
                and_count: 0,
                has_not: false,
            }
            .encode(0, 0)
            .is_none()
        );
        assert!(
            DirectValue::Not {
                rule_idx: 0,
                and_count: 32,
            }
            .encode(0, 0)
            .is_none()
        );
    }
}
