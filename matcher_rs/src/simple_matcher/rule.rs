//! Rule metadata and rule-state transition logic.
//!
//! This module contains the types that bind rule metadata to the scan state machine.
//! [`RuleSet`] owns parallel hot/cold metadata vectors and exposes [`process_entry`](RuleSet::process_entry)
//! — the core state-transition function that tracks bitmasks, matrix counters, and
//! generation stamps in the thread-local [`super::state::SimpleMatchState`].
//!
//! Pattern types ([`super::pattern::PatternEntry`], [`super::pattern::PatternIndex`],
//! [`super::pattern::PatternDispatch`]) live in [`super::pattern`].
//! Bit-packing constants live in [`super::encoding`].

use std::borrow::Cow;
use std::collections::HashMap;

use crate::process::ProcessType;

use super::SimpleResult;
use super::pattern::{PatternEntry, PatternKind};
use super::state::{ScanContext, ScanState, init_matrix};

/// Raw table format accepted by [`SimpleMatcher::new`](super::SimpleMatcher::new).
///
/// The outer key is the [`ProcessType`] that governs which text-transformation
/// pipeline to apply before matching. The inner key is a caller-chosen rule id
/// (`word_id`) that will be returned in [`SimpleResult::word_id`](super::SimpleResult::word_id)
/// on a match. The inner value is the pattern string, which may contain `&`
/// (AND), `~` (NOT), and `|` (OR) operators to combine sub-patterns.
/// `|` binds tighter than `&`/`~`: `"a|b&c"` means (a OR b) AND c.
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
/// table.entry(ProcessType::VariantNorm).or_default().insert(3, "你好");
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
    pub(super) fn has_not(self) -> bool {
        self as u8 & 1 != 0
    }

    /// Whether the owning rule requires the per-variant counter matrix.
    pub(super) fn use_matrix(self) -> bool {
        matches!(self, Self::Matrix | Self::MatrixNot)
    }
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
    /// means the veto segment was observed). Only read when `RuleShape::use_matrix()` is true.
    pub(super) segment_counts: Vec<i32>,
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
    #[inline(always)]
    pub(super) fn has_match(&self, ss: &ScanState<'_>) -> bool {
        ss.touched_indices()
            .iter()
            .any(|&rule_idx| ss.rule_is_satisfied(rule_idx))
    }

    /// Pushes one result when `rule_idx` becomes positive for the first time in this generation.
    ///
    /// Used by the all-simple fast path where every hit is immediately a completed rule.
    #[inline(always)]
    pub(super) fn push_result_if_new<'a>(
        &'a self,
        rule_idx: usize,
        ss: &mut ScanState<'_>,
        results: &mut Vec<SimpleResult<'a>>,
    ) {
        if ss.mark_positive_simple(rule_idx) {
            self.push_result(rule_idx, results);
        }
    }

    /// Appends every satisfied touched rule to `results`.
    pub(super) fn collect_matches<'a>(
        &'a self,
        ss: &ScanState<'_>,
        results: &mut Vec<SimpleResult<'a>>,
    ) {
        for &rule_idx in ss.touched_indices() {
            if ss.rule_is_satisfied(rule_idx) {
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
    /// Init logic is inlined rather than calling `ScanState::init_rule` so that the
    /// `&mut WordState` reference obtained at the start of each arm survives across the
    /// init — eliminating a second `word_states` lookup per call.
    ///
    /// # Panics
    ///
    /// In debug builds, panics if `entry.rule_idx` is out of bounds for the
    /// rule arrays. This invariant is guaranteed by construction in
    /// [`SimpleMatcher::new`](super::SimpleMatcher::new).
    #[inline(always)]
    pub(super) fn process_entry(
        &self,
        entry: &PatternEntry,
        ctx: ScanContext,
        ss: &mut ScanState<'_>,
    ) -> bool {
        let generation = ss.generation;
        let &PatternEntry {
            rule_idx,
            offset,
            pt_index,
            kind,
            shape,
            boundary: _,
            and_count: _,
        } = entry;

        let rule_idx = rule_idx as usize;

        if ctx.process_type_mask & (1u64 << pt_index) == 0 {
            return false;
        }

        debug_assert!(rule_idx < ss.word_states.len());
        debug_assert!(rule_idx < self.hot.len());

        match kind {
            PatternKind::Simple => {
                // SAFETY: `rule_idx` is in bounds — guaranteed by debug_assert above.
                let word_state = unsafe { ss.word_states.get_unchecked_mut(rule_idx) };
                if word_state.positive_generation == generation {
                    return ctx.exit_early;
                }
                if word_state.matrix_generation != generation {
                    word_state.matrix_generation = generation;
                    word_state.positive_generation = generation;
                    ss.touched_indices.push(rule_idx);
                    ss.resolved_count += 1;
                    return ctx.exit_early;
                }
            }
            PatternKind::And => {
                let offset = offset as usize;
                // SAFETY: `rule_idx` is in bounds — guaranteed by debug_asserts above.
                let word_state = unsafe { ss.word_states.get_unchecked_mut(rule_idx) };

                if shape.has_not() && word_state.not_generation == generation {
                    return false;
                }
                if word_state.positive_generation == generation {
                    if !shape.has_not() && ctx.exit_early {
                        return true;
                    }
                    return false;
                }

                // Inline init: disjoint field borrows keep word_state valid across
                // touched_indices.push() and matrix access.
                if word_state.matrix_generation != generation {
                    let and_count = entry.and_count;
                    word_state.matrix_generation = generation;
                    word_state.positive_generation = if and_count == 0 { generation } else { 0 };
                    word_state.remaining_and = and_count as u16;
                    word_state.satisfied_mask = 0;
                    ss.touched_indices.push(rule_idx);
                    if shape.use_matrix() {
                        // SAFETY: `rule_idx` is in bounds — guaranteed by debug_asserts above.
                        let rule = unsafe { self.hot.get_unchecked(rule_idx) };
                        init_matrix(
                            // SAFETY: `rule_idx` is in bounds — matrix vecs are sized to match rules.
                            unsafe { ss.matrix.get_unchecked_mut(rule_idx) },
                            // SAFETY: ditto.
                            unsafe { ss.matrix_status.get_unchecked_mut(rule_idx) },
                            &rule.segment_counts,
                            ctx.num_variants,
                        );
                    }
                }

                // word_state still valid — no re-load needed.
                let is_satisfied = if shape.use_matrix() {
                    // SAFETY: `rule_idx` is in bounds — matrix vecs are sized to match rules.
                    let flat_matrix = unsafe { ss.matrix.get_unchecked_mut(rule_idx) };
                    // SAFETY: ditto.
                    let flat_status = unsafe { ss.matrix_status.get_unchecked_mut(rule_idx) };
                    let counter = &mut flat_matrix[offset * ctx.num_variants + ctx.text_index];
                    *counter -= 1;
                    if flat_status[offset] == 0 && *counter <= 0 {
                        flat_status[offset] = 1;
                        word_state.remaining_and -= 1;
                        if word_state.remaining_and == 0 {
                            word_state.positive_generation = generation;
                            ss.resolved_count += 1;
                        }
                    }
                    word_state.positive_generation == generation
                } else if matches!(shape, RuleShape::SingleAnd | RuleShape::SingleAndNot) {
                    word_state.positive_generation = generation;
                    ss.resolved_count += 1;
                    true
                } else {
                    let bit = 1u64 << offset;
                    if word_state.satisfied_mask & bit == 0 {
                        word_state.satisfied_mask |= bit;
                        word_state.remaining_and -= 1;
                        if word_state.remaining_and == 0 {
                            word_state.positive_generation = generation;
                            ss.resolved_count += 1;
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
                let word_state = unsafe { ss.word_states.get_unchecked_mut(rule_idx) };

                if word_state.not_generation == generation {
                    return false;
                }

                // Inline init (same rationale as AND path).
                if word_state.matrix_generation != generation {
                    let and_count = entry.and_count;
                    word_state.matrix_generation = generation;
                    word_state.positive_generation = if and_count == 0 { generation } else { 0 };
                    word_state.remaining_and = and_count as u16;
                    word_state.satisfied_mask = 0;
                    ss.touched_indices.push(rule_idx);
                    if shape.use_matrix() {
                        // SAFETY: `rule_idx` is in bounds — guaranteed by debug_asserts above.
                        let rule = unsafe { self.hot.get_unchecked(rule_idx) };
                        init_matrix(
                            // SAFETY: `rule_idx` is in bounds — matrix vecs are sized to match rules.
                            unsafe { ss.matrix.get_unchecked_mut(rule_idx) },
                            // SAFETY: ditto.
                            unsafe { ss.matrix_status.get_unchecked_mut(rule_idx) },
                            &rule.segment_counts,
                            ctx.num_variants,
                        );
                    }
                }

                // word_state still valid — no re-load needed.
                if shape.use_matrix() {
                    // SAFETY: `rule_idx` is in bounds — matrix vecs are sized to match rules.
                    let flat_matrix = unsafe { ss.matrix.get_unchecked_mut(rule_idx) };
                    // SAFETY: ditto.
                    let flat_status = unsafe { ss.matrix_status.get_unchecked_mut(rule_idx) };
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

#[cfg(test)]
mod tests {
    use super::super::state::SimpleMatchState;
    use super::*;

    fn make_ctx(exit_early: bool) -> ScanContext {
        ScanContext {
            text_index: 0,
            process_type_mask: u64::MAX,
            num_variants: 1,
            exit_early,
            non_ascii_density: 0.0,
        }
    }

    fn make_simple_ruleset(word_id: u32, word: &str) -> RuleSet {
        RuleSet::new(
            vec![RuleHot {
                segment_counts: vec![1],
            }],
            vec![RuleCold {
                word_id,
                word: word.to_owned(),
            }],
            false,
        )
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
        let mut ss = state.as_scan_state();

        let entry = PatternEntry {
            rule_idx: 0,
            offset: 0,
            pt_index: 0,
            kind: PatternKind::Simple,
            shape: RuleShape::SingleAnd,
            boundary: 0,
            and_count: 1,
        };

        let result = rules.process_entry(&entry, make_ctx(true), &mut ss);
        assert!(result, "Simple entry with exit_early should return true");
        assert!(ss.rule_is_satisfied(0));

        let result2 = rules.process_entry(&entry, make_ctx(true), &mut ss);
        assert!(
            result2,
            "already-satisfied Simple should still return exit_early"
        );
    }

    #[test]
    fn test_process_entry_and_bitmask() {
        let rules = RuleSet::new(
            vec![RuleHot {
                segment_counts: vec![1, 1, 1],
            }],
            vec![RuleCold {
                word_id: 1,
                word: "a&b&c".to_owned(),
            }],
            false,
        );
        let mut state = SimpleMatchState::new();
        state.prepare(1);
        let mut ss = state.as_scan_state();
        let ctx = make_ctx(true);

        let e0 = PatternEntry {
            rule_idx: 0,
            offset: 0,
            pt_index: 0,
            kind: PatternKind::And,
            shape: RuleShape::Bitmask,
            boundary: 0,
            and_count: 3,
        };
        assert!(!rules.process_entry(&e0, ctx, &mut ss));
        assert!(!ss.rule_is_satisfied(0));

        let e1 = PatternEntry { offset: 1, ..e0 };
        assert!(!rules.process_entry(&e1, ctx, &mut ss));
        assert!(!ss.rule_is_satisfied(0));

        let e2 = PatternEntry { offset: 2, ..e0 };
        assert!(rules.process_entry(&e2, ctx, &mut ss));
        assert!(ss.rule_is_satisfied(0));
    }

    #[test]
    fn test_process_entry_not_veto() {
        let rules = RuleSet::new(
            vec![RuleHot {
                segment_counts: vec![1, 0],
            }],
            vec![RuleCold {
                word_id: 1,
                word: "a~b".to_owned(),
            }],
            true,
        );
        let mut state = SimpleMatchState::new();
        state.prepare(1);
        let mut ss = state.as_scan_state();
        let ctx = make_ctx(false);

        let and_entry = PatternEntry {
            rule_idx: 0,
            offset: 0,
            pt_index: 0,
            kind: PatternKind::And,
            shape: RuleShape::SingleAndNot,
            boundary: 0,
            and_count: 1,
        };
        rules.process_entry(&and_entry, ctx, &mut ss);
        assert!(ss.rule_is_satisfied(0));

        let not_entry = PatternEntry {
            rule_idx: 0,
            offset: 1,
            pt_index: 0,
            kind: PatternKind::Not,
            shape: RuleShape::SingleAndNot,
            boundary: 0,
            and_count: 1,
        };
        rules.process_entry(&not_entry, ctx, &mut ss);
        assert!(!ss.rule_is_satisfied(0), "NOT should veto the rule");
    }

    #[test]
    fn test_process_entry_matrix_counters() {
        let rules = RuleSet::new(
            vec![RuleHot {
                segment_counts: vec![2, 1],
            }],
            vec![RuleCold {
                word_id: 1,
                word: "a&a&b".to_owned(),
            }],
            false,
        );
        let mut state = SimpleMatchState::new();
        state.prepare(1);
        let mut ss = state.as_scan_state();
        let ctx = make_ctx(true);

        let seg0 = PatternEntry {
            rule_idx: 0,
            offset: 0,
            pt_index: 0,
            kind: PatternKind::And,
            shape: RuleShape::Matrix,
            boundary: 0,
            and_count: 2,
        };
        let seg1 = PatternEntry {
            rule_idx: 0,
            offset: 1,
            pt_index: 0,
            kind: PatternKind::And,
            shape: RuleShape::Matrix,
            boundary: 0,
            and_count: 2,
        };

        assert!(!rules.process_entry(&seg0, ctx, &mut ss));
        assert!(!ss.rule_is_satisfied(0));

        assert!(!rules.process_entry(&seg1, ctx, &mut ss));
        assert!(!ss.rule_is_satisfied(0));

        assert!(rules.process_entry(&seg0, ctx, &mut ss));
        assert!(ss.rule_is_satisfied(0));
    }

    #[test]
    fn test_process_entry_pt_mask_filters() {
        let rules = make_simple_ruleset(1, "hello");
        let mut state = SimpleMatchState::new();
        state.prepare(1);
        let mut ss = state.as_scan_state();

        let entry = PatternEntry {
            rule_idx: 0,
            offset: 0,
            pt_index: 3,
            kind: PatternKind::Simple,
            shape: RuleShape::SingleAnd,
            boundary: 0,
            and_count: 1,
        };

        let ctx = ScanContext {
            text_index: 0,
            process_type_mask: 0b0101,
            num_variants: 1,
            exit_early: true,
            non_ascii_density: 0.0,
        };
        assert!(!rules.process_entry(&entry, ctx, &mut ss));
        assert!(!ss.rule_is_satisfied(0), "entry should be filtered by mask");
    }
}
