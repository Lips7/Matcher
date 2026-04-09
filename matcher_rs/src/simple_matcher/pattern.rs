//! Deduplicated pattern storage and dispatch for automaton hits.
//!
//! During construction, each user-supplied rule string is split into
//! sub-patterns that are deduplicated across all rules. Each unique sub-pattern
//! gets one automaton entry, and one or more [`PatternEntry`] records that map
//! hits back to the rule segments they belong to.
//!
//! The [`PatternIndex`] flattens all per-pattern entry buckets into contiguous
//! storage for cache-friendly iteration, and [`PatternDispatch`] provides the
//! hot-path dispatch API.

use super::{
    encoding::{DIRECT_RULE_BIT, encode_direct},
    rule::{RuleInfo, SatisfactionMethod},
};

/// Logical role of one emitted pattern inside a rule.
///
/// Determined at construction time by the operator that precedes the
/// sub-pattern in the original rule string:
///
/// - No operator, or `&` → [`And`](Self::And)
/// - `~` → [`Not`](Self::Not)
///
/// Single-segment rules without NOT use [`RuleShape::SingleAnd`] for the
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
    /// (with [`RuleShape::SingleAnd`]).
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
/// Returned by [`PatternIndex::dispatch_indirect`] for values that do **not**
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
    /// Four direct encoding kinds are supported — see [`super::encoding`] for
    /// the bit layout.
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
    use super::{super::encoding::decode_direct, *};

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

    /// Builds a rule_info vec with enough slots for the given rule_idx,
    /// filling unused slots with a default immediate info.
    fn ri_for(rule_idx: u32, info: RuleInfo) -> Vec<RuleInfo> {
        let mut ri = vec![immediate_info(); rule_idx as usize + 1];
        ri[rule_idx as usize] = info;
        ri
    }

    #[test]
    fn test_direct_single_and() {
        let entries = vec![vec![entry(5, 0, 2, PatternKind::And, 1)]];
        let ri = ri_for(5, immediate_info());
        let raw = PatternIndex::new(entries).build_value_map(&ri)[0];
        assert!(raw & DIRECT_RULE_BIT != 0);
        let (pt, bd, kind, off, idx) = decode_direct(raw);
        assert_eq!((pt, bd, kind, off, idx), (2, 1, PatternKind::And, 0, 5));
    }

    #[test]
    fn test_direct_single_and_not() {
        let entries = vec![vec![entry(100, 0, 3, PatternKind::And, 0)]];
        let ri = ri_for(
            100,
            RuleInfo {
                and_count: 1,
                method: SatisfactionMethod::Immediate,
                has_not: true,
            },
        );
        let raw = PatternIndex::new(entries).build_value_map(&ri)[0];
        assert!(raw & DIRECT_RULE_BIT != 0);
        let (pt, _, kind, _, idx) = decode_direct(raw);
        assert_eq!((pt, kind, idx), (3, PatternKind::And, 100));
    }

    #[test]
    fn test_direct_bitmask_and() {
        let entries = vec![vec![entry(42, 1, 2, PatternKind::And, 0)]];
        let ri = ri_for(42, bitmask_info(3));
        let raw = PatternIndex::new(entries).build_value_map(&ri)[0];
        assert!(raw & DIRECT_RULE_BIT != 0);
        let (pt, _, kind, off, idx) = decode_direct(raw);
        assert_eq!((pt, kind, off, idx), (2, PatternKind::And, 1, 42));
    }

    #[test]
    fn test_direct_not_entry() {
        let entries = vec![vec![entry(50, 1, 0, PatternKind::Not, 0)]];
        let ri = ri_for(
            50,
            RuleInfo {
                and_count: 1,
                method: SatisfactionMethod::Immediate,
                has_not: true,
            },
        );
        let raw = PatternIndex::new(entries).build_value_map(&ri)[0];
        assert!(raw & DIRECT_RULE_BIT != 0);
        let (_, _, kind, off, idx) = decode_direct(raw);
        assert_eq!((kind, off, idx), (PatternKind::Not, 1, 50));
    }

    #[test]
    fn test_bitmask_large_rule_idx_now_fits() {
        let entries = vec![vec![entry(40000, 0, 0, PatternKind::And, 0)]];
        let ri = ri_for(40000, bitmask_info(2));
        assert!(PatternIndex::new(entries).build_value_map(&ri)[0] & DIRECT_RULE_BIT != 0);
    }

    #[test]
    fn test_matrix_always_falls_back() {
        let entries = vec![vec![entry(0, 0, 0, PatternKind::And, 0)]];
        let ri = [matrix_info(2)];
        assert!(PatternIndex::new(entries).build_value_map(&ri)[0] & DIRECT_RULE_BIT == 0);
    }

    #[test]
    fn test_dispatch_multi_entry() {
        let entries = vec![vec![
            entry(0, 0, 0, PatternKind::And, 0),
            entry(1, 0, 0, PatternKind::And, 0),
        ]];
        let ri = [immediate_info(), immediate_info()];
        let index = PatternIndex::new(entries);
        let value_map = index.build_value_map(&ri);

        assert!(value_map[0] & DIRECT_RULE_BIT == 0);
        match index.dispatch_indirect(value_map[0]) {
            PatternDispatch::Entries(slice) => assert_eq!(slice.len(), 2),
            _ => panic!("expected Entries dispatch"),
        }
    }
}
