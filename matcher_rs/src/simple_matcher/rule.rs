//! Rule metadata, pattern dispatch, and rule-state transition logic.
//!
//! This module contains the types that bind deduplicated scan-engine patterns back to the
//! logical rules they came from. During construction ([`super::build`]), each user-supplied
//! rule string is split on `&`/`~` operators into sub-patterns. Those sub-patterns are
//! deduplicated across all rules, and each unique string receives a single automaton entry.
//! The [`PatternEntry`] records how every automaton hit maps back to a specific rule and
//! segment offset.
//!
//! At scan time the hot path reads raw match values from the automaton, dispatches them
//! through [`PatternIndex::dispatch`] into [`PatternDispatch`] variants, and feeds each
//! hit into [`RuleSet::process_entry`] — the core state machine that tracks bitmasks,
//! matrix counters, and generation stamps in the thread-local [`super::state::SimpleMatchState`].

use std::borrow::Cow;
use std::collections::HashMap;

use crate::process::ProcessType;

use super::SimpleResult;
use super::state::{ScanContext, SimpleMatchState};

/// Raw table format accepted by [`SimpleMatcher::new`](super::SimpleMatcher::new).
///
/// The outer key is the [`ProcessType`] that governs which text-transformation
/// pipeline to apply before matching. The inner key is a caller-chosen rule id
/// (`word_id`) that will be returned in [`SimpleResult::word_id`](super::SimpleResult::word_id)
/// on a match. The inner value is the pattern string, which may contain `&`
/// (AND) and `~` (NOT) operators to combine sub-patterns.
///
/// This is the borrowed-string variant — all pattern strings must outlive the
/// table reference passed to [`SimpleMatcher::new`](super::SimpleMatcher::new).
/// For owned or deserialized strings, use [`SimpleTableSerde`] instead.
///
/// # Examples
///
/// ```rust
/// use matcher_rs::{SimpleMatcher, SimpleTable, ProcessType};
/// use std::collections::HashMap;
///
/// let mut table: SimpleTable = HashMap::new();
/// table.entry(ProcessType::None).or_default().insert(1, "hello");
/// table.entry(ProcessType::None).or_default().insert(2, "apple&pie");
/// table.entry(ProcessType::Fanjian).or_default().insert(3, "你好");
///
/// let matcher = SimpleMatcher::new(&table).unwrap();
/// assert!(matcher.is_match("hello world"));
/// assert!(matcher.is_match("apple and pie"));
/// ```
pub type SimpleTable<'a> = HashMap<ProcessType, HashMap<u32, &'a str>>;

/// Serde-friendly table format that stores rule strings as [`Cow<str>`].
///
/// Identical semantics to [`SimpleTable`] but allows both owned and borrowed
/// pattern strings. This is useful when deserializing rule tables from JSON,
/// YAML, or other external sources where the strings are owned by the
/// deserializer.
///
/// # Examples
///
/// ```rust
/// use matcher_rs::{SimpleMatcher, SimpleTableSerde, ProcessType};
/// use std::borrow::Cow;
/// use std::collections::HashMap;
///
/// // Build programmatically with owned strings.
/// let mut table: SimpleTableSerde = HashMap::new();
/// table
///     .entry(ProcessType::None)
///     .or_default()
///     .insert(1, Cow::Owned("hello".to_string()));
///
/// let matcher = SimpleMatcher::new(&table).unwrap();
/// assert!(matcher.is_match("hello world"));
///
/// // Or deserialize from JSON (ProcessType serializes as its raw u8 bits).
/// let json = r#"{"1":{"1":"hello","2":"world"}}"#;
/// let deserialized: SimpleTableSerde = serde_json::from_str(json).unwrap();
/// let matcher = SimpleMatcher::new(&deserialized).unwrap();
/// assert!(matcher.is_match("hello world"));
/// ```
pub type SimpleTableSerde<'a> = HashMap<ProcessType, HashMap<u32, Cow<'a, str>>>;

/// High bit used to encode the direct-rule fast path in raw scan values.
///
/// When a deduplicated pattern is attached to exactly one [`PatternKind::Simple`] rule,
/// the automaton stores an encoded value with this bit set so that
/// [`PatternIndex::dispatch`] can skip the indirection through the entry table entirely.
///
/// The encoding packs both `rule_idx` and `pt_index` into 31 bits:
///
/// ```text
/// Bit 31:     DIRECT_RULE_BIT flag
/// Bits 26-30: pt_index (5 bits, max 31)
/// Bits 0-25:  rule_idx (26 bits, max ~67M rules)
/// ```
pub(super) const DIRECT_RULE_BIT: u32 = 1 << 31;

/// Bit shift for the process-type index inside a direct-rule encoded value.
const DIRECT_PT_SHIFT: u32 = 26;

/// Mask for extracting the process-type index from a direct-rule encoded value.
const DIRECT_PT_MASK: u32 = 0x1F << DIRECT_PT_SHIFT;

/// Mask for extracting the rule index from a direct-rule encoded value.
const DIRECT_RULE_MASK: u32 = (1 << DIRECT_PT_SHIFT) - 1;

/// Maximum number of segments handled by the bitmask fast path.
///
/// Rules with up to 64 AND/NOT segments track per-segment satisfaction in a single `u64`
/// bitmask (`WordState::satisfied_mask`). Rules exceeding this threshold fall back to
/// the per-variant counter matrix (`SimpleMatchState::matrix`).
pub(super) const BITMASK_CAPACITY: usize = 64;

/// Size of the compact process-type lookup table indexed by raw [`ProcessType`] bits.
///
/// [`ProcessType`] is a 6-bit bitflag, so `2^6 = 64` covers every possible combination.
/// The table maps each bitflag value to a dense sequential index used in the scan masks.
pub(super) const PROCESS_TYPE_TABLE_SIZE: usize = 64;

/// Pre-resolved rule shape encoding the combination of `use_matrix`, `and_count == 1`,
/// and `has_not` for one [`PatternEntry`].
///
/// Stored in [`PatternEntry::shape`] so the hot path in [`RuleSet::process_entry`] can
/// branch on rule properties without loading [`RuleHot`].
///
/// `repr(u8)` values are chosen so that:
/// - `has_not` = `self as u8 & 1 != 0` (odd values)
/// - `use_matrix` = `self as u8 >= 4`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub(super) enum RuleShape {
    /// Multi-segment bitmask path, no NOT segments.
    Bitmask = 0,
    /// Multi-segment bitmask path, with NOT segments.
    BitmaskNot = 1,
    /// Single AND segment, no NOT segments.
    SingleAnd = 2,
    /// Single AND segment, with NOT segments.
    SingleAndNot = 3,
    /// Per-variant counter matrix, no NOT segments.
    Matrix = 4,
    /// Per-variant counter matrix, with NOT segments.
    MatrixNot = 5,
}

impl RuleShape {
    /// Whether the owning rule contains at least one NOT (`~`) segment.
    #[inline(always)]
    pub(super) fn has_not(self) -> bool {
        self as u8 & 1 != 0
    }

    /// Whether the owning rule requires the per-variant counter matrix.
    #[inline(always)]
    pub(super) fn use_matrix(self) -> bool {
        matches!(self, Self::Matrix | Self::MatrixNot)
    }
}

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

/// Hot-path per-rule metadata used during scanning.
///
/// Stored in a contiguous `Vec` inside [`RuleSet`] and accessed by rule index on every
/// pattern hit. Fields are ordered to keep the most frequently read data together.
///
/// The `segment_counts` layout is:
/// ```text
/// [ and_0, and_1, …, and_{and_count-1}, not_0, not_1, … ]
///   ╰──── positive (AND) segments ────╯  ╰── negative (NOT) ──╯
/// ```
/// Positive counts start at the required number of hits (usually 1); negative counts
/// start at 0 and veto the rule when they go above 0 (for matrix mode) or on first hit
/// (for bitmask mode).
#[derive(Debug, Clone)]
pub(super) struct RuleHot {
    /// Required counts for every positive and negative segment in rule order.
    ///
    /// AND entries hold the required hit count (decremented toward zero);
    /// NOT entries hold a starting value of 0 (incremented on hit; any positive value
    /// means the veto segment was observed). Only read when `use_matrix` is true.
    pub(super) segment_counts: Vec<i32>,
    /// Number of positive (AND) segments at the front of `segment_counts`.
    pub(super) and_count: usize,
    /// Whether the rule needs the per-variant counter matrix instead of the `u64` bitmask.
    ///
    /// True when any segment requires a count other than 1, or when the total number
    /// of segments exceeds [`BITMASK_CAPACITY`].
    pub(super) use_matrix: bool,
}

/// Cold rule metadata only needed when returning results.
///
/// Separated from [`RuleHot`] so the scan hot path never touches this data. Only
/// accessed when a rule is confirmed satisfied and a [`SimpleResult`]
/// must be produced.
#[derive(Debug, Clone)]
pub(super) struct RuleCold {
    /// Caller-supplied rule identifier returned in match results.
    pub(super) word_id: u32,
    /// Original rule string stored for borrowed result output.
    ///
    /// Owned here so that [`SimpleResult::word`](super::SimpleResult::word) can borrow
    /// it as `Cow::Borrowed`.
    pub(super) word: String,
}

/// One deduplicated pattern's attachment to a concrete rule segment.
///
/// Multiple rules may share the same deduplicated pattern string (e.g., two rules both
/// contain the sub-pattern `"hello"`). Each such binding is stored as a separate
/// `PatternEntry` in the same bucket of the [`PatternIndex`].
///
/// Size: 8 bytes (u32 + u8 + u8 + u8 + u8).
#[derive(Debug, Clone)]
pub(super) struct PatternEntry {
    /// Rule index inside [`RuleSet`] (indexes into the hot/cold `Vec`s).
    pub(super) rule_idx: u32,
    /// Segment offset within the rule's [`RuleHot::segment_counts`] array.
    ///
    /// For AND segments this is `0..and_count`; for NOT segments it is `and_count..`.
    /// Maximum 255 segments per rule (far exceeds [`BITMASK_CAPACITY`] of 64).
    pub(super) offset: u8,
    /// Compact process-type index assigned by `SimpleMatcher::build_pt_index_table`.
    ///
    /// Used to filter pattern hits by comparing against the current variant's
    /// [`ScanContext::process_type_mask`].
    pub(super) pt_index: u8,
    /// Logical role of this segment hit.
    pub(super) kind: PatternKind,
    /// Pre-resolved rule shape encoding `use_matrix`, `and_count == 1`, and `has_not`.
    ///
    /// Lets [`RuleSet::process_entry`] branch on rule properties without touching the
    /// `hot` array (only needed on first-touch in [`SimpleMatchState::init_rule`]).
    pub(super) shape: RuleShape,
}

/// All hot and cold metadata for the compiled rule set.
///
/// `hot` and `cold` are parallel `Vec`s indexed by rule index. [`RuleHot`] is read on
/// every pattern hit (scan hot path); [`RuleCold`] is read only when producing output
/// results, keeping the scan loop's working set small.
#[derive(Clone)]
pub(super) struct RuleSet {
    hot: Vec<RuleHot>,
    cold: Vec<RuleCold>,
    /// `true` when at least one rule contains a NOT (`~`) segment.
    ///
    /// When false, `positive_generation == generation` permanently resolves a rule,
    /// enabling variant-level early termination in `walk_and_scan`.
    has_not_rules: bool,
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
}

/// Dispatch result for one raw scan value.
///
/// Returned by [`PatternIndex::dispatch`] to tell the caller how to interpret an
/// automaton hit. The three variants are ordered by decreasing fast-path likelihood:
///
/// 1. [`DirectRule`](Self::DirectRule) — the automaton value already encodes the rule index
///    (only possible for single-entry [`PatternKind::Simple`] patterns in
///    [`AllSimple`](super::SearchMode::AllSimple) mode).
/// 2. [`SingleEntry`](Self::SingleEntry) — one entry to process.
/// 3. [`Entries`](Self::Entries) — multiple rules share this pattern string.
pub(super) enum PatternDispatch<'a> {
    /// Direct simple-rule fast path — carries rule index and process-type index.
    DirectRule { rule_idx: usize, pt_index: u8 },
    /// Exactly one attached pattern entry.
    SingleEntry(&'a PatternEntry),
    /// Multiple attached entries sharing the same deduplicated pattern string.
    Entries(&'a [PatternEntry]),
}

/// Rule-evaluation helpers used by the scan hot path.
impl RuleSet {
    /// Creates the compiled rule set from parallel hot and cold metadata vectors.
    ///
    /// # Panics
    ///
    /// Debug-asserts that `hot.len() == cold.len()`.
    pub(super) fn new(hot: Vec<RuleHot>, cold: Vec<RuleCold>, has_not_rules: bool) -> Self {
        Self {
            hot,
            cold,
            has_not_rules,
        }
    }

    /// Returns `true` when at least one rule contains a NOT segment.
    #[inline(always)]
    pub(super) fn has_not_rules(&self) -> bool {
        self.has_not_rules
    }

    /// Returns the estimated heap memory in bytes owned by this rule set.
    pub(super) fn heap_bytes(&self) -> usize {
        let hot_inner: usize = self
            .hot
            .iter()
            .map(|r| r.segment_counts.capacity() * size_of::<i32>())
            .sum();
        let cold_inner: usize = self.cold.iter().map(|r| r.word.capacity()).sum();
        self.hot.capacity() * size_of::<RuleHot>()
            + hot_inner
            + self.cold.capacity() * size_of::<RuleCold>()
            + cold_inner
    }

    /// Returns the number of compiled rules.
    #[inline(always)]
    pub(super) fn len(&self) -> usize {
        self.hot.len()
    }

    /// Returns whether any touched rule is satisfied in the current generation.
    ///
    /// Iterates only over rule indices that were touched (had at least one pattern hit),
    /// not over the full rule set.
    #[inline(always)]
    pub(super) fn has_match(&self, state: &SimpleMatchState) -> bool {
        state
            .touched_indices()
            .iter()
            .any(|&rule_idx| state.rule_is_satisfied(rule_idx))
    }

    /// Pushes one result when `rule_idx` becomes positive for the first time in this generation.
    ///
    /// Used by the all-simple fast path where every hit is immediately a completed rule.
    /// Deduplication is handled by [`SimpleMatchState::mark_positive`]: if the rule was
    /// already marked positive in this generation, no result is emitted.
    #[inline(always)]
    pub(super) fn push_result_if_new<'a>(
        &'a self,
        rule_idx: usize,
        state: &mut SimpleMatchState,
        results: &mut Vec<SimpleResult<'a>>,
    ) {
        if state.mark_positive(rule_idx) {
            self.push_result(rule_idx, results);
        }
    }

    /// Appends every satisfied touched rule to `results`.
    ///
    /// Called after all variants have been scanned (Pass 2). Only rules whose AND
    /// segments are all satisfied and whose NOT segments were never triggered are emitted.
    pub(super) fn collect_matches<'a>(
        &'a self,
        state: &SimpleMatchState,
        results: &mut Vec<SimpleResult<'a>>,
    ) {
        for &rule_idx in state.touched_indices() {
            if state.rule_is_satisfied(rule_idx) {
                self.push_result(rule_idx, results);
            }
        }
    }

    /// Applies one pattern hit to the rule state machine.
    ///
    /// This is the core state-transition function for the two-pass matcher. Given a
    /// [`PatternEntry`] produced by automaton dispatch, it updates the corresponding
    /// rule's [`WordState`](super::state::WordState) and returns `true` only when the
    /// caller may stop early because a non-vetoed rule is already satisfied.
    ///
    /// # State transitions by [`PatternKind`]
    ///
    /// - **Simple**: Marks the rule satisfied on first touch. Idempotent on repeat hits.
    /// - **And**: Decrements the remaining-AND counter (bitmask path) or the matrix
    ///   counter (matrix path). When the counter reaches zero the rule becomes satisfied.
    /// - **Not**: Sets `not_generation` to veto the rule. With the matrix path, the
    ///   per-segment counter is incremented and the veto fires only when the count goes
    ///   positive.
    ///
    /// # Safety
    ///
    /// Uses `get_unchecked` / `get_unchecked_mut` on `state.word_states`, `state.matrix`,
    /// and `state.matrix_status`. `self.hot` is only accessed on the cold init path
    /// (first touch per rule per generation). All accesses are guarded by preceding
    /// `debug_assert!` bounds checks.
    #[inline(always)]
    pub(super) fn process_entry(
        &self,
        entry: &PatternEntry,
        ctx: ScanContext,
        state: &mut SimpleMatchState,
    ) -> bool {
        let generation = state.generation();
        let &PatternEntry {
            rule_idx,
            offset,
            pt_index,
            kind,
            shape,
        } = entry;

        let rule_idx = rule_idx as usize;

        if ctx.process_type_mask & (1u64 << pt_index) == 0 {
            return false;
        }

        debug_assert!(rule_idx < state.word_states.len());
        debug_assert!(rule_idx < self.hot.len());

        match kind {
            PatternKind::Simple => {
                // SAFETY: `rule_idx` is in bounds — guaranteed by debug_assert above.
                let word_state = unsafe { state.word_states.get_unchecked_mut(rule_idx) };
                if word_state.positive_generation == generation {
                    return ctx.exit_early;
                }
                if word_state.matrix_generation != generation {
                    word_state.matrix_generation = generation;
                    word_state.positive_generation = generation;
                    state.touched_indices.push(rule_idx);
                    state.resolved_count += 1;
                    return ctx.exit_early;
                }
            }
            PatternKind::And => {
                let offset = offset as usize;
                // SAFETY: `rule_idx` is in bounds — guaranteed by debug_asserts above.
                let word_state = unsafe { state.word_states.get_unchecked_mut(rule_idx) };

                if word_state.not_generation == generation {
                    return false;
                }
                if word_state.positive_generation == generation {
                    if !shape.has_not() && ctx.exit_early {
                        return true;
                    }
                    return false;
                }

                if word_state.matrix_generation != generation {
                    // SAFETY: `rule_idx` is in bounds — guaranteed by debug_asserts above.
                    let rule = unsafe { self.hot.get_unchecked(rule_idx) };
                    state.init_rule(rule, rule_idx, ctx);
                }

                // SAFETY: `rule_idx` is in bounds — guaranteed by debug_asserts above.
                let word_state = unsafe { state.word_states.get_unchecked_mut(rule_idx) };
                let is_satisfied = if shape.use_matrix() {
                    // SAFETY: `rule_idx` is in bounds — matrix/status vecs are sized to match rules.
                    let flat_matrix = unsafe { state.matrix.get_unchecked_mut(rule_idx) };
                    // SAFETY: `rule_idx` is in bounds — matrix/status vecs are sized to match rules.
                    let flat_status = unsafe { state.matrix_status.get_unchecked_mut(rule_idx) };
                    let counter = &mut flat_matrix[offset * ctx.num_variants + ctx.text_index];
                    *counter -= 1;
                    if flat_status[offset] == 0 && *counter <= 0 {
                        flat_status[offset] = 1;
                        word_state.remaining_and -= 1;
                        if word_state.remaining_and == 0 {
                            word_state.positive_generation = generation;
                            state.resolved_count += 1;
                        }
                    }
                    word_state.positive_generation == generation
                } else if matches!(shape, RuleShape::SingleAnd | RuleShape::SingleAndNot) {
                    word_state.positive_generation = generation;
                    state.resolved_count += 1;
                    true
                } else {
                    let bit = 1u64 << offset;
                    if word_state.satisfied_mask & bit == 0 {
                        word_state.satisfied_mask |= bit;
                        word_state.remaining_and -= 1;
                        if word_state.remaining_and == 0 {
                            word_state.positive_generation = generation;
                            state.resolved_count += 1;
                        }
                    }
                    word_state.positive_generation == generation
                };

                if ctx.exit_early
                    && is_satisfied
                    && !shape.has_not()
                    && word_state.not_generation != generation
                {
                    return true;
                }
            }
            PatternKind::Not => {
                let offset = offset as usize;
                // SAFETY: `rule_idx` is in bounds — guaranteed by debug_asserts above.
                let word_state = unsafe { state.word_states.get_unchecked_mut(rule_idx) };

                if word_state.not_generation == generation {
                    return false;
                }

                if word_state.matrix_generation != generation {
                    // SAFETY: `rule_idx` is in bounds — guaranteed by debug_asserts above.
                    let rule = unsafe { self.hot.get_unchecked(rule_idx) };
                    state.init_rule(rule, rule_idx, ctx);
                }

                // SAFETY: `rule_idx` is in bounds — guaranteed by debug_asserts above.
                let word_state = unsafe { state.word_states.get_unchecked_mut(rule_idx) };
                if shape.use_matrix() {
                    // SAFETY: `rule_idx` is in bounds — matrix/status vecs are sized to match rules.
                    let flat_matrix = unsafe { state.matrix.get_unchecked_mut(rule_idx) };
                    // SAFETY: `rule_idx` is in bounds — matrix/status vecs are sized to match rules.
                    let flat_status = unsafe { state.matrix_status.get_unchecked_mut(rule_idx) };
                    let counter = &mut flat_matrix[offset * ctx.num_variants + ctx.text_index];
                    *counter += 1;
                    if flat_status[offset] == 0 && *counter > 0 {
                        flat_status[offset] = 1;
                        word_state.not_generation = generation;
                    }
                } else {
                    word_state.not_generation = generation;
                }
            }
        }

        false
    }

    /// Pushes the borrowed public result for `rule_idx`.
    ///
    /// # Safety
    ///
    /// Uses `get_unchecked` on `self.cold`. Guarded by a preceding `debug_assert!`
    /// bounds check.
    ///
    /// # Panics
    ///
    /// Debug-asserts that `rule_idx < self.cold.len()`.
    #[inline(always)]
    fn push_result<'a>(&'a self, rule_idx: usize, results: &mut Vec<SimpleResult<'a>>) {
        debug_assert!(rule_idx < self.cold.len());
        // SAFETY: `rule_idx` is in bounds — guaranteed by debug_assert above.
        let cold = unsafe { self.cold.get_unchecked(rule_idx) };
        results.push(SimpleResult {
            word_id: cold.word_id,
            word: Cow::Borrowed(&cold.word),
        });
    }
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

        Self { entries, ranges }
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

    /// Returns whether every entry across all patterns is a [`PatternKind::Simple`] segment.
    ///
    /// When true, the matcher can use [`AllSimple`](super::SearchMode::AllSimple)
    /// which skips the full state machine and processes every hit as a completed rule.
    #[inline(always)]
    pub(super) fn all_simple(&self) -> bool {
        self.entries
            .iter()
            .all(|entry| entry.kind == PatternKind::Simple)
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
                    && (entry.pt_index as u32) < 32
                    && entry.rule_idx < (1 << DIRECT_PT_SHIFT)
                {
                    let encoded = DIRECT_RULE_BIT
                        | ((entry.pt_index as u32) << DIRECT_PT_SHIFT)
                        | entry.rule_idx;
                    value_map.push(encoded);
                    continue;
                }
            }
            value_map.push(dedup_idx as u32);
        }

        value_map
    }

    /// Resolves one raw scan value back into rule-attachment metadata.
    ///
    /// When the high bit ([`DIRECT_RULE_BIT`]) is set, the value encodes both `rule_idx`
    /// and `pt_index` directly — no indirection through the entry table. Otherwise, the
    /// value is a deduplicated pattern index looked up in `ranges` and `entries`.
    ///
    /// # Safety
    ///
    /// Uses `get_unchecked` on `self.ranges` and `self.entries`. All accesses are guarded
    /// by preceding `debug_assert!` bounds checks.
    ///
    /// # Panics
    ///
    /// Debug-asserts that the pattern index is within `self.ranges` and that the resulting
    /// entry slice is within `self.entries`.
    #[inline(always)]
    pub(super) fn dispatch(&self, raw_value: u32) -> PatternDispatch<'_> {
        if raw_value & DIRECT_RULE_BIT != 0 {
            let pt_index = ((raw_value & DIRECT_PT_MASK) >> DIRECT_PT_SHIFT) as u8;
            let rule_idx = (raw_value & DIRECT_RULE_MASK) as usize;
            return PatternDispatch::DirectRule { rule_idx, pt_index };
        }

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
    use super::*;

    fn make_ctx(exit_early: bool) -> ScanContext {
        ScanContext {
            text_index: 0,
            process_type_mask: u64::MAX,
            num_variants: 1,
            exit_early,
            is_ascii: true,
        }
    }

    fn make_simple_ruleset(word_id: u32, word: &str) -> RuleSet {
        RuleSet::new(
            vec![RuleHot {
                segment_counts: vec![1],
                and_count: 1,
                use_matrix: false,
            }],
            vec![RuleCold {
                word_id,
                word: word.to_owned(),
            }],
            false,
        )
    }

    // --- PatternIndex dispatch tests ---

    #[test]
    fn test_pattern_index_dispatch_direct_rule() {
        let entries = vec![vec![PatternEntry {
            rule_idx: 5,
            offset: 0,
            pt_index: 2,
            kind: PatternKind::Simple,
            shape: RuleShape::SingleAnd,
        }]];
        let index = PatternIndex::new(entries);
        let value_map = index.build_value_map();

        assert_eq!(value_map.len(), 1);
        assert!(
            value_map[0] & DIRECT_RULE_BIT != 0,
            "should set DIRECT_RULE_BIT"
        );

        match index.dispatch(value_map[0]) {
            PatternDispatch::DirectRule { rule_idx, pt_index } => {
                assert_eq!(rule_idx, 5);
                assert_eq!(pt_index, 2);
            }
            _ => panic!("expected DirectRule dispatch"),
        }
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
        }]];
        let index = PatternIndex::new(entries);
        let value_map = index.build_value_map();

        assert!(
            value_map[0] & DIRECT_RULE_BIT == 0,
            "And kind should not get DIRECT_RULE_BIT"
        );

        match index.dispatch(value_map[0]) {
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
            },
            PatternEntry {
                rule_idx: 1,
                offset: 0,
                pt_index: 0,
                kind: PatternKind::Simple,
                shape: RuleShape::SingleAnd,
            },
        ]];
        let index = PatternIndex::new(entries);
        let value_map = index.build_value_map();

        // Multi-entry patterns never get DIRECT_RULE_BIT
        assert!(value_map[0] & DIRECT_RULE_BIT == 0);

        match index.dispatch(value_map[0]) {
            PatternDispatch::Entries(slice) => assert_eq!(slice.len(), 2),
            _ => panic!("expected Entries dispatch"),
        }
    }

    // --- RuleShape predicate tests ---

    #[test]
    fn test_rule_shape_predicates() {
        assert!(!RuleShape::Bitmask.has_not());
        assert!(RuleShape::BitmaskNot.has_not());
        assert!(!RuleShape::SingleAnd.has_not());
        assert!(RuleShape::SingleAndNot.has_not());
        assert!(!RuleShape::Matrix.has_not());
        assert!(RuleShape::MatrixNot.has_not());

        assert!(!RuleShape::Bitmask.use_matrix());
        assert!(!RuleShape::BitmaskNot.use_matrix());
        assert!(!RuleShape::SingleAnd.use_matrix());
        assert!(!RuleShape::SingleAndNot.use_matrix());
        assert!(RuleShape::Matrix.use_matrix());
        assert!(RuleShape::MatrixNot.use_matrix());
    }

    // --- process_entry tests ---

    #[test]
    fn test_process_entry_simple_kind() {
        let rules = make_simple_ruleset(1, "hello");
        let mut state = SimpleMatchState::new();
        state.prepare(1);

        let entry = PatternEntry {
            rule_idx: 0,
            offset: 0,
            pt_index: 0,
            kind: PatternKind::Simple,
            shape: RuleShape::SingleAnd,
        };

        let result = rules.process_entry(&entry, make_ctx(true), &mut state);
        assert!(result, "Simple entry with exit_early should return true");
        assert!(state.rule_is_satisfied(0));

        // Idempotent on repeat
        let result2 = rules.process_entry(&entry, make_ctx(true), &mut state);
        assert!(
            result2,
            "already-satisfied Simple should still return exit_early"
        );
    }

    #[test]
    fn test_process_entry_and_bitmask() {
        // 3-segment AND rule: a&b&c
        let rules = RuleSet::new(
            vec![RuleHot {
                segment_counts: vec![1, 1, 1],
                and_count: 3,
                use_matrix: false,
            }],
            vec![RuleCold {
                word_id: 1,
                word: "a&b&c".to_owned(),
            }],
            false,
        );
        let mut state = SimpleMatchState::new();
        state.prepare(1);
        let ctx = make_ctx(true);

        // Process segment 0
        let e0 = PatternEntry {
            rule_idx: 0,
            offset: 0,
            pt_index: 0,
            kind: PatternKind::And,
            shape: RuleShape::Bitmask,
        };
        assert!(!rules.process_entry(&e0, ctx, &mut state));
        assert!(!state.rule_is_satisfied(0));

        // Process segment 1
        let e1 = PatternEntry { offset: 1, ..e0 };
        assert!(!rules.process_entry(&e1, ctx, &mut state));
        assert!(!state.rule_is_satisfied(0));

        // Process segment 2 — now satisfied
        let e2 = PatternEntry { offset: 2, ..e0 };
        assert!(rules.process_entry(&e2, ctx, &mut state));
        assert!(state.rule_is_satisfied(0));
    }

    #[test]
    fn test_process_entry_not_veto() {
        // Rule: a~b (1 AND, 1 NOT)
        let rules = RuleSet::new(
            vec![RuleHot {
                segment_counts: vec![1, 0],
                and_count: 1,
                use_matrix: false,
            }],
            vec![RuleCold {
                word_id: 1,
                word: "a~b".to_owned(),
            }],
            true,
        );
        let mut state = SimpleMatchState::new();
        state.prepare(1);
        let ctx = make_ctx(false);

        // Satisfy the AND segment
        let and_entry = PatternEntry {
            rule_idx: 0,
            offset: 0,
            pt_index: 0,
            kind: PatternKind::And,
            shape: RuleShape::SingleAndNot,
        };
        rules.process_entry(&and_entry, ctx, &mut state);
        assert!(state.rule_is_satisfied(0));

        // NOT segment vetoes
        let not_entry = PatternEntry {
            rule_idx: 0,
            offset: 1,
            pt_index: 0,
            kind: PatternKind::Not,
            shape: RuleShape::SingleAndNot,
        };
        rules.process_entry(&not_entry, ctx, &mut state);
        assert!(!state.rule_is_satisfied(0), "NOT should veto the rule");
    }

    #[test]
    fn test_process_entry_matrix_counters() {
        // Matrix rule: repeated AND segment that needs 2 hits
        let rules = RuleSet::new(
            vec![RuleHot {
                segment_counts: vec![2, 1],
                and_count: 2,
                use_matrix: true,
            }],
            vec![RuleCold {
                word_id: 1,
                word: "a&a&b".to_owned(),
            }],
            false,
        );
        let mut state = SimpleMatchState::new();
        state.prepare(1);
        let ctx = make_ctx(true);

        let seg0 = PatternEntry {
            rule_idx: 0,
            offset: 0,
            pt_index: 0,
            kind: PatternKind::And,
            shape: RuleShape::Matrix,
        };
        let seg1 = PatternEntry {
            rule_idx: 0,
            offset: 1,
            pt_index: 0,
            kind: PatternKind::And,
            shape: RuleShape::Matrix,
        };

        // First hit on seg0 (needs 2): counter goes 2→1, not satisfied
        assert!(!rules.process_entry(&seg0, ctx, &mut state));
        assert!(!state.rule_is_satisfied(0));

        // Hit seg1 (needs 1): counter goes 1→0, seg1 done but seg0 still pending
        assert!(!rules.process_entry(&seg1, ctx, &mut state));
        assert!(!state.rule_is_satisfied(0));

        // Second hit on seg0: counter goes 1→0, all segments done
        assert!(rules.process_entry(&seg0, ctx, &mut state));
        assert!(state.rule_is_satisfied(0));
    }

    #[test]
    fn test_process_entry_pt_mask_filters() {
        let rules = make_simple_ruleset(1, "hello");
        let mut state = SimpleMatchState::new();
        state.prepare(1);

        let entry = PatternEntry {
            rule_idx: 0,
            offset: 0,
            pt_index: 3, // bit 3
            kind: PatternKind::Simple,
            shape: RuleShape::SingleAnd,
        };

        // Mask does NOT include bit 3
        let ctx = ScanContext {
            text_index: 0,
            process_type_mask: 0b0101, // bits 0 and 2 only
            num_variants: 1,
            exit_early: true,
            is_ascii: true,
        };
        assert!(!rules.process_entry(&entry, ctx, &mut state));
        assert!(
            !state.rule_is_satisfied(0),
            "entry should be filtered by mask"
        );
    }
}
