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
    encoding::{DIRECT_RULE_BIT, DirectValue},
    rule::RuleShape,
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
/// Size: 12 bytes (u32 + 5×u8 + padding).
#[derive(Debug, Clone)]
pub(super) struct PatternEntry {
    /// Rule index inside [`RuleSet`](super::rule::RuleSet).
    pub(super) rule_idx: u32,
    /// Segment offset within the rule's
    /// [`Rule::segment_counts`](super::rule::Rule::segment_counts) array.
    ///
    /// For AND segments this is `0..and_count`; for NOT segments it is
    /// `and_count..`. Maximum 255 segments per rule (far exceeds
    /// [`BITMASK_CAPACITY`](super::encoding::BITMASK_CAPACITY) of 64).
    pub(super) offset: u8,
    /// Compact process-type index assigned by
    /// `SimpleMatcher::build_pt_index_table`.
    ///
    /// Used to filter pattern hits by comparing against the current variant's
    /// [`ScanContext::process_type_mask`](super::state::ScanContext::process_type_mask).
    pub(super) pt_index: u8,
    /// Logical role of this segment hit.
    pub(super) kind: PatternKind,
    /// Pre-resolved rule shape encoding `use_matrix`, `and_count == 1`, and
    /// `has_not`.
    ///
    /// Lets [`RuleSet::process_entry`](super::rule::RuleSet::process_entry)
    /// branch on rule properties without loading the `Rule` struct (only
    /// needed on first-touch in `ScanState::init_rule`).
    pub(super) shape: RuleShape,
    /// Word boundary flags (bit 0 = left `\b`, bit 1 = right `\b`).
    ///
    /// When non-zero, the scan dispatch checks `is_word_byte` at match
    /// start/end before forwarding the hit to
    /// [`RuleSet::process_entry`](super::rule::RuleSet::process_entry).
    pub(super) boundary: u8,
    /// Number of positive (AND) segments in the owning rule.
    ///
    /// Duplicated from the rule's AND-segment count so that
    /// [`RuleSet::process_entry`](super::rule::RuleSet::process_entry) can
    /// initialize per-rule state without loading the `Rule` struct
    /// (avoiding a cache miss on the rules array). Fits in the
    /// existing struct padding (9→10 bytes, still padded to 12).
    pub(super) and_count: u8,
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

    /// Returns whether every pattern maps to a single-entry SingleAnd rule.
    ///
    /// When true and the transform tree has no children, `is_match` can
    /// delegate directly to the AC automaton — each hit is a completed rule.
    #[inline(always)]
    pub(super) fn all_single_and(&self) -> bool {
        self.ranges.iter().all(|&(_, len)| len == 1)
            && self
                .entries
                .iter()
                .all(|e| matches!(e.shape, RuleShape::SingleAnd))
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
    pub(super) fn build_value_map(&self) -> Vec<u32> {
        let mut value_map = Vec::with_capacity(self.ranges.len());

        for (dedup_idx, &(start, len)) in self.ranges.iter().enumerate() {
            if len == 1 {
                // SAFETY: `start` is in bounds — sourced from `self.ranges`, built by
                // `Self::new`.
                let entry = unsafe { self.entries.get_unchecked(start) };
                if let Some(encoded) = Self::try_encode_direct(entry) {
                    value_map.push(encoded);
                    continue;
                }
            }
            value_map.push(dedup_idx as u32);
        }

        value_map
    }

    /// Attempts to encode a single-entry pattern into a direct value.
    /// Returns `None` if the entry doesn't fit any direct encoding.
    fn try_encode_direct(entry: &PatternEntry) -> Option<u32> {
        if entry.shape.use_matrix() {
            return None;
        }
        let rule_idx = entry.rule_idx as usize;
        let dv = match entry.kind {
            PatternKind::And => match entry.shape {
                RuleShape::SingleAnd => DirectValue::SingleAnd { rule_idx },
                RuleShape::SingleAndNot => DirectValue::SingleAndNot { rule_idx },
                RuleShape::Bitmask | RuleShape::BitmaskNot => DirectValue::BitmaskAnd {
                    rule_idx,
                    offset: entry.offset as usize,
                    and_count: entry.and_count as u16,
                    has_not: entry.shape.has_not(),
                },
                _ => return None,
            },
            PatternKind::Not => DirectValue::Not {
                rule_idx,
                and_count: entry.and_count as u16,
            },
        };
        dv.encode(entry.pt_index, entry.boundary)
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

    fn make_entry(
        rule_idx: u32,
        offset: u8,
        pt_index: u8,
        kind: PatternKind,
        shape: RuleShape,
        boundary: u8,
        and_count: u8,
    ) -> PatternEntry {
        PatternEntry {
            rule_idx,
            offset,
            pt_index,
            kind,
            shape,
            boundary,
            and_count,
        }
    }

    #[test]
    fn test_direct_single_and() {
        let entries = vec![vec![make_entry(
            5,
            0,
            2,
            PatternKind::And,
            RuleShape::SingleAnd,
            1,
            1,
        )]];
        let raw = PatternIndex::new(entries).build_value_map()[0];
        assert!(raw & DIRECT_RULE_BIT != 0);
        let (pt, bd, dv) = DirectValue::decode(raw);
        assert_eq!(pt, 2);
        assert_eq!(bd, 1);
        assert_eq!(dv, DirectValue::SingleAnd { rule_idx: 5 });
    }

    #[test]
    fn test_direct_single_and_not() {
        let entries = vec![vec![make_entry(
            100,
            0,
            3,
            PatternKind::And,
            RuleShape::SingleAndNot,
            0,
            1,
        )]];
        let raw = PatternIndex::new(entries).build_value_map()[0];
        assert!(raw & DIRECT_RULE_BIT != 0);
        let (pt, _, dv) = DirectValue::decode(raw);
        assert_eq!(pt, 3);
        assert_eq!(dv, DirectValue::SingleAndNot { rule_idx: 100 });
    }

    #[test]
    fn test_direct_bitmask_and() {
        let entries = vec![vec![make_entry(
            42,
            1,
            2,
            PatternKind::And,
            RuleShape::Bitmask,
            0,
            3,
        )]];
        let raw = PatternIndex::new(entries).build_value_map()[0];
        assert!(raw & DIRECT_RULE_BIT != 0);
        let (pt, _, dv) = DirectValue::decode(raw);
        assert_eq!(pt, 2);
        assert_eq!(
            dv,
            DirectValue::BitmaskAnd {
                rule_idx: 42,
                offset: 1,
                and_count: 3,
                has_not: false
            }
        );
    }

    #[test]
    fn test_direct_bitmask_not_and_entry() {
        let entries = vec![vec![make_entry(
            7,
            0,
            0,
            PatternKind::And,
            RuleShape::BitmaskNot,
            0,
            2,
        )]];
        let raw = PatternIndex::new(entries).build_value_map()[0];
        assert!(raw & DIRECT_RULE_BIT != 0);
        let (_, _, dv) = DirectValue::decode(raw);
        assert!(matches!(dv, DirectValue::BitmaskAnd { has_not: true, .. }));
    }

    #[test]
    fn test_direct_not_entry() {
        let entries = vec![vec![make_entry(
            50,
            1,
            0,
            PatternKind::Not,
            RuleShape::SingleAndNot,
            0,
            1,
        )]];
        let raw = PatternIndex::new(entries).build_value_map()[0];
        assert!(raw & DIRECT_RULE_BIT != 0);
        let (_, _, dv) = DirectValue::decode(raw);
        assert_eq!(
            dv,
            DirectValue::Not {
                rule_idx: 50,
                and_count: 1
            }
        );
    }

    #[test]
    fn test_bitmask_large_rule_idx_falls_back() {
        let entries = vec![vec![make_entry(
            40000,
            0,
            0,
            PatternKind::And,
            RuleShape::Bitmask,
            0,
            2,
        )]];
        assert!(PatternIndex::new(entries).build_value_map()[0] & DIRECT_RULE_BIT == 0);
    }

    #[test]
    fn test_matrix_always_falls_back() {
        let entries = vec![vec![make_entry(
            0,
            0,
            0,
            PatternKind::And,
            RuleShape::Matrix,
            0,
            2,
        )]];
        assert!(PatternIndex::new(entries).build_value_map()[0] & DIRECT_RULE_BIT == 0);
    }

    #[test]
    fn test_dispatch_multi_entry() {
        let entries = vec![vec![
            PatternEntry {
                rule_idx: 0,
                offset: 0,
                pt_index: 0,
                kind: PatternKind::And,
                shape: RuleShape::SingleAnd,
                boundary: 0,
                and_count: 1,
            },
            PatternEntry {
                rule_idx: 1,
                offset: 0,
                pt_index: 0,
                kind: PatternKind::And,
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
