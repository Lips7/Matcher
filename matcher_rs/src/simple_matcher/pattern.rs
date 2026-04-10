//! Deduplicated pattern storage, direct-rule encoding, and dispatch for
//! automaton hits.
//!
//! During construction, each user-supplied rule string is split into
//! sub-patterns that are deduplicated across all rules. Each unique sub-pattern
//! gets one automaton entry, and one or more [`PatternEntry`] records that map
//! hits back to the rule segments they belong to.
//!
//! The [`PatternIndex`] flattens all per-pattern entry buckets into contiguous
//! storage for cache-friendly iteration, and [`PatternDispatch`] provides the
//! hot-path dispatch API.
//!
//! ## Direct-rule encoding
//!
//! When a deduplicated pattern is attached to exactly one non-matrix rule, its
//! automaton value is bit-packed via [`encode_direct`] so the scan hot path can
//! skip the entry-table indirection entirely.
//!
//! ### Bit layout
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

use super::rule::{RuleInfo, SatisfactionMethod};

// ===========================================================================
// Direct-rule encoding constants and functions
// ===========================================================================

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

// ===========================================================================
// Pattern types and dispatch
// ===========================================================================

/// Logical role of one emitted pattern inside a rule.
///
/// Determined at construction time by the operator that precedes the
/// sub-pattern in the original rule string:
///
/// - No operator, or `&` → [`And`](Self::And)
/// - `~` → [`Not`](Self::Not)
///
/// Single-segment rules without NOT use `SatisfactionMethod::SingleAnd` for the
/// simplified satisfaction path. The DIRECT bit-packing in `process_match`
/// handles these inline without consulting `PatternKind`.
///
/// `repr(u8)` keeps this type small for dense storage in [`PatternEntry`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub(super) enum PatternKind {
    /// Positive segment that must be observed.
    ///
    /// All AND segments in a rule must be satisfied (across any text variant)
    /// before the rule can fire. Single-segment rules also use this variant
    /// (with `SatisfactionMethod::SingleAnd`).
    And = 0,
    /// Negative segment that vetoes the rule when observed.
    ///
    /// If any NOT segment is matched in any variant, the rule is permanently
    /// vetoed for the current scan generation.
    Not = 1,
}

/// One deduplicated pattern's attachment to a concrete rule segment.
///
/// Multiple rules may share the same deduplicated pattern string (e.g., two
/// rules both contain the sub-pattern `"hello"`). Each such binding is stored
/// as a separate `PatternEntry` in the same bucket of the [`PatternIndex`].
///
/// Size: 8 bytes (u32 + 4×u8). Rule-level metadata (shape, and_count,
/// has_not) lives in [`RuleInfo`] — not duplicated per entry.
#[derive(Debug, Clone)]
pub(super) struct PatternEntry {
    /// Rule index inside [`RuleSet`](super::rule::RuleSet).
    pub(super) rule_idx: u32,
    /// Segment offset within the rule's
    /// [`Rule::segment_counts`](super::rule::Rule::segment_counts) array.
    pub(super) offset: u8,
    /// Compact process-type index assigned by
    /// `SimpleMatcher::build_pt_index_table`.
    pub(super) pt_index: u8,
    /// Logical role of this segment hit (AND or NOT).
    pub(super) kind: PatternKind,
    /// Word boundary flags (bit 0 = left `\b`, bit 1 = right `\b`).
    pub(super) boundary: u8,
}

/// Flat storage for deduplicated pattern entries plus their original bucket
/// ranges.
///
/// During construction, each unique pattern string may be attached to one or
/// more [`PatternEntry`] values (one per rule segment that uses that string).
/// Those per-pattern buckets are flattened into a single contiguous `entries`
/// vec, and `ranges` records the `(start, len)` slice for each deduplicated
/// pattern id.
///
/// The automaton raw value for a given pattern is either:
/// - A deduplicated index into `ranges` (general case), or
/// - A direct rule index with [`DIRECT_RULE_BIT`] set (fast path for simple
///   single-entry patterns).
#[derive(Clone)]
pub(super) struct PatternIndex {
    /// Contiguous storage of all pattern entries across all deduplicated
    /// patterns.
    entries: Vec<PatternEntry>,
    /// `(start_offset, length)` into `entries` for each deduplicated pattern
    /// id.
    ranges: Vec<(usize, usize)>,
    /// `true` when at least one entry has non-zero boundary flags.
    has_boundary: bool,
}

/// Dispatch result for a non-direct raw scan value.
///
/// Returned by `PatternIndex::dispatch_indirect` for values that do **not**
/// have [`DIRECT_RULE_BIT`] set. Callers handle direct-rule values inline
/// (checking `DIRECT_RULE_BIT` and extracting `rule_idx` / `pt_index` from the
/// bit-packed value) before falling through to `dispatch_indirect` for the
/// remaining cases.
pub(super) enum PatternDispatch<'a> {
    /// Exactly one attached pattern entry.
    SingleEntry(&'a PatternEntry),
    /// Multiple attached entries sharing the same deduplicated pattern string.
    Entries(&'a [PatternEntry]),
}

/// Pattern-dispatch helpers for the compiled deduplicated index.
impl PatternIndex {
    /// Flattens per-pattern entry buckets into contiguous storage and records
    /// their ranges.
    ///
    /// Each element of `dedup_entries` is the set of [`PatternEntry`] values
    /// attached to one unique pattern string. After flattening,
    /// `ranges[dedup_id]` gives the `(start, len)` slice into the flat
    /// `entries` vec.
    pub(super) fn new(dedup_entries: Vec<Vec<PatternEntry>>) -> Self {
        let mut entries = Vec::with_capacity(dedup_entries.iter().map(|bucket| bucket.len()).sum());
        let mut ranges = Vec::with_capacity(dedup_entries.len());

        for bucket in dedup_entries {
            let start = entries.len();
            let len = bucket.len();
            entries.extend(bucket);
            ranges.push((start, len));
        }

        let has_boundary = entries.iter().any(|e| e.boundary != 0);
        Self {
            entries,
            ranges,
            has_boundary,
        }
    }

    /// Returns the estimated heap memory in bytes owned by the pattern index.
    pub(super) fn heap_bytes(&self) -> usize {
        self.entries.capacity() * size_of::<PatternEntry>()
            + self.ranges.capacity() * size_of::<(usize, usize)>()
    }

    /// Returns whether any entry requires word boundary checking.
    pub(super) fn has_boundary(&self) -> bool {
        self.has_boundary
    }

    /// Returns whether every pattern maps to a single-entry immediate rule
    /// without NOT segments.
    ///
    /// When true and the transform tree has no children, `is_match` can
    /// delegate directly to the AC automaton — each hit is a completed rule.
    #[inline(always)]
    pub(super) fn all_single_and(&self, rule_info: &[RuleInfo]) -> bool {
        self.ranges.iter().all(|&(_, len)| len == 1)
            && self.entries.iter().all(|e| {
                let info = rule_info[e.rule_idx as usize];
                info.method == SatisfactionMethod::Immediate && !info.has_not
            })
    }

    /// Builds the raw scan-value mapping used by the automata.
    ///
    /// For each deduplicated pattern, produces the `u32` value that the
    /// automaton will report on a hit. Single-entry patterns that fit the
    /// direct encoding constraints are bit-packed with [`DIRECT_RULE_BIT`] set
    /// so the hot path skips entry-table indirection. All other patterns store
    /// the deduplicated index directly.
    ///
    /// # Safety
    ///
    /// Uses `get_unchecked` on `self.entries` when checking the single-entry
    /// fast path. The index `start` comes from `self.ranges` which was
    /// built by [`Self::new`] and is always in bounds.
    pub(super) fn build_value_map(&self, rule_info: &[RuleInfo]) -> Vec<u32> {
        let mut value_map = Vec::with_capacity(self.ranges.len());

        for (dedup_idx, &(start, len)) in self.ranges.iter().enumerate() {
            if len == 1 {
                // SAFETY: `start` is in bounds — sourced from `self.ranges`, built by
                // `Self::new`.
                let entry = unsafe { self.entries.get_unchecked(start) };
                if let Some(encoded) = Self::try_encode_direct(entry, rule_info) {
                    value_map.push(encoded);
                    continue;
                }
            }
            value_map.push(dedup_idx as u32);
        }

        value_map
    }

    /// Attempts to encode a single-entry pattern into a direct value.
    /// Returns `None` if the entry uses the matrix path or overflows bit
    /// widths.
    fn try_encode_direct(entry: &PatternEntry, rule_info: &[RuleInfo]) -> Option<u32> {
        if rule_info[entry.rule_idx as usize].method.use_matrix() {
            return None;
        }
        encode_direct(
            entry.pt_index,
            entry.boundary,
            entry.kind,
            entry.offset,
            entry.rule_idx,
        )
    }

    /// Dispatches a non-direct raw scan value into a [`PatternDispatch`]
    /// variant.
    ///
    /// The caller **must** have already checked that `raw_value &
    /// DIRECT_RULE_BIT == 0`. Direct-rule values are handled inline by the
    /// caller (extracting `rule_idx` and `pt_index` from the bit-packed
    /// value). This function handles the remaining cases where the value is
    /// a deduplicated pattern index into the entry table.
    #[inline(always)]
    pub(super) fn dispatch_indirect(&self, raw_value: u32) -> PatternDispatch<'_> {
        let pattern_idx = raw_value as usize;
        // SAFETY: caller guarantees DIRECT_RULE_BIT is not set; pattern_idx
        // and range bounds originate from construction with validated indices.
        let (start, len) = unsafe {
            core::hint::assert_unchecked(raw_value & DIRECT_RULE_BIT == 0);
            core::hint::assert_unchecked(pattern_idx < self.ranges.len());
            *self.ranges.get_unchecked(pattern_idx)
        };
        // SAFETY: range bounds validated during construction.
        unsafe { core::hint::assert_unchecked(start + len <= self.entries.len()) };

        if len == 1 {
            // SAFETY: `start` is in bounds — guaranteed by assert_unchecked above.
            PatternDispatch::SingleEntry(unsafe { self.entries.get_unchecked(start) })
        } else {
            // SAFETY: `start..start + len` is in bounds — guaranteed by assert_unchecked
            // above.
            PatternDispatch::Entries(unsafe { self.entries.get_unchecked(start..start + len) })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Encoding roundtrip tests ---

    #[test]
    fn encoding_roundtrip() {
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
    fn encoding_overflow_returns_none() {
        assert!(encode_direct(8, 0, PatternKind::And, 0, 0).is_none());
        assert!(encode_direct(0, 0, PatternKind::And, 64, 0).is_none());
        assert!(encode_direct(0, 0, PatternKind::And, 0, RULE_IDX_MASK + 1).is_none());
    }

    // --- Pattern dispatch tests ---

    fn entry(
        rule_idx: u32,
        offset: u8,
        pt_index: u8,
        kind: PatternKind,
        boundary: u8,
    ) -> PatternEntry {
        PatternEntry {
            rule_idx,
            offset,
            pt_index,
            kind,
            boundary,
        }
    }

    fn immediate_info() -> RuleInfo {
        RuleInfo {
            and_count: 1,
            method: SatisfactionMethod::Immediate,
            has_not: false,
        }
    }

    fn bitmask_info(and_count: u8) -> RuleInfo {
        RuleInfo {
            and_count,
            method: SatisfactionMethod::Bitmask,
            has_not: false,
        }
    }

    fn matrix_info(and_count: u8) -> RuleInfo {
        RuleInfo {
            and_count,
            method: SatisfactionMethod::Matrix,
            has_not: false,
        }
    }

    fn ri_for(rule_idx: u32, info: RuleInfo) -> Vec<RuleInfo> {
        let mut ri = vec![immediate_info(); rule_idx as usize + 1];
        ri[rule_idx as usize] = info;
        ri
    }

    type EncodingCase = (
        (u32, u8, u8, PatternKind, u8),
        RuleInfo,
        (u8, PatternKind, usize),
    );

    #[test]
    fn test_direct_encoding_variants() {
        let cases: &[EncodingCase] = &[
            // Single AND → Immediate
            (
                (5, 0, 2, PatternKind::And, 1),
                immediate_info(),
                (2, PatternKind::And, 5),
            ),
            // Single AND with has_not → Immediate
            (
                (100, 0, 3, PatternKind::And, 0),
                RuleInfo {
                    and_count: 1,
                    method: SatisfactionMethod::Immediate,
                    has_not: true,
                },
                (3, PatternKind::And, 100),
            ),
            // Bitmask AND with offset
            (
                (42, 1, 2, PatternKind::And, 0),
                bitmask_info(3),
                (2, PatternKind::And, 42),
            ),
            // NOT entry
            (
                (50, 1, 0, PatternKind::Not, 0),
                RuleInfo {
                    and_count: 1,
                    method: SatisfactionMethod::Immediate,
                    has_not: true,
                },
                (0, PatternKind::Not, 50),
            ),
        ];

        for &(ref e, ref ri, (exp_pt, exp_kind, exp_idx)) in cases {
            let entries = vec![vec![entry(e.0, e.1, e.2, e.3, e.4)]];
            let ri_vec = ri_for(e.0, *ri);
            let raw = PatternIndex::new(entries).build_value_map(&ri_vec)[0];
            assert!(raw & DIRECT_RULE_BIT != 0, "should use direct encoding");
            let (pt, _, kind, _, idx) = decode_direct(raw);
            assert_eq!((pt, kind, idx), (exp_pt, exp_kind, exp_idx));
        }
    }

    #[test]
    fn test_matrix_always_falls_back() {
        let entries = vec![vec![entry(0, 0, 0, PatternKind::And, 0)]];
        let ri = [matrix_info(2)];
        assert!(PatternIndex::new(entries).build_value_map(&ri)[0] & DIRECT_RULE_BIT == 0);
    }
}
