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

use std::cell::UnsafeCell;

use tinyvec::TinyVec;

use super::rule::RuleHot;

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
    /// [`PatternKind::Simple`](super::rule::PatternKind::Simple) hit. A rule is
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
    /// when [`RuleHot::use_matrix`](super::rule::RuleHot::use_matrix) is `false`
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
/// For rules with [`RuleHot::use_matrix`](super::rule::RuleHot::use_matrix) = `true`,
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
    /// [`PatternEntry::pt_index`](super::rule::PatternEntry::pt_index) to filter hits
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
    /// Whether the current variant is pure ASCII.
    ///
    /// Passed through to [`ScanPlan::for_each_match_value`](super::engine::ScanPlan::for_each_match_value)
    /// to select the bytewise or charwise automaton.
    pub(super) is_ascii: bool,
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
    }

    /// Returns the current scan generation id.
    #[inline(always)]
    pub(super) fn generation(&self) -> u32 {
        self.generation
    }

    /// Returns the rules touched during the current generation.
    #[inline(always)]
    pub(super) fn touched_indices(&self) -> &[usize] {
        &self.touched_indices
    }

    /// Returns whether `rule_idx` is satisfied in the current generation.
    ///
    /// A rule is satisfied when all its AND segments have been observed
    /// (`positive_generation == generation`) and no NOT segment has vetoed it
    /// (`not_generation != generation`).
    ///
    /// # Safety
    ///
    /// Uses `get_unchecked` on `self.word_states`. Guarded by a preceding `debug_assert!`.
    ///
    /// # Panics
    ///
    /// Debug-asserts that `rule_idx < self.word_states.len()`.
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
    /// generation — used by the all-simple fast path to deduplicate results.
    ///
    /// If the rule has not been touched at all in this generation, it is also added to
    /// `touched_indices`.
    ///
    /// # Safety
    ///
    /// Uses `get_unchecked_mut` on `self.word_states`. Guarded by a preceding `debug_assert!`.
    ///
    /// # Panics
    ///
    /// Debug-asserts that `rule_idx < self.word_states.len()`.
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

    /// Initializes one complex rule the first time it is touched in a generation.
    ///
    /// Sets up the generation stamp, resets the remaining-AND counter and bitmask, adds
    /// the rule to `touched_indices`, and — for matrix-mode rules — initializes the
    /// per-variant counter matrix via [`init_matrix`].
    ///
    /// If the rule has zero AND segments (pure NOT rule), it is immediately marked
    /// positive since there is nothing to satisfy.
    ///
    /// # Safety
    ///
    /// Uses `get_unchecked_mut` on `self.word_states`, `self.matrix`, and
    /// `self.matrix_status`. All accesses are for `rule_idx` which was bounds-checked
    /// by the caller.
    #[inline(always)]
    pub(super) fn init_rule(&mut self, rule: &RuleHot, rule_idx: usize, ctx: ScanContext) {
        let generation = self.generation;
        // SAFETY: `rule_idx` is in bounds — caller guarantees it via `RuleSet` indexing.
        let word_state = unsafe { self.word_states.get_unchecked_mut(rule_idx) };
        word_state.matrix_generation = generation;
        word_state.positive_generation = if rule.and_count == 0 { generation } else { 0 };
        word_state.remaining_and = rule.and_count as u16;
        word_state.satisfied_mask = 0;
        self.touched_indices.push(rule_idx);

        if rule.use_matrix {
            init_matrix(
                // SAFETY: `rule_idx` is in bounds — `matrix` is resized to match
                // `word_states` in `prepare`.
                unsafe { self.matrix.get_unchecked_mut(rule_idx) },
                // SAFETY: `matrix_status` is resized identically to `matrix` in `prepare`.
                unsafe { self.matrix_status.get_unchecked_mut(rule_idx) },
                &rule.segment_counts,
                ctx.num_variants,
            );
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
fn init_matrix(
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
            is_ascii: true,
        }
    }

    #[test]
    fn test_prepare_grows_storage() {
        let mut state = SimpleMatchState::new();
        assert_eq!(state.generation(), 0);
        state.prepare(10);
        assert!(state.word_states.len() >= 10);
        assert!(state.matrix.len() >= 10);
        assert!(state.matrix_status.len() >= 10);
        assert_eq!(state.generation(), 1);
        assert!(state.touched_indices().is_empty());
    }

    #[test]
    fn test_prepare_generation_increments() {
        let mut state = SimpleMatchState::new();
        state.prepare(1);
        assert_eq!(state.generation(), 1);
        state.prepare(1);
        assert_eq!(state.generation(), 2);
        state.prepare(1);
        assert_eq!(state.generation(), 3);
    }

    #[test]
    fn test_prepare_generation_wraparound() {
        let mut state = SimpleMatchState::new();
        state.prepare(3);
        let current = state.generation();
        state.word_states[0].positive_generation = current;
        state.word_states[1].matrix_generation = current;
        state.word_states[2].not_generation = current;

        // Force generation to u32::MAX - 1 so next prepare hits MAX
        state.generation = u32::MAX - 1;
        state.prepare(3);
        assert_eq!(state.generation(), u32::MAX);

        // Next prepare should wraparound: reset all stamps to 0, generation = 1
        state.prepare(3);
        assert_eq!(state.generation(), 1);
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

        assert!(state.mark_positive(0), "first mark should return true");
        assert!(!state.mark_positive(0), "second mark should return false");
        assert_eq!(state.touched_indices(), &[0]);
    }

    #[test]
    fn test_rule_is_satisfied() {
        let mut state = SimpleMatchState::new();
        state.prepare(1);
        let current = state.generation();

        assert!(!state.rule_is_satisfied(0));

        state.word_states[0].positive_generation = current;
        assert!(state.rule_is_satisfied(0));

        // NOT veto overrides positive
        state.word_states[0].not_generation = current;
        assert!(!state.rule_is_satisfied(0));
    }

    #[test]
    fn test_init_rule_matrix() {
        let mut state = SimpleMatchState::new();
        state.prepare(1);

        let rule = RuleHot {
            segment_counts: vec![1, 1, 0],
            and_count: 2,
            use_matrix: true,
        };
        let ctx = make_ctx(2, false);
        state.init_rule(&rule, 0, ctx);

        assert_eq!(state.word_states[0].matrix_generation, state.generation());
        assert_eq!(state.word_states[0].remaining_and, 2);
        assert_eq!(state.word_states[0].satisfied_mask, 0);
        assert_eq!(state.touched_indices(), &[0]);

        // Matrix should be 3 segments × 2 variants = 6 cells
        assert_eq!(state.matrix[0].len(), 6);
        // Row 0: [1, 1], Row 1: [1, 1], Row 2 (NOT): [0, 0]
        assert_eq!(&state.matrix[0][..], &[1, 1, 1, 1, 0, 0]);
        assert_eq!(state.matrix_status[0].len(), 3);
        assert!(state.matrix_status[0].iter().all(|&s| s == 0));
    }
}
