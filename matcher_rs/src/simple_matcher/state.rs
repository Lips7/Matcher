//! Thread-local scan state for [`super::SimpleMatcher`].
//!
//! All mutable state needed during a scan is kept in a single [`SimpleMatchState`] instance
//! per thread, accessed through the `#[thread_local]` static [`SIMPLE_MATCH_STATE`]. This
//! avoids per-call allocation: the backing storage grows monotonically and is reused across
//! matchers and across calls.
//!
//! # Generation-based state reset
//!
//! Instead of zeroing every [`WordState`] between calls, a monotonic `generation` counter
//! is bumped in [`SimpleMatchState::prepare`]. A rule's state is "live" only when its
//! stored generation stamp matches the current generation. Stale entries are effectively
//! invisible, giving O(1) amortized reset cost.
//!
//! When `generation` wraps to `u32::MAX`, all stamps are reset to 0 and the counter
//! restarts at 1. This happens at most once every ~4 billion calls per thread.
//!
//! ```text
//! Call 1 (gen=1): touch rules [0, 3, 7] → only word_states[0,3,7] stamped gen=1
//! Call 2 (gen=2): touch rules [1, 3]    → word_states[0,7] still stamped gen=1 (stale)
//!                                         word_states[1,3] stamped gen=2 (live)
//! // No zeroing needed between calls — stale stamps are simply ignored.
//! ```

use std::cell::UnsafeCell;

use tinyvec::TinyVec;

/// Per-rule mutable state reused across scans.
///
/// Each rule has one `WordState` slot in [`SimpleMatchState::word_states`], indexed by
/// `rule_idx`. Fields use generation stamps rather than boolean flags so the entire
/// vector can be "reset" by incrementing the global generation counter.
///
/// # Layout
///
/// The three `*_generation` fields track whether the rule has been touched / satisfied /
/// vetoed in the current scan. `satisfied_mask` and `remaining_and` are only meaningful
/// when `matrix_generation == current_generation` (i.e., the rule has been initialized
/// for this scan).
#[derive(Default, Clone, Copy)]
pub(super) struct WordState {
    /// Generation in which the rule's matrix/bitmask state was initialized.
    ///
    /// Set to the current generation on first touch. If it does not match the current
    /// generation, the rest of this struct's fields are stale.
    pub(super) matrix_generation: u32,
    /// Generation in which all positive (AND) requirements became satisfied.
    ///
    /// Set to the current generation when `remaining_and` reaches zero or on a
    /// [`PatternKind::Simple`](super::pattern::PatternKind::Simple) hit. A rule is
    /// considered "satisfied" when `positive_generation == current_generation` and
    /// `not_generation != current_generation`.
    pub(super) positive_generation: u32,
    /// Generation in which a NOT segment vetoed the rule.
    ///
    /// Once set, the rule cannot fire regardless of how many AND segments match.
    pub(super) not_generation: u32,
    /// Bitset fast path for tracking which AND segments have been satisfied.
    ///
    /// Bit `i` is set when segment `i` has been observed at least once. Only used
    /// when the rule does not use the matrix path (i.e., `RuleShape::use_matrix()` is `false`)
    /// and the rule has more than one AND segment.
    pub(super) satisfied_mask: u64,
    /// Remaining AND segments still needed before the rule can fire.
    ///
    /// Initialized to [`RuleHot::and_count`](super::rule::RuleHot::and_count) and
    /// decremented as segments are satisfied. The rule becomes positive when this
    /// reaches zero.
    pub(super) remaining_and: u16,
}

/// Thread-local state reused by every [`super::SimpleMatcher`] call on one thread.
///
/// Backing storage grows monotonically to accommodate the largest rule set seen on this
/// thread. Between calls, only [`prepare`](Self::prepare) is needed — it bumps the
/// generation and clears the touched-indices list without touching the bulk arrays.
///
/// # Matrix layout
///
/// For rules with `RuleShape::use_matrix()` = `true`,
/// `matrix[rule_idx]` is a flat 2-D array of shape `[num_segments × num_variants]`
/// stored in row-major order. Each cell starts at the segment's required count and is
/// decremented (AND) or incremented (NOT) on each hit. `matrix_status[rule_idx]` is a
/// parallel 1-D array of per-segment completion flags.
pub(super) struct SimpleMatchState {
    /// Per-rule state slots indexed by rule id.
    pub(super) word_states: Vec<WordState>,
    /// Per-variant counter matrix for complex rules (one `TinyVec` per rule index).
    ///
    /// `matrix[rule_idx][segment * num_variants + variant_idx]` holds the remaining
    /// count for that segment in that variant. Initialized lazily on first touch.
    pub(super) matrix: Vec<TinyVec<[i32; 16]>>,
    /// Per-segment completion flags for complex rules (one `TinyVec` per rule index).
    ///
    /// `matrix_status[rule_idx][segment]` is 0 if the segment is still pending, 1 if
    /// it has been satisfied (AND) or triggered (NOT).
    pub(super) matrix_status: Vec<TinyVec<[u8; 16]>>,
    /// Rule indices touched during the current scan generation.
    ///
    /// Cleared at the start of each scan in [`prepare`](Self::prepare). Used by
    /// [`RuleSet::collect_matches`](super::rule::RuleSet::collect_matches) and
    /// [`RuleSet::has_match`](super::rule::RuleSet::has_match) to iterate only over
    /// rules that received at least one pattern hit.
    pub(super) touched_indices: Vec<usize>,
    /// Number of rules whose outcome is permanently decided in the current generation.
    ///
    /// Incremented when a rule first reaches `positive_generation == generation` (for
    /// matchers without NOT rules, this is final). Used by `walk_and_scan` to skip
    /// remaining tree variants when all rules are resolved.
    pub(super) resolved_count: usize,
    /// Monotonic generation id used to avoid clearing full state between calls.
    generation: u32,
}

/// Thread-local reusable scan state shared by all matchers on the current thread.
///
/// # Safety
///
/// The `UnsafeCell` is safe to use here because:
///
/// 1. `#[thread_local]` guarantees that each thread has its own instance — no cross-thread
///    sharing occurs.
/// 2. The scan functions that access this static (`is_match_inner`, `process_simple`,
///    `process_preprocessed_into`) are not re-entrant: they obtain a `&mut` reference via
///    `SIMPLE_MATCH_STATE.get()` at the top of the call and hold it for the entire
///    duration. No callback or nested call re-enters the same path.
///
/// This pattern avoids the overhead of `RefCell` on the scan hot path.
#[thread_local]
pub(super) static SIMPLE_MATCH_STATE: UnsafeCell<SimpleMatchState> =
    UnsafeCell::new(SimpleMatchState::new());

/// Split-borrow view into [`SimpleMatchState`] for the scan hot path.
///
/// Created by [`SimpleMatchState::as_scan_state`] after [`prepare`](SimpleMatchState::prepare).
/// By storing mutable slices (not `Vec`s), the compiler can cache base pointers in
/// registers — eliminating repeated `Vec::get_unchecked_mut` pointer resolution that
/// otherwise accounts for 10–16% of runtime in profiled `process` workloads.
///
/// Disjoint field borrows (e.g. `self.word_states[i]` and `self.touched_indices.push()`)
/// are sound because the compiler can see they target different struct fields.
pub(super) struct ScanState<'a> {
    pub(super) word_states: &'a mut [WordState],
    pub(super) touched_indices: &'a mut Vec<usize>,
    pub(super) resolved_count: usize,
    pub(super) matrix: &'a mut [TinyVec<[i32; 16]>],
    pub(super) matrix_status: &'a mut [TinyVec<[u8; 16]>],
    pub(super) generation: u32,
}

/// Scan metadata passed through the hot match-processing path.
///
/// One `ScanContext` is constructed per text variant and threaded through
/// [`RuleSet::process_entry`](super::rule::RuleSet::process_entry) for every hit in
/// that variant. Kept `Copy` to avoid reference overhead in tight loops.
#[derive(Clone, Copy)]
pub(super) struct ScanContext {
    /// Index of the current transformed text variant.
    ///
    /// Used as the column index into [`SimpleMatchState::matrix`] for matrix-mode rules.
    pub(super) text_index: usize,
    /// Bitmask of compact process-type indices that contributed to this variant.
    ///
    /// Bit `i` is set if the variant was produced by (or is reachable from) the process
    /// type whose compact index is `i`. Checked against
    /// [`PatternEntry::pt_index`](super::pattern::PatternEntry::pt_index) to filter hits
    /// from irrelevant variants.
    pub(super) process_type_mask: u64,
    /// Total number of transformed variants participating in this scan.
    ///
    /// Determines the number of columns in the matrix for matrix-mode rules.
    pub(super) num_variants: usize,
    /// Whether the caller may stop on the first satisfied rule.
    ///
    /// `true` for `is_match` calls; `false` for `process` calls that must collect all
    /// matching rules.
    pub(super) exit_early: bool,
    /// Non-ASCII byte density of the current variant (0.0 = pure ASCII, 1.0 = all non-ASCII).
    ///
    /// Passed through to [`ScanPlan::for_each_match_value`](super::engine::ScanPlan::for_each_match_value)
    /// to select the bytewise or charwise automaton. Computed once at the root
    /// via SIMD, then propagated through the transform tree via the density
    /// estimate returned by [`TransformStep::apply`](crate::process::step::TransformStep::apply).
    pub(super) non_ascii_density: f32,
}

/// Hot-path methods on the split-borrow scan state.
impl ScanState<'_> {
    /// Returns the rules touched during the current generation.
    pub(super) fn touched_indices(&self) -> &[usize] {
        self.touched_indices
    }

    /// Returns whether `rule_idx` is satisfied in the current generation.
    #[inline(always)]
    pub(super) fn rule_is_satisfied(&self, rule_idx: usize) -> bool {
        debug_assert!(rule_idx < self.word_states.len());
        // SAFETY: `rule_idx` is in bounds — guarded by the debug_assert above.
        let word_state = unsafe { self.word_states.get_unchecked(rule_idx) };
        word_state.positive_generation == self.generation
            && word_state.not_generation != self.generation
    }

    /// Marks a simple rule as positive for the current generation.
    ///
    /// Returns `true` only when this is the first positive hit for the rule in the current
    /// generation. If the rule has not been touched at all, it is also added to
    /// `touched_indices`.
    #[inline(always)]
    pub(super) fn mark_positive(&mut self, rule_idx: usize) -> bool {
        let generation = self.generation;
        debug_assert!(rule_idx < self.word_states.len());
        // SAFETY: `rule_idx` is in bounds — guarded by the debug_assert above.
        let word_state = unsafe { self.word_states.get_unchecked_mut(rule_idx) };
        if word_state.positive_generation == generation {
            return false;
        }
        if word_state.matrix_generation != generation {
            word_state.matrix_generation = generation;
            self.touched_indices.push(rule_idx);
        }
        word_state.positive_generation = generation;
        true
    }

    /// Marks a simple rule as positive for the current generation (lightweight).
    ///
    /// Like [`mark_positive`](Self::mark_positive) but skips the `matrix_generation`
    /// check and `touched_indices` bookkeeping. Only safe to call from code paths
    /// that never read `touched_indices` afterward (i.e., `process_simple`).
    #[inline(always)]
    pub(super) fn mark_positive_simple(&mut self, rule_idx: usize) -> bool {
        let generation = self.generation;
        debug_assert!(rule_idx < self.word_states.len());
        // SAFETY: `rule_idx` is in bounds — guarded by the debug_assert above.
        let word_state = unsafe { self.word_states.get_unchecked_mut(rule_idx) };
        if word_state.positive_generation == generation {
            return false;
        }
        word_state.positive_generation = generation;
        true
    }
}

/// Test-only convenience methods. The hot path inlines these operations directly
/// to preserve `&mut WordState` references across disjoint field borrows.
#[cfg(test)]
impl ScanState<'_> {
    pub(super) fn generation(&self) -> u32 {
        self.generation
    }

    pub(super) fn init_rule(
        &mut self,
        rule: &super::rule::RuleHot,
        and_count: usize,
        rule_idx: usize,
        ctx: ScanContext,
    ) {
        let generation = self.generation;
        // SAFETY: `rule_idx` is in bounds — guarded by the debug_assert above.
        let word_state = unsafe { self.word_states.get_unchecked_mut(rule_idx) };
        word_state.matrix_generation = generation;
        word_state.positive_generation = if and_count == 0 { generation } else { 0 };
        word_state.remaining_and = and_count as u16;
        word_state.satisfied_mask = 0;
        self.touched_indices.push(rule_idx);

        // Derive use_matrix from segment_counts (same logic as build.rs).
        let use_matrix = and_count > super::encoding::BITMASK_CAPACITY
            || rule.segment_counts.len() > super::encoding::BITMASK_CAPACITY
            || rule.segment_counts[..and_count].iter().any(|&v| v != 1)
            || rule.segment_counts[and_count..].iter().any(|&v| v != 0);
        if use_matrix {
            init_matrix(
                // SAFETY: `rule_idx` is in bounds — matrix vecs are sized to match word_states.
                unsafe { self.matrix.get_unchecked_mut(rule_idx) },
                // SAFETY: `rule_idx` is in bounds — matrix_status is sized identically.
                unsafe { self.matrix_status.get_unchecked_mut(rule_idx) },
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
    /// All backing vectors start empty and grow on the first call to [`prepare`](Self::prepare).
    pub(super) const fn new() -> Self {
        Self {
            word_states: Vec::new(),
            matrix: Vec::new(),
            matrix_status: Vec::new(),
            touched_indices: Vec::new(),
            resolved_count: 0,
            generation: 0,
        }
    }

    /// Advances the generation and grows backing storage for at least `size` rules.
    ///
    /// Must be called exactly once at the start of every scan before any state is read.
    /// On `u32::MAX` overflow, all generation stamps are bulk-reset to 0 and the
    /// counter restarts at 1.
    pub(super) fn prepare(&mut self, size: usize) {
        if self.generation == u32::MAX {
            for state in self.word_states.iter_mut() {
                state.matrix_generation = 0;
                state.positive_generation = 0;
                state.not_generation = 0;
            }
            self.generation = 1;
        } else {
            self.generation += 1;
        }

        if self.word_states.len() < size {
            self.word_states.resize(size, WordState::default());
            self.matrix.resize(size, TinyVec::new());
            self.matrix_status.resize(size, TinyVec::new());
        }

        self.touched_indices.clear();
        self.resolved_count = 0;
    }

    /// Creates a [`ScanState`] split-borrow view for the scan hot path.
    ///
    /// Must be called after [`prepare`](Self::prepare). The returned `ScanState` borrows
    /// individual fields as mutable slices, allowing the compiler to cache base pointers
    /// in registers instead of re-resolving Vec metadata on every access.
    #[inline(always)]
    pub(super) fn as_scan_state(&mut self) -> ScanState<'_> {
        ScanState {
            word_states: &mut self.word_states,
            touched_indices: &mut self.touched_indices,
            resolved_count: self.resolved_count,
            matrix: &mut self.matrix,
            matrix_status: &mut self.matrix_status,
            generation: self.generation,
        }
    }
}

/// Initializes the per-variant counter matrix for a complex rule.
///
/// Allocates (or re-sizes) `flat_matrix` to `num_segments × num_variants` cells and
/// fills each row with the segment's required count from `segment_counts`. Resets
/// `flat_status` to all-zero (no segment satisfied yet).
///
/// Marked `#[cold]` because matrix-mode rules are rare — most rules use the bitmask
/// fast path.
#[cold]
#[inline(never)]
pub(super) fn init_matrix(
    flat_matrix: &mut TinyVec<[i32; 16]>,
    flat_status: &mut TinyVec<[u8; 16]>,
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
    use super::*;

    fn make_ctx(num_variants: usize, exit_early: bool) -> ScanContext {
        ScanContext {
            text_index: 0,
            process_type_mask: u64::MAX,
            num_variants,
            exit_early,
            non_ascii_density: 0.0,
        }
    }

    #[test]
    fn test_prepare_grows_storage() {
        let mut state = SimpleMatchState::new();
        assert_eq!(state.generation, 0);
        state.prepare(10);
        assert!(state.word_states.len() >= 10);
        assert!(state.matrix.len() >= 10);
        assert!(state.matrix_status.len() >= 10);
        assert_eq!(state.generation, 1);
        assert!(state.touched_indices.is_empty());
    }

    #[test]
    fn test_prepare_generation_increments() {
        let mut state = SimpleMatchState::new();
        state.prepare(1);
        assert_eq!(state.generation, 1);
        state.prepare(1);
        assert_eq!(state.generation, 2);
        state.prepare(1);
        assert_eq!(state.generation, 3);
    }

    #[test]
    fn test_prepare_generation_wraparound() {
        let mut state = SimpleMatchState::new();
        state.prepare(3);
        let current = state.generation;
        state.word_states[0].positive_generation = current;
        state.word_states[1].matrix_generation = current;
        state.word_states[2].not_generation = current;

        state.generation = u32::MAX - 1;
        state.prepare(3);
        assert_eq!(state.generation, u32::MAX);

        state.prepare(3);
        assert_eq!(state.generation, 1);
        for ws in &state.word_states {
            assert_eq!(ws.matrix_generation, 0);
            assert_eq!(ws.positive_generation, 0);
            assert_eq!(ws.not_generation, 0);
        }
    }

    #[test]
    fn test_mark_positive_dedup() {
        let mut state = SimpleMatchState::new();
        state.prepare(2);
        let mut ss = state.as_scan_state();

        assert!(ss.mark_positive(0), "first mark should return true");
        assert!(!ss.mark_positive(0), "second mark should return false");
        assert_eq!(ss.touched_indices(), &[0]);
    }

    #[test]
    fn test_rule_is_satisfied() {
        let mut state = SimpleMatchState::new();
        state.prepare(1);
        let current = state.generation;

        state.word_states[0].positive_generation = current;
        let ss = state.as_scan_state();
        assert!(ss.rule_is_satisfied(0));
    }

    #[test]
    fn test_rule_is_satisfied_not_veto() {
        let mut state = SimpleMatchState::new();
        state.prepare(1);
        let current = state.generation;

        state.word_states[0].positive_generation = current;
        state.word_states[0].not_generation = current;
        let ss = state.as_scan_state();
        assert!(!ss.rule_is_satisfied(0));
    }

    #[test]
    fn test_init_rule_matrix() {
        let mut state = SimpleMatchState::new();
        state.prepare(1);

        let rule = super::super::rule::RuleHot {
            segment_counts: vec![2, 1, 0],
        };
        let ctx = make_ctx(2, false);
        let mut ss = state.as_scan_state();
        ss.init_rule(&rule, 2, 0, ctx);

        assert_eq!(ss.word_states[0].matrix_generation, ss.generation());
        assert_eq!(ss.word_states[0].remaining_and, 2);
        assert_eq!(ss.word_states[0].satisfied_mask, 0);
        assert_eq!(ss.touched_indices(), &[0]);

        assert_eq!(ss.matrix[0].len(), 6);
        assert_eq!(&ss.matrix[0][..], &[2, 2, 1, 1, 0, 0]);
        assert_eq!(ss.matrix_status[0].len(), 3);
        assert!(ss.matrix_status[0].iter().all(|&s| s == 0));
    }
}
