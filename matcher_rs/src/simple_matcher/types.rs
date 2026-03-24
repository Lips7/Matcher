use std::cell::RefCell;

use tinyvec::TinyVec;

/// Threshold for selecting the bitmask fast-path over the matrix fallback.
///
/// Rules with ≤ 64 AND/NOT segments use a `u64` bitmask to track satisfaction;
/// rules with more segments use the 2-D counter matrix in [`SimpleMatchState`].
pub(super) const BITMASK_CAPACITY: usize = 64;

/// Number of slots in the sequential `ProcessType` index table.
///
/// [`ProcessType`](crate::ProcessType) bit flags occupy positions 0–7 (8 single-bit flags), but
/// the bitflag `.bits()` value of a composite type can be up to 63 (all 6 flags set = 0b111111 = 63).
/// The table must be large enough to index any composite `.bits()` value directly.
pub(super) const PROCESS_TYPE_TABLE_SIZE: usize = 64;

/// Maximum number of ASCII patterns to route through AC DFA before switching
/// to DAAC bytewise. Below this count AC DFA leads on search throughput
/// (especially against non-ASCII text); above it DAAC bytewise wins while
/// using ~16x less memory.
///
/// Derived from `bench_engine.rs`: AC DFA leads at n≤1000, DAAC bytewise
/// leads at n≥10000 on ASCII text. 5000 is a conservative midpoint.
pub(super) const AC_DFA_PATTERN_THRESHOLD: usize = 5_000;

/// Bytewise automaton engine for ASCII-only patterns.
///
/// When the `dfa` feature is enabled and the ASCII pattern count is below
/// [`AC_DFA_PATTERN_THRESHOLD`], the `AcDfa` variant is used (faster at
/// small counts). Otherwise `DaacBytewise` is used (faster at large counts
/// and uses ~16x less memory).
///
/// When the `dfa` feature is disabled, only `DaacBytewise` is available —
/// it outperforms `AhoCorasick` (ContiguousNFA) at every pattern count.
///
/// For `DaacBytewise`, the automaton value directly encodes the global dedup
/// index, eliminating one indirection hop per automaton hit. For `AcDfa`,
/// `aho_corasick` assigns sequential pattern IDs that differ from dedup
/// indices, so a compact `to_dedup` map is kept inside the enum variant.
#[derive(Clone)]
pub(super) enum BytewiseMatcher {
    #[cfg(feature = "dfa")]
    AcDfa {
        matcher: aho_corasick::AhoCorasick,
        /// Maps sequential AC pattern ID → global dedup index.
        to_dedup: Vec<u32>,
    },
    /// DAAC value IS the global dedup index — no extra indirection.
    DaacBytewise(daachorse::DoubleArrayAhoCorasick<u32>),
}

/// Per-rule match state for a single search, keyed by generation ID.
///
/// Stored in a flat `Vec` inside [`SimpleMatchState`], one entry per rule.
/// Generation IDs implement a sparse-set pattern: comparing a field against the current
/// `SimpleMatchState::generation` determines whether the field was written during this
/// search without requiring a full zero-fill between calls.
///
/// * `matrix_generation` — set to the current generation when this rule is first touched.
/// * `not_generation` — set to the current generation when a NOT sub-pattern fires,
///   permanently disqualifying this rule for the remainder of the search.
/// * `satisfied_mask` — bitmask of AND sub-patterns (up to 64) satisfied so far.
/// * `satisfied_generation` — set to the current generation when the rule is fully
///   satisfied (bitmask fast-path only, rules without NOT segments). Enables a
///   single-comparison skip in `process_match` instead of a 4-condition check.
#[derive(Default, Clone, Copy)]
pub(super) struct WordState {
    pub(super) matrix_generation: u32,
    pub(super) not_generation: u32,
    pub(super) satisfied_generation: u32,
    pub(super) satisfied_mask: u64,
}

/// Reusable per-thread scratch space for a single [`super::SimpleMatcher`] scan.
///
/// Allocated once and stored in a `thread_local!`; reused across calls via the generation
/// trick in [`WordState`] to avoid clearing the full state between searches.
///
/// * `word_states` — flat array indexed by `rule_idx`; one [`WordState`] per rule.
/// * `matrix` — fallback storage for rules with >64 AND-splits or repeated sub-patterns;
///   a flattened `(num_splits × num_text_variants)` counter matrix per rule.
/// * `touched_indices` — indices of rules written during the current generation; iterated
///   in Pass 2 to avoid scanning the entire `word_states` array.
/// * `generation` — monotonically incrementing ID; wrapping to `u32::MAX` triggers a
///   full reset of all generation fields.
pub(super) struct SimpleMatchState {
    pub(super) word_states: Vec<WordState>,
    pub(super) matrix: Vec<TinyVec<[i32; 16]>>,
    pub(super) touched_indices: Vec<usize>,
    pub(super) generation: u32,
}

impl SimpleMatchState {
    /// Creates an empty `SimpleMatchState` ready for its first search.
    pub(super) fn new() -> Self {
        Self {
            word_states: Vec::new(),
            matrix: Vec::new(),
            touched_indices: Vec::new(),
            generation: 0,
        }
    }

    /// Advances the generation counter and grows buffers to hold `size` rules.
    ///
    /// Must be called at the start of every search. Overflow of the `u32` counter
    /// triggers a full reset of all generation fields before incrementing to `1`.
    pub(super) fn prepare(&mut self, size: usize) {
        if self.generation == u32::MAX {
            for state in self.word_states.iter_mut() {
                state.matrix_generation = 0;
                state.not_generation = 0;
                state.satisfied_generation = 0;
            }
            self.generation = 1;
        } else {
            self.generation += 1;
        }

        if self.word_states.len() < size {
            self.word_states.resize(size, WordState::default());
            self.matrix.resize(size, TinyVec::new());
        }

        self.touched_indices.clear();
    }
}

thread_local! {
    /// Thread-local cache for `SimpleMatchState` to avoid repeated allocations.
    pub(super) static SIMPLE_MATCH_STATE: RefCell<SimpleMatchState> = RefCell::new(SimpleMatchState::new());
}

/// Context for a single text variant scan, bundling parameters shared between
/// `scan_variant` and `process_match`.
#[derive(Clone, Copy)]
pub(super) struct ScanContext {
    pub(super) text_index: usize,
    pub(super) process_type_mask: u64,
    pub(super) num_variants: usize,
    pub(super) exit_early: bool,
    pub(super) is_ascii: bool,
}

/// Hot match-evaluation fields for a single pattern rule, accessed during Pass 1.
///
/// Kept separate from [`RuleCold`] so that the hot data fits in fewer cache lines
/// when scanning large rule sets.
///
/// * `segment_counts` — per-sub-pattern counters. Indices `0..and_count` are AND segments
///   (initial value +1, decremented toward ≤0 to signal satisfaction); indices
///   `and_count..` are NOT segments (initial value 0, incremented toward >0 to signal
///   disqualification).
/// * `and_count` — boundary in `segment_counts` separating AND from NOT segments.
/// * `expected_mask` — bitmask of AND segments that must all reach ≤0. Non-zero only
///   when `and_count ≤ 64` and all AND segments appear exactly once (the common, fast case).
/// * `use_matrix` — `true` when the rule requires the full counter matrix (>64 segments,
///   repeated sub-patterns across `&`-splits, or a non-trivial NOT pattern).
/// * `num_splits` — `segment_counts.len()` cached to avoid pointer chasing.
#[derive(Debug, Clone)]
pub(super) struct RuleHot {
    pub(super) segment_counts: Vec<i32>,
    pub(super) and_count: usize,
    pub(super) expected_mask: u64,
    pub(super) use_matrix: bool,
    pub(super) num_splits: u16,
}

/// Cold result-construction fields for a single pattern rule, accessed only in Pass 2.
///
/// * `word_id` — caller-assigned identifier returned in [`super::SimpleResult`].
/// * `word` — the original pattern string (stored for inclusion in results).
#[derive(Debug, Clone)]
pub(super) struct RuleCold {
    pub(super) word_id: u32,
    pub(super) word: String,
}

/// Links a deduplicated automaton pattern back to the rule and sub-pattern it belongs to.
///
/// Stored in the flat `ac_dedup_entries` array; a `(start, len)` range in
/// `ac_dedup_ranges` maps each automaton pattern index to its slice of entries.
///
/// * `rule_idx` — index into `rule_hot`/`rule_cold` identifying the owning rule.
/// * `offset` — index into `segment_counts` of the owning rule; identifies which
///   AND or NOT sub-pattern was matched.
/// * `pt_index` — sequential index into the compact process-type table built during
///   [`super::SimpleMatcher::new`]. Used as `1u64 << pt_index` to compare against the
///   text variant's `process_type_mask` and discard hits from the wrong pipeline.
///   Replaces the former `process_type_mask: u64` field, shrinking the struct from
///   16 bytes to 8 bytes and doubling entries per cache line.
#[derive(Debug, Clone)]
pub(super) struct PatternEntry {
    pub(super) rule_idx: u32,
    pub(super) offset: u16,
    pub(super) pt_index: u8,
}
