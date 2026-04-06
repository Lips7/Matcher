//! Deduplicated pattern storage and dispatch for automaton hits.
//!
//! During construction, each user-supplied rule string is split into sub-patterns that are
//! deduplicated across all rules. Each unique sub-pattern gets one automaton entry, and one
//! or more [`PatternEntry`] records that map hits back to the rule segments they belong to.
//!
//! The [`PatternIndex`] flattens all per-pattern entry buckets into contiguous storage for
//! cache-friendly iteration, and [`PatternDispatch`] provides the hot-path dispatch API.

use super::encoding::{DIRECT_BOUNDARY_SHIFT, DIRECT_PT_SHIFT, DIRECT_RULE_BIT};
use super::rule::RuleShape;

/// Logical role of one emitted pattern inside a rule.
///
/// Determined at construction time by the operator that precedes the sub-pattern
/// in the original rule string:
///
/// - No operator or the first segment of a single-segment rule → [`Simple`](Self::Simple)
/// - `&` operator → [`And`](Self::And)
/// - `~` operator → [`Not`](Self::Not)
///
/// `repr(u8)` keeps this type small for dense storage in [`PatternEntry`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub(super) enum PatternKind {
    /// Single-fragment rule that can complete on one hit.
    ///
    /// Only used when the rule has exactly one positive segment, no NOT segments,
    /// and does not need the matrix fallback.
    Simple = 0,
    /// Positive segment that must be observed.
    ///
    /// All AND segments in a rule must be satisfied (across any text variant)
    /// before the rule can fire.
    And = 1,
    /// Negative segment that vetoes the rule when observed.
    ///
    /// If any NOT segment is matched in any variant, the rule is permanently
    /// vetoed for the current scan generation.
    Not = 2,
}

/// One deduplicated pattern's attachment to a concrete rule segment.
///
/// Multiple rules may share the same deduplicated pattern string (e.g., two rules both
/// contain the sub-pattern `"hello"`). Each such binding is stored as a separate
/// `PatternEntry` in the same bucket of the [`PatternIndex`].
///
/// Size: 12 bytes (u32 + 5×u8 + padding).
#[derive(Debug, Clone)]
pub(super) struct PatternEntry {
    /// Rule index inside [`RuleSet`](super::rule::RuleSet) (indexes into the hot/cold `Vec`s).
    pub(super) rule_idx: u32,
    /// Segment offset within the rule's [`RuleHot::segment_counts`](super::rule::RuleHot::segment_counts) array.
    ///
    /// For AND segments this is `0..and_count`; for NOT segments it is `and_count..`.
    /// Maximum 255 segments per rule (far exceeds [`BITMASK_CAPACITY`](super::encoding::BITMASK_CAPACITY) of 64).
    pub(super) offset: u8,
    /// Compact process-type index assigned by `SimpleMatcher::build_pt_index_table`.
    ///
    /// Used to filter pattern hits by comparing against the current variant's
    /// [`ScanContext::process_type_mask`](super::state::ScanContext::process_type_mask).
    pub(super) pt_index: u8,
    /// Logical role of this segment hit.
    pub(super) kind: PatternKind,
    /// Pre-resolved rule shape encoding `use_matrix`, `and_count == 1`, and `has_not`.
    ///
    /// Lets [`RuleSet::process_entry`](super::rule::RuleSet::process_entry) branch on rule
    /// properties without touching the `hot` array (only needed on first-touch in
    /// `ScanState::init_rule`).
    pub(super) shape: RuleShape,
    /// Word boundary flags (bit 0 = left `\b`, bit 1 = right `\b`).
    ///
    /// When non-zero, the scan dispatch checks `is_word_byte` at match start/end
    /// before forwarding the hit to [`RuleSet::process_entry`](super::rule::RuleSet::process_entry).
    pub(super) boundary: u8,
    /// Number of positive (AND) segments in the owning rule.
    ///
    /// Duplicated from the rule's AND-segment count so that
    /// [`RuleSet::process_entry`](super::rule::RuleSet::process_entry) can initialize
    /// per-rule state without loading the `RuleHot` struct (avoiding a cache miss on
    /// the 400KB+ hot array). Fits in the existing struct padding (9→10 bytes, still
    /// padded to 12).
    pub(super) and_count: u8,
}

/// Flat storage for deduplicated pattern entries plus their original bucket ranges.
///
/// During construction, each unique pattern string may be attached to one or more
/// [`PatternEntry`] values (one per rule segment that uses that string). Those per-pattern
/// buckets are flattened into a single contiguous `entries` vec, and `ranges` records the
/// `(start, len)` slice for each deduplicated pattern id.
///
/// The automaton raw value for a given pattern is either:
/// - A deduplicated index into `ranges` (general case), or
/// - A direct rule index with [`DIRECT_RULE_BIT`] set (fast path for simple single-entry
///   patterns).
#[derive(Clone)]
pub(super) struct PatternIndex {
    /// Contiguous storage of all pattern entries across all deduplicated patterns.
    entries: Vec<PatternEntry>,
    /// `(start_offset, length)` into `entries` for each deduplicated pattern id.
    ranges: Vec<(usize, usize)>,
    /// `true` when at least one entry has non-zero boundary flags.
    has_boundary: bool,
}

/// Dispatch result for a non-direct raw scan value.
///
/// Returned by [`PatternIndex::dispatch_indirect`] for values that do **not** have
/// [`DIRECT_RULE_BIT`] set. Callers handle direct-rule values inline (checking
/// `DIRECT_RULE_BIT` and extracting `rule_idx` / `pt_index` from the bit-packed
/// value) before falling through to `dispatch_indirect` for the remaining cases.
pub(super) enum PatternDispatch<'a> {
    /// Exactly one attached pattern entry.
    SingleEntry(&'a PatternEntry),
    /// Multiple attached entries sharing the same deduplicated pattern string.
    Entries(&'a [PatternEntry]),
}

/// Pattern-dispatch helpers for the compiled deduplicated index.
impl PatternIndex {
    /// Flattens per-pattern entry buckets into contiguous storage and records their ranges.
    ///
    /// Each element of `dedup_entries` is the set of [`PatternEntry`] values attached to
    /// one unique pattern string. After flattening, `ranges[dedup_id]` gives the
    /// `(start, len)` slice into the flat `entries` vec.
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

    /// Returns whether any entry requires word boundary checking.
    pub(super) fn has_boundary(&self) -> bool {
        self.has_boundary
    }

    /// Returns the estimated heap memory in bytes owned by the pattern index.
    pub(super) fn heap_bytes(&self) -> usize {
        self.entries.capacity() * size_of::<PatternEntry>()
            + self.ranges.capacity() * size_of::<(usize, usize)>()
    }

    /// Returns whether there are no deduplicated patterns to scan.
    #[inline(always)]
    pub(super) fn is_empty(&self) -> bool {
        self.ranges.is_empty()
    }

    /// Returns whether every entry across all patterns is a [`PatternKind::Simple`] segment
    /// and every pattern maps to exactly one rule.
    ///
    /// When true, the matcher can use [`AllSimple`](super::SearchMode::AllSimple)
    /// which skips the full state machine and processes every hit as a completed rule.
    ///
    /// The single-entry requirement exists because the AllSimple fast path extracts
    /// `rule_idx` directly from the raw scan value via [`super::encoding::DIRECT_RULE_MASK`]. Patterns
    /// shared across multiple rules (e.g., via OR alternatives `"cat|dog"` + `"dog|bird"`)
    /// produce multi-entry buckets that require the General dispatch path.
    #[inline(always)]
    pub(super) fn all_simple(&self) -> bool {
        self.entries
            .iter()
            .all(|entry| entry.kind == PatternKind::Simple)
            && self.ranges.iter().all(|&(_, len)| len == 1)
    }

    /// Builds the raw scan-value mapping used by the automata.
    ///
    /// For each deduplicated pattern, produces the `u32` value that the automaton will
    /// report on a hit. A pattern with exactly one [`PatternKind::Simple`] entry is encoded as
    /// `rule_idx | DIRECT_RULE_BIT` so the hot path can skip the indirection through the
    /// entry table. All other patterns store the deduplicated index directly.
    ///
    /// # Safety
    ///
    /// Uses `get_unchecked` on `self.entries` when checking the single-entry fast path.
    /// The index `start` comes from `self.ranges` which was built by [`Self::new`] and
    /// is always in bounds.
    pub(super) fn build_value_map(&self) -> Vec<u32> {
        let mut value_map = Vec::with_capacity(self.ranges.len());

        for (dedup_idx, &(start, len)) in self.ranges.iter().enumerate() {
            if len == 1 {
                // SAFETY: `start` is in bounds — sourced from `self.ranges`, built by `Self::new`.
                let entry = unsafe { self.entries.get_unchecked(start) };
                if entry.kind == PatternKind::Simple
                    && (entry.pt_index as u32) < 8
                    && entry.rule_idx < (1 << DIRECT_BOUNDARY_SHIFT)
                {
                    let encoded = DIRECT_RULE_BIT
                        | ((entry.pt_index as u32) << DIRECT_PT_SHIFT)
                        | ((entry.boundary as u32) << DIRECT_BOUNDARY_SHIFT)
                        | entry.rule_idx;
                    value_map.push(encoded);
                    continue;
                }
            }
            value_map.push(dedup_idx as u32);
        }

        value_map
    }

    /// Dispatches a non-direct raw scan value into a [`PatternDispatch`] variant.
    ///
    /// The caller **must** have already checked that `raw_value & DIRECT_RULE_BIT == 0`.
    /// Direct-rule values are handled inline by the caller (extracting `rule_idx` and
    /// `pt_index` from the bit-packed value). This function handles the remaining cases
    /// where the value is a deduplicated pattern index into the entry table.
    #[inline(always)]
    pub(super) fn dispatch_indirect(&self, raw_value: u32) -> PatternDispatch<'_> {
        debug_assert!(
            raw_value & DIRECT_RULE_BIT == 0,
            "dispatch_indirect called with DIRECT_RULE_BIT set"
        );

        let pattern_idx = raw_value as usize;
        debug_assert!(pattern_idx < self.ranges.len());
        // SAFETY: `pattern_idx` is in bounds — guaranteed by debug_assert above.
        let &(start, len) = unsafe { self.ranges.get_unchecked(pattern_idx) };
        debug_assert!(start + len <= self.entries.len());

        if len == 1 {
            // SAFETY: `start` and `start + len` are in bounds — guaranteed by debug_assert above.
            PatternDispatch::SingleEntry(unsafe { self.entries.get_unchecked(start) })
        } else {
            // SAFETY: `start..start + len` is in bounds — guaranteed by debug_assert above.
            PatternDispatch::Entries(unsafe { self.entries.get_unchecked(start..start + len) })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::encoding::{DIRECT_PT_MASK, DIRECT_RULE_MASK};
    use super::*;

    #[test]
    fn test_pattern_index_direct_rule_encoding() {
        let entries = vec![vec![PatternEntry {
            rule_idx: 5,
            offset: 0,
            pt_index: 2,
            kind: PatternKind::Simple,
            shape: RuleShape::SingleAnd,
            boundary: 0,
            and_count: 1,
        }]];
        let index = PatternIndex::new(entries);
        let value_map = index.build_value_map();

        assert_eq!(value_map.len(), 1);
        let raw = value_map[0];
        assert!(raw & DIRECT_RULE_BIT != 0, "should set DIRECT_RULE_BIT");

        let rule_idx = (raw & DIRECT_RULE_MASK) as usize;
        let pt_index = ((raw & DIRECT_PT_MASK) >> DIRECT_PT_SHIFT) as u8;
        assert_eq!(rule_idx, 5);
        assert_eq!(pt_index, 2);
    }

    #[test]
    fn test_pattern_index_dispatch_single_entry() {
        // Non-Simple kind should NOT get DIRECT_RULE_BIT
        let entries = vec![vec![PatternEntry {
            rule_idx: 0,
            offset: 0,
            pt_index: 0,
            kind: PatternKind::And,
            shape: RuleShape::Bitmask,
            boundary: 0,
            and_count: 2,
        }]];
        let index = PatternIndex::new(entries);
        let value_map = index.build_value_map();

        assert!(
            value_map[0] & DIRECT_RULE_BIT == 0,
            "And kind should not get DIRECT_RULE_BIT"
        );

        match index.dispatch_indirect(value_map[0]) {
            PatternDispatch::SingleEntry(entry) => {
                assert_eq!(entry.rule_idx, 0);
                assert_eq!(entry.kind, PatternKind::And);
            }
            _ => panic!("expected SingleEntry dispatch"),
        }
    }

    #[test]
    fn test_pattern_index_dispatch_multi_entry() {
        let entries = vec![vec![
            PatternEntry {
                rule_idx: 0,
                offset: 0,
                pt_index: 0,
                kind: PatternKind::Simple,
                shape: RuleShape::SingleAnd,
                boundary: 0,
                and_count: 1,
            },
            PatternEntry {
                rule_idx: 1,
                offset: 0,
                pt_index: 0,
                kind: PatternKind::Simple,
                shape: RuleShape::SingleAnd,
                boundary: 0,
                and_count: 1,
            },
        ]];
        let index = PatternIndex::new(entries);
        let value_map = index.build_value_map();

        // Multi-entry patterns never get DIRECT_RULE_BIT
        assert!(value_map[0] & DIRECT_RULE_BIT == 0);

        match index.dispatch_indirect(value_map[0]) {
            PatternDispatch::Entries(slice) => assert_eq!(slice.len(), 2),
            _ => panic!("expected Entries dispatch"),
        }
    }
}
