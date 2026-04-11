//! Rule metadata and rule-state transition logic.
//!
//! This module contains the types that bind rule metadata to the scan state
//! machine. [`RuleSet`] owns the rule metadata vector and exposes
//! `eval_hit` — the core state-transition
//! function that tracks bitmasks, matrix counters, and generation stamps in the
//! thread-local [`super::state::SimpleMatchState`].
//!
//! Pattern types ([`super::pattern::PatternEntry`],
//! [`super::pattern::PatternIndex`], [`super::pattern::PatternDispatch`]) live
//! in [`super::pattern`].

use std::{borrow::Cow, collections::HashMap};

use super::{
    SimpleResult,
    pattern::PatternKind,
    state::{ScanContext, ScanState, init_matrix},
};
use crate::process::ProcessType;

/// Raw table format accepted by
/// [`SimpleMatcher::new`](super::SimpleMatcher::new).
///
/// The outer key is the [`ProcessType`] that governs which text-transformation
/// pipeline to apply before matching. The inner key is a caller-chosen rule id
/// (`word_id`) that will be returned in
/// [`SimpleResult::word_id`](super::SimpleResult::word_id) on a match. The
/// inner value is the pattern string, which may contain `&` (AND), `~` (NOT),
/// and `|` (OR) operators to combine sub-patterns. `|` binds tighter than
/// `&`/`~`: `"a|b&c"` means (a OR b) AND c.
///
/// This is the borrowed-string variant — all pattern strings must outlive the
/// table reference passed to [`SimpleMatcher::new`](super::SimpleMatcher::new).
/// For owned or deserialized strings, use [`SimpleTableSerde`] instead.
///
/// # Examples
///
/// ```rust
/// use std::collections::HashMap;
///
/// use matcher_rs::{ProcessType, SimpleMatcher, SimpleTable};
///
/// let mut table: SimpleTable = HashMap::new();
/// table
///     .entry(ProcessType::None)
///     .or_default()
///     .insert(1, "hello");
/// table
///     .entry(ProcessType::None)
///     .or_default()
///     .insert(2, "apple&pie");
/// table
///     .entry(ProcessType::VariantNorm)
///     .or_default()
///     .insert(3, "你好");
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
/// use std::{borrow::Cow, collections::HashMap};
///
/// use matcher_rs::{ProcessType, SimpleMatcher, SimpleTableSerde};
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
/// ```
///
/// With the `serde` feature enabled, the table can be deserialized from JSON
/// (ProcessType serializes as its raw u8 bits):
///
/// ```rust,ignore
/// use matcher_rs::{ProcessType, SimpleMatcher, SimpleTableSerde};
///
/// let json = r#"{"1":{"1":"hello","2":"world"}}"#;
/// let deserialized: SimpleTableSerde = serde_json::from_str(json).unwrap();
/// let matcher = SimpleMatcher::new(&deserialized).unwrap();
/// assert!(matcher.is_match("hello world"));
/// ```
pub type SimpleTableSerde<'a> = HashMap<ProcessType, HashMap<u32, Cow<'a, str>>>;

/// How a rule tracks segment satisfaction on the hot path.
///
/// Stored in [`RuleInfo::method`] for shape dispatch in
/// [`RuleSet::eval_hit`]. Three strategies cover all rule shapes:
///
/// - [`Immediate`](Self::Immediate): single AND segment → mark positive on
///   first hit.
/// - [`Bitmask`](Self::Bitmask): ≤64 segments with unique counts → bit
///   test-and-set in a `u64`.
/// - [`Matrix`](Self::Matrix): repeated sub-patterns or >64 segments →
///   per-variant counter matrix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub(super) enum SatisfactionMethod {
    /// Single AND segment — mark positive immediately on first hit.
    Immediate = 0,
    /// Multiple segments (≤64), each needing exactly one hit — track via
    /// `u64` bitmask.
    Bitmask = 1,
    /// Per-variant counter matrix — for repeated sub-patterns or >64
    /// segments.
    Matrix = 2,
}

impl SatisfactionMethod {
    /// Whether this method requires the per-variant counter matrix.
    #[inline(always)]
    pub(super) fn use_matrix(self) -> bool {
        matches!(self, Self::Matrix)
    }
}

/// Per-rule metadata: segment counts for matrix initialization and result
/// fields for output production.
///
/// The `segment_counts` layout is:
/// ```text
/// [ and_0, and_1, …, and_{and_count-1}, not_0, not_1, … ]
///   ╰──── positive (AND) segments ────╯  ╰── negative (NOT) ──╯
/// ```
/// Positive counts start at the required number of hits (usually 1); negative
/// counts start at 0 and veto the rule when they go above 0 (for matrix mode)
/// or on first hit (for bitmask mode).
///
/// `segment_counts` is only read on first-touch of a matrix-mode rule (the
/// `#[cold]` `init_matrix` path). `word_id` and `word` are only read when
/// producing result output after scanning completes.
#[derive(Debug, Clone)]
pub(super) struct Rule {
    /// Required counts for every positive and negative segment in rule order.
    ///
    /// AND entries hold the required hit count (decremented toward zero);
    /// NOT entries hold a starting value of 0 (incremented on hit; any positive
    /// value means the veto segment was observed). Only read when
    /// `RuleShape::use_matrix()` is true.
    pub(super) segment_counts: Vec<i32>,
    /// Caller-supplied rule identifier returned in match results.
    pub(super) word_id: u32,
    /// Original rule string stored for borrowed result output.
    ///
    /// Owned here so that [`SimpleResult::word`](super::SimpleResult::word) can
    /// borrow it as `Cow::Borrowed`.
    pub(super) word: String,
}

/// Per-rule compact metadata loaded on the hot evaluation path.
///
/// Stored contiguously in [`RuleSet::rule_info`] and indexed by `rule_idx`.
/// At 3 bytes (padded to 4), this keeps the hot-path data dense and
/// cache-friendly — the full [`Rule`] struct is only loaded on the cold
/// matrix-init and result-collection paths.
#[derive(Debug, Clone, Copy)]
pub(super) struct RuleInfo {
    /// Number of positive (AND) segments in this rule.
    pub(super) and_count: u8,
    /// Satisfaction tracking strategy.
    pub(super) method: SatisfactionMethod,
    /// Whether this rule contains at least one NOT (`~`) segment.
    pub(super) has_not: bool,
}

/// All metadata for the compiled rule set.
///
/// `rules` is a `Vec` indexed by rule index. `segment_counts` is read on the
/// `#[cold]` matrix-init path; `word_id` and `word` are read only when
/// producing output results. `rule_info` holds the hot-path metadata loaded
/// during evaluation.
#[derive(Clone)]
pub(super) struct RuleSet {
    rules: Vec<Rule>,
    rule_info: Vec<RuleInfo>,
}

/// Rule-evaluation helpers used by the scan hot path.
impl RuleSet {
    /// Creates the compiled rule set from rule metadata and hot-path info.
    pub(super) fn new(rules: Vec<Rule>, rule_info: Vec<RuleInfo>) -> Self {
        debug_assert_eq!(rules.len(), rule_info.len());
        Self { rules, rule_info }
    }

    /// Returns the full rule info slice (used during construction).
    pub(super) fn rule_info(&self) -> &[RuleInfo] {
        &self.rule_info
    }

    /// Returns the hot-path metadata for a given rule index.
    #[inline(always)]
    pub(super) fn info(&self, rule_idx: usize) -> RuleInfo {
        // SAFETY: `rule_idx` originates from a valid scan hit; bounded by construction.
        unsafe { core::hint::assert_unchecked(rule_idx < self.rule_info.len()) };
        self.rule_info[rule_idx]
    }

    /// Returns the estimated heap memory in bytes owned by this rule set.
    pub(super) fn heap_bytes(&self) -> usize {
        let inner: usize = self
            .rules
            .iter()
            .map(|r| r.segment_counts.capacity() * size_of::<i32>() + r.word.capacity())
            .sum();
        self.rules.capacity() * size_of::<Rule>()
            + self.rule_info.capacity() * size_of::<RuleInfo>()
            + inner
    }

    /// Returns the number of compiled rules.
    #[inline(always)]
    pub(super) fn len(&self) -> usize {
        self.rules.len()
    }

    /// Returns whether any touched rule is satisfied in the current generation.
    #[inline(always)]
    pub(super) fn has_match(&self, ss: &ScanState<'_>) -> bool {
        ss.touched_indices()
            .iter()
            .any(|&rule_idx| ss.rule_is_satisfied(rule_idx))
    }

    /// Appends every satisfied touched rule to `results`.
    pub(super) fn collect_matches<'a>(
        &'a self,
        ss: &ScanState<'_>,
        results: &mut Vec<SimpleResult<'a>>,
    ) {
        for &rule_idx in ss.touched_indices() {
            if ss.rule_is_satisfied(rule_idx) {
                results.push(self.result_at(rule_idx));
            }
        }
    }

    /// Calls `on_match` for each satisfied touched rule.
    ///
    /// Returns `true` if the callback requested early exit.
    pub(super) fn for_each_satisfied<'a>(
        &'a self,
        ss: &ScanState<'_>,
        mut on_match: impl FnMut(SimpleResult<'a>) -> bool,
    ) -> bool {
        for &rule_idx in ss.touched_indices() {
            if ss.rule_is_satisfied(rule_idx) && on_match(self.result_at(rule_idx)) {
                return true;
            }
        }
        false
    }

    /// Produces a [`SimpleResult`] for a given rule index.
    ///
    /// # Safety (internal)
    ///
    /// The caller must ensure `rule_idx` originated from a valid scan (e.g.
    /// `touched_indices`).
    #[inline(always)]
    pub(super) fn result_at<'a>(&'a self, rule_idx: usize) -> SimpleResult<'a> {
        // SAFETY: `rule_idx` originates from a valid scan (e.g. `touched_indices`).
        unsafe { core::hint::assert_unchecked(rule_idx < self.rules.len()) };
        let rule = &self.rules[rule_idx];
        SimpleResult {
            word_id: rule.word_id,
            word: Cow::Borrowed(&rule.word),
        }
    }

    /// Unified rule evaluation: processes one pattern hit.
    ///
    /// This is the single entry point for all rule state transitions. Both
    /// direct-encoded and indirect pattern hits are routed here after the
    /// caller has already verified process-type mask and word boundary
    /// conditions.
    ///
    /// Returns `true` only when the caller may stop early because a
    /// non-vetoed rule is already satisfied and `exit_early` is set.
    ///
    /// # Control flow
    ///
    /// One generation comparison decides init-vs-process:
    ///
    /// - `ws.generation == current`: rule already touched this scan. Check
    ///   vetoed / remaining_and before processing the hit.
    /// - `ws.generation != current`: first touch. Initialize state, then
    ///   process.
    ///
    /// # Safety (internal)
    ///
    /// Uses `get_unchecked` on all rule-indexed arrays. `rule_idx` is
    /// guaranteed in bounds by construction in
    /// [`SimpleMatcher::new`](super::SimpleMatcher::new).
    #[inline(always)]
    pub(super) fn eval_hit(
        &self,
        rule_idx: usize,
        kind: PatternKind,
        offset: usize,
        ctx: ScanContext,
        ss: &mut ScanState<'_>,
    ) -> bool {
        let generation = ss.generation;
        let info = self.info(rule_idx);

        // SAFETY: `rule_idx` is bounded by construction; all parallel arrays have the
        // same length (allocated in `SimpleMatchState::prepare` to at least
        // `rules.len()`).
        unsafe {
            core::hint::assert_unchecked(rule_idx < ss.word_states.len());
            core::hint::assert_unchecked(rule_idx < ss.satisfied_masks.len());
            core::hint::assert_unchecked(rule_idx < ss.matrix.len());
            core::hint::assert_unchecked(rule_idx < ss.matrix_status.len());
            core::hint::assert_unchecked(rule_idx < self.rules.len());
        }

        let ws = &mut ss.word_states[rule_idx];

        // ── NOT hit ──────────────────────────────────────────────────
        if matches!(kind, PatternKind::Not) {
            if ws.generation == generation {
                if ws.vetoed {
                    return false;
                }
            } else {
                ws.generation = generation;
                ws.remaining_and = info.and_count as u16;
                ws.vetoed = false;
                ss.satisfied_masks[rule_idx] = 0;
                ss.touched_indices.push(rule_idx);
                if info.method.use_matrix() {
                    init_matrix(
                        &mut ss.matrix[rule_idx],
                        &mut ss.matrix_status[rule_idx],
                        &self.rules[rule_idx].segment_counts,
                        ctx.num_variants,
                    );
                }
            }

            if info.method.use_matrix() {
                let flat_matrix = &mut ss.matrix[rule_idx];
                let flat_status = &mut ss.matrix_status[rule_idx];
                let counter = &mut flat_matrix[offset * ctx.num_variants + ctx.text_index];
                *counter += 1;
                if flat_status[offset] == 0 && *counter > 0 {
                    flat_status[offset] = 1;
                    ws.vetoed = true;
                }
            } else {
                ws.vetoed = true;
            }

            return false;
        }

        // ── AND hit ──────────────────────────────────────────────────

        if ws.generation == generation {
            if info.has_not && ws.vetoed {
                return false;
            }
            if ws.remaining_and == 0 {
                return !info.has_not && ctx.exit_early;
            }
        } else {
            ws.generation = generation;
            ws.remaining_and = info.and_count as u16;
            ws.vetoed = false;
            ss.satisfied_masks[rule_idx] = 0;
            ss.touched_indices.push(rule_idx);
            if info.method.use_matrix() {
                init_matrix(
                    &mut ss.matrix[rule_idx],
                    &mut ss.matrix_status[rule_idx],
                    &self.rules[rule_idx].segment_counts,
                    ctx.num_variants,
                );
            }
        }

        let is_satisfied = match info.method {
            SatisfactionMethod::Matrix => {
                let flat_matrix = &mut ss.matrix[rule_idx];
                let flat_status = &mut ss.matrix_status[rule_idx];
                let counter = &mut flat_matrix[offset * ctx.num_variants + ctx.text_index];
                *counter -= 1;
                if flat_status[offset] == 0 && *counter <= 0 {
                    flat_status[offset] = 1;
                    ws.remaining_and -= 1;
                }
                ws.remaining_and == 0
            }
            SatisfactionMethod::Immediate => {
                ws.remaining_and = 0;
                true
            }
            SatisfactionMethod::Bitmask => {
                let bit = 1u64 << offset;
                let mask = &mut ss.satisfied_masks[rule_idx];
                if *mask & bit == 0 {
                    *mask |= bit;
                    ws.remaining_and -= 1;
                }
                ws.remaining_and == 0
            }
        };

        ctx.exit_early && is_satisfied && !info.has_not && !ws.vetoed
    }
}

#[cfg(test)]
mod tests {
    use super::{super::state::SimpleMatchState, *};

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
            vec![Rule {
                segment_counts: vec![1],
                word_id,
                word: word.to_owned(),
            }],
            vec![RuleInfo {
                and_count: 1,
                method: SatisfactionMethod::Immediate,
                has_not: false,
            }],
        )
    }

    // --- eval_hit tests ---

    #[test]
    fn test_eval_hit_simple_kind() {
        let rules = make_simple_ruleset(1, "hello");
        let mut state = SimpleMatchState::new();
        state.prepare(1);
        let mut ss = state.as_scan_state();

        let result = rules.eval_hit(0, PatternKind::And, 0, make_ctx(true), &mut ss);
        assert!(result, "Simple AND with exit_early should return true");
        assert!(ss.rule_is_satisfied(0));

        let result2 = rules.eval_hit(0, PatternKind::And, 0, make_ctx(true), &mut ss);
        assert!(
            result2,
            "already-satisfied Simple should still return exit_early"
        );
    }

    #[test]
    fn test_eval_hit_and_bitmask() {
        let rules = RuleSet::new(
            vec![Rule {
                segment_counts: vec![1, 1, 1],
                word_id: 1,
                word: "a&b&c".to_owned(),
            }],
            vec![RuleInfo {
                and_count: 3,
                method: SatisfactionMethod::Bitmask,
                has_not: false,
            }],
        );
        let mut state = SimpleMatchState::new();
        state.prepare(1);
        let mut ss = state.as_scan_state();
        let ctx = make_ctx(true);

        assert!(!rules.eval_hit(0, PatternKind::And, 0, ctx, &mut ss));
        assert!(!ss.rule_is_satisfied(0));

        assert!(!rules.eval_hit(0, PatternKind::And, 1, ctx, &mut ss));
        assert!(!ss.rule_is_satisfied(0));

        assert!(rules.eval_hit(0, PatternKind::And, 2, ctx, &mut ss));
        assert!(ss.rule_is_satisfied(0));
    }

    #[test]
    fn test_eval_hit_not_veto() {
        let rules = RuleSet::new(
            vec![Rule {
                segment_counts: vec![1, 0],
                word_id: 1,
                word: "a~b".to_owned(),
            }],
            vec![RuleInfo {
                and_count: 1,
                method: SatisfactionMethod::Immediate,
                has_not: true,
            }],
        );
        let mut state = SimpleMatchState::new();
        state.prepare(1);
        let mut ss = state.as_scan_state();
        let ctx = make_ctx(false);

        rules.eval_hit(0, PatternKind::And, 0, ctx, &mut ss);
        assert!(ss.rule_is_satisfied(0));

        rules.eval_hit(0, PatternKind::Not, 1, ctx, &mut ss);
        assert!(!ss.rule_is_satisfied(0), "NOT should veto the rule");
    }

    #[test]
    fn test_eval_hit_matrix_counters() {
        let rules = RuleSet::new(
            vec![Rule {
                segment_counts: vec![2, 1],
                word_id: 1,
                word: "a&a&b".to_owned(),
            }],
            vec![RuleInfo {
                and_count: 2,
                method: SatisfactionMethod::Matrix,
                has_not: false,
            }],
        );
        let mut state = SimpleMatchState::new();
        state.prepare(1);
        let mut ss = state.as_scan_state();
        let ctx = make_ctx(true);

        assert!(!rules.eval_hit(0, PatternKind::And, 0, ctx, &mut ss));
        assert!(!ss.rule_is_satisfied(0));

        assert!(!rules.eval_hit(0, PatternKind::And, 1, ctx, &mut ss));
        assert!(!ss.rule_is_satisfied(0));

        assert!(rules.eval_hit(0, PatternKind::And, 0, ctx, &mut ss));
        assert!(ss.rule_is_satisfied(0));
    }
}
