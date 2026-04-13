//! Thread-local scan state for [`super::SimpleMatcher`].
//!
//! All mutable state needed during a scan is kept in a single
//! [`SimpleMatchState`] instance per thread, accessed through the
//! `#[thread_local]` static [`SIMPLE_MATCH_STATE`]. This avoids per-call
//! allocation: the backing storage grows monotonically and is reused across
//! matchers and across calls.
//!
//! # Generation-based state reset
//!
//! Instead of zeroing every [`RuleState`] between calls, a monotonic
//! `generation` counter is bumped in [`SimpleMatchState::prepare`]. A rule's
//! state is "live" only when its stored generation stamp matches the current
//! generation. Stale entries are effectively invisible, giving O(1) amortized
//! reset cost.
//!
//! When `generation` wraps to `u16::MAX`, all stamps are reset to 0 and the
//! counter restarts at 1. Using `u16` keeps the generation field at 2 bytes
//! (fits 10K rules in 60KB — within L1d cache). The bulk-reset fires every ~65K
//! scans (~20µs amortized to <1ns per scan).
//!
//! ```text
//! Call 1 (gen=1): touch rules [0, 3, 7] → only word_states[0,3,7] stamped gen=1
//! Call 2 (gen=2): touch rules [1, 3]    → word_states[0,7] still stamped gen=1 (stale)
//!                                         word_states[1,3] stamped gen=2 (live)
//! // No zeroing needed between calls — stale stamps are simply ignored.
//! ```

use std::cell::UnsafeCell;

/// Per-rule mutable state reused across scans.
///
/// Each rule has one `RuleState` slot in [`SimpleMatchState::rule_states`],
/// indexed by `rule_idx`. All fields the hot path needs are co-located so
/// that a single cache-line load covers generation check, countdown update,
/// AND bitmask update — eliminating the second cache-line load that the
/// previous split layout (`WordState` + separate `satisfied_masks`) required.
#[derive(Default, Clone, Copy)]
pub(super) struct RuleState {
    /// Scan generation in which this rule was initialized.
    ///
    /// Set to the current generation on first touch. If it does not match,
    /// all other fields are stale and must not be read.
    pub(super) generation: u16,
    /// Remaining AND segments needed before the rule can fire.
    ///
    /// Initialized to the rule's `and_count`. The rule is "positive" (all AND
    /// segments satisfied) when this reaches zero.
    pub(super) remaining_and: u16,
    /// Whether a NOT segment has vetoed this rule.
    ///
    /// Once set, the rule cannot fire regardless of how many AND segments
    /// match. Only meaningful when `generation == current_generation`.
    pub(super) vetoed: bool,
    /// Bitmask tracking which AND segments are satisfied.
    ///
    /// Bit `i` is set when segment `i` has been observed. Only used for
    /// [`SatisfactionMethod::Bitmask`](super::rule::SatisfactionMethod::Bitmask)
    /// rules (≤64 segments, each needing exactly one hit).
    pub(super) satisfied_mask: u64,
}

/// Thread-local state reused by every [`super::SimpleMatcher`] call on one
/// thread.
///
/// Backing storage grows monotonically to accommodate the largest rule set seen
/// on this thread. Between calls, only [`prepare`](Self::prepare) is needed —
/// it bumps the generation and clears the touched-indices list without touching
/// the bulk arrays.
///
/// # Matrix layout
///
/// For rules with `SatisfactionMethod::Matrix`,
/// `matrix[rule_idx]` is a flat 2-D array of shape `[num_segments ×
/// num_variants]` stored in row-major order. Each cell starts at the segment's
/// required count and is decremented (AND) or incremented (NOT) on each hit.
/// `matrix_status[rule_idx]` is a parallel 1-D array of per-segment completion
/// flags.
pub(super) struct SimpleMatchState {
    /// Fused per-rule state: generation + countdown + veto + bitmask in one
    /// 16-byte struct. Indexed by rule id. One cache-line load covers all
    /// fields that `eval_hit` needs.
    pub(super) rule_states: Vec<RuleState>,
    /// Per-variant counter matrix for complex rules (one `Vec` per rule
    /// index).
    ///
    /// `matrix[rule_idx][segment * num_variants + variant_idx]` holds the
    /// remaining count for that segment in that variant. Initialized lazily
    /// on first touch.
    pub(super) matrix: Vec<Vec<i32>>,
    /// Per-segment completion flags for complex rules (one `Vec` per rule
    /// index).
    ///
    /// `matrix_status[rule_idx][segment]` is 0 if the segment is still pending,
    /// 1 if it has been satisfied (AND) or triggered (NOT).
    pub(super) matrix_status: Vec<Vec<u8>>,
    /// Rule indices touched during the current scan generation.
    ///
    /// Cleared at the start of each scan in [`prepare`](Self::prepare). Used by
    /// [`RuleSet::collect_matches`](super::rule::RuleSet::collect_matches) and
    /// [`ScanState::has_match`] to iterate only
    /// over rules that received at least one pattern hit.
    pub(super) touched_indices: Vec<usize>,
    /// Monotonic generation id used to avoid clearing full state between calls.
    generation: u16,
}

/// Thread-local reusable scan state shared by all matchers on the current
/// thread.
///
/// # Safety
///
/// The `UnsafeCell` is safe to use here because:
///
/// 1. `#[thread_local]` guarantees that each thread has its own instance — no
///    cross-thread sharing occurs.
/// 2. The scan functions that access this static (`is_match_inner`,
///    `process_simple`, `process_preprocessed_into`) are not re-entrant: they
///    obtain a `&mut` reference via `SIMPLE_MATCH_STATE.get()` at the top of
///    the call and hold it for the entire duration. No callback or nested call
///    re-enters the same path.
///
/// This pattern avoids the overhead of `RefCell` on the scan hot path.
#[thread_local]
pub(super) static SIMPLE_MATCH_STATE: UnsafeCell<SimpleMatchState> =
    UnsafeCell::new(SimpleMatchState::new());

/// Split-borrow view into [`SimpleMatchState`] for the scan hot path.
///
/// Created by [`SimpleMatchState::as_scan_state`] after
/// [`prepare`](SimpleMatchState::prepare). By storing mutable slices (not
/// `Vec`s), the compiler can cache base pointers in registers — eliminating
/// repeated `Vec::get_unchecked_mut` pointer resolution that otherwise accounts
/// for 10–16% of runtime in profiled `process` workloads.
///
/// Disjoint field borrows (e.g. `self.word_states[i]` and
/// `self.touched_indices.push()`) are sound because the compiler can see they
/// target different struct fields.
pub(super) struct ScanState<'a> {
    pub(super) rule_states: &'a mut [RuleState],
    pub(super) touched_indices: &'a mut Vec<usize>,
    pub(super) matrix: &'a mut [Vec<i32>],
    pub(super) matrix_status: &'a mut [Vec<u8>],
    pub(super) generation: u16,
}

/// Walk-level constants that stay fixed for the entire tree walk.
///
/// Extracted from [`ScanContext`] so that walk-loop helpers can accept these
/// constants without repeating them as individual parameters. Use
/// [`scan_ctx`](WalkConfig::scan_ctx) to construct a per-variant
/// [`ScanContext`] from these constants.
#[derive(Clone, Copy)]
pub(super) struct WalkConfig {
    /// Total number of transformed variants participating in this scan.
    pub(super) num_variants: usize,
    /// Whether the caller may stop on the first satisfied rule.
    pub(super) exit_early: bool,
}

impl WalkConfig {
    /// Constructs a per-variant [`ScanContext`] from walk constants and
    /// variant-specific values.
    #[inline(always)]
    pub(super) fn scan_ctx(
        self,
        text_index: usize,
        process_type_mask: u64,
        char_density: f32,
    ) -> ScanContext {
        ScanContext {
            text_index,
            process_type_mask,
            num_variants: self.num_variants,
            exit_early: self.exit_early,
            char_density,
        }
    }
}

/// Per-variant scan metadata passed through the hot match-processing path.
///
/// One `ScanContext` is constructed per text variant and threaded through
/// `RuleSet::eval_hit` for every hit in that variant.
///
/// Intentionally `Copy` (32 bytes) so that values live in registers during the
/// hot eval loop, avoiding pointer-chase overhead. Passed by value, not by
/// reference.
///
/// Contains both walk-level constants (from [`WalkConfig`]) and per-variant
/// values, bundled for efficient passthrough on the hot path.
#[derive(Clone, Copy)]
pub(super) struct ScanContext {
    /// Index of the current transformed text variant.
    ///
    /// Used as the column index into [`SimpleMatchState::matrix`] for
    /// matrix-mode rules.
    pub(super) text_index: usize,
    /// Bitmask of compact process-type indices that contributed to this
    /// variant.
    ///
    /// Bit `i` is set if the variant was produced by (or is reachable from) the
    /// process type whose compact index is `i`. Checked against
    /// [`PatternEntry::process_type_index`](super::pattern::PatternEntry::process_type_index) to
    /// filter hits from irrelevant variants.
    pub(super) process_type_mask: u64,
    /// Total number of transformed variants participating in this scan.
    ///
    /// Determines the number of columns in the matrix for matrix-mode rules.
    pub(super) num_variants: usize,
    /// Whether the caller may stop on the first satisfied rule.
    ///
    /// `true` for `is_match` calls; `false` for `process` calls that must
    /// collect all matching rules.
    pub(super) exit_early: bool,
    /// Character density of the current variant (1.0 = pure ASCII, lower
    /// values indicate more multi-byte characters).
    ///
    /// Passed through to
    /// [`ScanPlan::for_each_match_value`](super::scan::ScanPlan::for_each_match_value)
    /// to select the bytewise or charwise automaton. Computed once at the root
    /// via SIMD (`bytecount::num_chars`), then propagated through the
    /// transform tree via the density estimate returned by
    /// [`TransformStep::apply`](crate::process::step::TransformStep::apply).
    pub(super) char_density: f32,
}

/// Hot-path methods on the split-borrow scan state.
impl ScanState<'_> {
    /// Returns the rules touched during the current generation.
    pub(super) fn touched_indices(&self) -> &[usize] {
        self.touched_indices
    }

    /// Returns whether any touched rule is satisfied in the current generation.
    #[inline(always)]
    pub(super) fn has_match(&self) -> bool {
        self.touched_indices()
            .iter()
            .any(|&rule_idx| self.rule_is_satisfied(rule_idx))
    }

    /// Returns whether `rule_idx` is satisfied in the current generation.
    #[inline(always)]
    pub(super) fn rule_is_satisfied(&self, rule_idx: usize) -> bool {
        // SAFETY: `rule_idx` originates from `touched_indices`, bounded by
        // `prepare(size)`.
        unsafe { core::hint::assert_unchecked(rule_idx < self.rule_states.len()) };
        let rs = &self.rule_states[rule_idx];
        rs.generation == self.generation && rs.remaining_and == 0 && !rs.vetoed
    }
}

/// Test-only convenience methods. The hot path inlines these operations
/// directly to preserve `&mut RuleState` references across disjoint field
/// borrows.
#[cfg(test)]
impl ScanState<'_> {
    pub(super) fn generation(&self) -> u16 {
        self.generation
    }

    pub(super) fn init_rule(
        &mut self,
        rule: &super::rule::Rule,
        and_count: usize,
        rule_idx: usize,
        ctx: ScanContext,
    ) {
        let generation = self.generation;
        let rs = &mut self.rule_states[rule_idx];
        rs.generation = generation;
        rs.remaining_and = and_count as u16;
        rs.vetoed = false;
        rs.satisfied_mask = 0;
        self.touched_indices.push(rule_idx);

        let use_matrix = and_count > super::pattern::BITMASK_CAPACITY
            || rule.segment_counts.len() > super::pattern::BITMASK_CAPACITY
            || rule.segment_counts[..and_count].iter().any(|&v| v != 1)
            || rule.segment_counts[and_count..].iter().any(|&v| v != 0);
        if use_matrix {
            init_matrix(
                &mut self.matrix[rule_idx],
                &mut self.matrix_status[rule_idx],
                &rule.segment_counts,
                ctx.num_variants,
            );
        }
    }
}

/// Lifecycle helpers for the thread-local scan state.
impl SimpleMatchState {
    /// Creates an empty reusable state container with generation 0.
    ///
    /// All backing vectors start empty and grow on the first call to
    /// [`prepare`](Self::prepare).
    pub(super) const fn new() -> Self {
        Self {
            rule_states: Vec::new(),
            matrix: Vec::new(),
            matrix_status: Vec::new(),
            touched_indices: Vec::new(),
            generation: 0,
        }
    }

    /// Advances the generation and grows backing storage for at least `size`
    /// rules.
    ///
    /// Must be called exactly once at the start of every scan before any state
    /// is read. On `u16::MAX` overflow, all generation stamps are
    /// bulk-reset to 0 and the counter restarts at 1. This fires every ~65K
    /// scans — the cost (~20µs for 10K rules) amortizes to <1ns per scan.
    pub(super) fn prepare(&mut self, size: usize) {
        if self.generation == u16::MAX {
            for rs in self.rule_states.iter_mut() {
                rs.generation = 0;
            }
            self.generation = 1;
        } else {
            self.generation += 1;
        }

        if self.rule_states.len() < size {
            self.rule_states.resize(size, RuleState::default());
            self.matrix.resize(size, Vec::new());
            self.matrix_status.resize(size, Vec::new());
        }

        self.touched_indices.clear();
    }

    /// Creates a [`ScanState`] split-borrow view for the scan hot path.
    ///
    /// Must be called after [`prepare`](Self::prepare). The returned
    /// `ScanState` borrows individual fields as mutable slices, allowing
    /// the compiler to cache base pointers in registers instead of
    /// re-resolving Vec metadata on every access.
    #[inline(always)]
    pub(super) fn as_scan_state(&mut self) -> ScanState<'_> {
        ScanState {
            rule_states: &mut self.rule_states,
            touched_indices: &mut self.touched_indices,
            matrix: &mut self.matrix,
            matrix_status: &mut self.matrix_status,
            generation: self.generation,
        }
    }
}

/// Initializes the per-variant counter matrix for a complex rule.
///
/// Allocates (or re-sizes) `flat_matrix` to `num_segments × num_variants` cells
/// and fills each row with the segment's required count from `segment_counts`.
/// Resets `flat_status` to all-zero (no segment satisfied yet).
///
/// Marked `#[cold]` because matrix-mode rules are rare — most rules use the
/// bitmask fast path.
#[cold]
#[inline(never)]
pub(super) fn init_matrix(
    flat_matrix: &mut Vec<i32>,
    flat_status: &mut Vec<u8>,
    segment_counts: &[i32],
    num_variants: usize,
) {
    let num_splits = segment_counts.len();
    flat_matrix.clear();
    flat_matrix.resize(num_splits * num_variants, 0i32);
    flat_status.clear();
    flat_status.resize(num_splits, 0u8);

    for (split_idx, &count) in segment_counts.iter().enumerate() {
        let row_start = split_idx * num_variants;
        flat_matrix[row_start..row_start + num_variants].fill(count);
    }
}

#[cfg(test)]
mod tests {
    use super::{super::rule::Rule, *};

    fn make_ctx(num_variants: usize, exit_early: bool) -> ScanContext {
        ScanContext {
            text_index: 0,
            process_type_mask: u64::MAX,
            num_variants,
            exit_early,
            char_density: 1.0,
        }
    }

    #[test]
    fn test_prepare() {
        let mut state = SimpleMatchState::new();
        assert_eq!(state.generation, 0);

        // First prepare: grows storage, sets generation to 1
        state.prepare(10);
        assert!(state.rule_states.len() >= 10);
        assert!(state.matrix.len() >= 10);
        assert_eq!(state.generation, 1);
        assert!(state.touched_indices.is_empty());

        // Subsequent prepares increment generation
        state.prepare(10);
        assert_eq!(state.generation, 2);
        state.prepare(10);
        assert_eq!(state.generation, 3);
    }

    #[test]
    fn test_prepare_generation_wraparound() {
        let mut state = SimpleMatchState::new();
        state.prepare(3);
        let current = state.generation;
        state.rule_states[0].generation = current;
        state.rule_states[1].generation = current;
        state.rule_states[2].generation = current;

        state.generation = u16::MAX - 1;
        state.prepare(3);
        assert_eq!(state.generation, u16::MAX);

        state.prepare(3);
        assert_eq!(state.generation, 1);
        for rs in &state.rule_states {
            assert_eq!(rs.generation, 0);
        }
    }

    #[test]
    fn test_rule_satisfaction() {
        let mut state = SimpleMatchState::new();
        state.prepare(1);
        let current = state.generation;

        // Satisfied: generation matches, remaining_and=0, not vetoed
        state.rule_states[0].generation = current;
        state.rule_states[0].remaining_and = 0;
        state.rule_states[0].vetoed = false;
        assert!(state.as_scan_state().rule_is_satisfied(0));

        // Vetoed: same but vetoed=true → not satisfied
        state.rule_states[0].vetoed = true;
        assert!(!state.as_scan_state().rule_is_satisfied(0));
    }

    #[test]
    fn test_init_rule_matrix() {
        let mut state = SimpleMatchState::new();
        state.prepare(1);

        let rule = Rule {
            segment_counts: vec![2, 1, 0],
            rule_id: 1,
            pattern: "a&a&b~c".to_owned(),
        };
        let ctx = make_ctx(2, false);
        let mut ss = state.as_scan_state();
        ss.init_rule(&rule, 2, 0, ctx);

        assert_eq!(ss.rule_states[0].generation, ss.generation());
        assert_eq!(ss.rule_states[0].remaining_and, 2);
        assert!(!ss.rule_states[0].vetoed);
        assert_eq!(ss.rule_states[0].satisfied_mask, 0);
        assert_eq!(ss.touched_indices(), &[0]);

        assert_eq!(ss.matrix[0].len(), 6);
        assert_eq!(&ss.matrix[0][..], &[2, 2, 1, 1, 0, 0]);
        assert_eq!(ss.matrix_status[0].len(), 3);
        assert!(ss.matrix_status[0].iter().all(|&s| s == 0));
    }
}
