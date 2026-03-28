//! Internal types and constants for [`super::SimpleMatcher`].
//!
//! All items here are `pub(super)`, visible within the `simple_matcher` module only.
//! Public-facing types ([`SimpleTable`], [`SimpleTableSerde`]) are the exception.

use std::borrow::Cow;
use std::cell::UnsafeCell;
use std::collections::HashMap;

#[cfg(feature = "dfa")]
use aho_corasick::AhoCorasick;
use daachorse::{DoubleArrayAhoCorasick, charwise::CharwiseDoubleArrayAhoCorasick};
use tinyvec::TinyVec;

use crate::process::ProcessType;

/// Mapping from [`ProcessType`] to a `{word_id → pattern}` dictionary.
///
/// The primary input to [`SimpleMatcher::new`](super::SimpleMatcher::new). Each outer key selects the
/// normalization pipeline applied before the patterns in the inner map are matched.
///
/// # Examples
///
/// ```rust
/// use std::collections::HashMap;
/// use matcher_rs::{SimpleTable, ProcessType};
///
/// let mut table: SimpleTable = HashMap::new();
/// table.entry(ProcessType::None).or_default().insert(1, "hello");
/// table.entry(ProcessType::Fanjian).or_default().insert(2, "漢字");
/// ```
pub type SimpleTable<'a> = HashMap<ProcessType, HashMap<u32, &'a str>>;

/// Owned/borrowed variant of [`SimpleTable`] suitable for serialization.
///
/// Identical in structure to [`SimpleTable`], but uses `Cow<'a, str>` instead of
/// `&'a str` so that both owned and borrowed patterns can be stored. Useful when
/// loading rules from a deserialized source (e.g. JSON) where the strings are
/// owned `String` values.
pub type SimpleTableSerde<'a> = HashMap<ProcessType, HashMap<u32, Cow<'a, str>>>;

/// Threshold for selecting the bitmask fast-path over the matrix fallback.
///
/// Rules with ≤ 64 AND/NOT segments use a `u64` bitmask to track satisfaction;
/// rules with more segments use the 2-D counter matrix in [`SimpleMatchState`].
pub(super) const BITMASK_CAPACITY: usize = 64;

/// Number of slots in the sequential `ProcessType` index table.
///
/// [`crate::ProcessType`] has 6 single-bit flags at bit positions 0–5. The bitflag
/// `.bits()` value of a composite type can be up to 0b00111111 = 63.
/// The table must be large enough to index any composite `.bits()` value directly.
pub(super) const PROCESS_TYPE_TABLE_SIZE: usize = 64;

/// Maximum number of ASCII patterns to route through AC DFA before switching
/// to the DAAC ASCII matcher when the `dfa` feature is enabled.
///
/// The threshold is benchmark-driven and optimized for search throughput.
#[cfg(feature = "dfa")]
pub(super) const AC_DFA_PATTERN_THRESHOLD: usize = 2_000;

/// Classifies a [`PatternEntry`] by its role in rule evaluation.
///
/// Determined once at construction time so that `process_match` can branch
/// on a single `match` instead of re-deriving the category from `offset`
/// and `RuleHot` fields on every automaton hit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub(super) enum PatternKind {
    /// Simple literal rule (and_count==1, no NOT, no matrix).
    /// Skips all counter/bitmask logic — just mark the rule as satisfied.
    Simple = 0,
    /// AND sub-pattern: offset < and_count. Decrements a counter or sets
    /// a bitmask bit; rule is satisfied when all AND segments fire.
    And = 1,
    /// NOT sub-pattern: offset >= and_count. Any hit permanently
    /// disqualifies the owning rule for this generation.
    Not = 2,
}

/// ASCII automaton engine for ASCII-only patterns.
///
/// When the `dfa` feature is enabled and the ASCII pattern count is below
/// [`AC_DFA_PATTERN_THRESHOLD`], the `AcDfa` variant is used. Otherwise
/// `DaacBytewise` is used. Without `dfa`, only `DaacBytewise` is built.
///
/// For `DaacBytewise`, the automaton value directly encodes the global dedup
/// index, eliminating one indirection hop per automaton hit. For `AcDfa`,
/// `aho_corasick` assigns sequential pattern IDs that differ from dedup
/// indices, so a compact `to_dedup` map is kept inside the enum variant.
#[derive(Clone)]
pub(super) enum AsciiMatcher {
    #[cfg(feature = "dfa")]
    AcDfa {
        matcher: AhoCorasick,
        /// Maps sequential AC pattern ID → global dedup index.
        to_dedup: Vec<u32>,
    },
    /// DAAC value IS the global dedup index — no extra indirection.
    DaacBytewise(DoubleArrayAhoCorasick<u32>),
}

/// Non-ASCII automaton engine for patterns containing multi-byte characters.
///
/// Uses `daachorse` charwise double-array Aho-Corasick, which does one state
/// transition per Unicode character rather than per UTF-8 byte. This is faster
/// for CJK-heavy text where characters are 3 bytes each.
#[derive(Clone)]
pub(super) enum NonAsciiMatcher {
    /// Charwise DAAC — the automaton value IS the global dedup index.
    DaacCharwise(CharwiseDoubleArrayAhoCorasick<u32>),
}

/// Per-rule match state for a single search, keyed by generation ID.
///
/// Stored in a flat `Vec` inside [`SimpleMatchState`], one entry per rule.
/// Generation IDs implement a sparse-set pattern: comparing a field against the current
/// `SimpleMatchState::generation` determines whether the field was written during this
/// search without requiring a full zero-fill between calls.
#[derive(Default, Clone, Copy)]
pub(super) struct WordState {
    /// Set to the current generation when this rule is first touched.
    pub(super) matrix_generation: u32,
    /// Set to the current generation when all required AND segments are satisfied,
    /// even if a NOT segment may still veto the rule later in the scan.
    pub(super) positive_generation: u32,
    /// Set to the current generation when a NOT sub-pattern fires, permanently
    /// disqualifying this rule for the remainder of the search.
    pub(super) not_generation: u32,
    /// Bitmask of AND sub-patterns (up to 64) satisfied so far this generation.
    pub(super) satisfied_mask: u64,
    /// Number of AND segments still unsatisfied for the matrix path.
    pub(super) remaining_and: u16,
}

/// Reusable per-thread scratch space for a single [`super::SimpleMatcher`] search.
///
/// Allocated once and stored in a `thread_local!`; reused across calls via the generation
/// trick in [`WordState`] to avoid clearing the full state between searches.
pub(super) struct SimpleMatchState {
    /// Flat array indexed by `rule_idx`; one [`WordState`] per rule.
    pub(super) word_states: Vec<WordState>,
    /// Fallback counter storage for rules with >64 AND-splits or repeated sub-patterns;
    /// a flattened `(num_splits × num_text_variants)` counter matrix per rule.
    pub(super) matrix: Vec<TinyVec<[i32; 16]>>,
    /// Per-segment status bits for the matrix path. A non-zero entry means the segment
    /// already crossed its terminal threshold during this generation.
    pub(super) matrix_status: Vec<TinyVec<[u8; 16]>>,
    /// Indices of rules written during the current generation; iterated in Pass 2 to avoid
    /// scanning the entire `word_states` array.
    pub(super) touched_indices: Vec<usize>,
    /// Monotonically incrementing ID; wrapping triggers a full reset of the generation
    /// markers before the next search starts at `1`.
    pub(super) generation: u32,
}

impl SimpleMatchState {
    /// Creates an empty `SimpleMatchState` ready for its first search.
    pub(super) const fn new() -> Self {
        Self {
            word_states: Vec::new(),
            matrix: Vec::new(),
            matrix_status: Vec::new(),
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
}

/// Thread-local cache for `SimpleMatchState` to avoid repeated allocations.
///
/// Uses `#[thread_local]` to eliminate the `thread_local!` macro's `.with()` closure
/// overhead. Access is a direct TLS segment-register read on x86/aarch64.
#[thread_local]
pub(super) static SIMPLE_MATCH_STATE: UnsafeCell<SimpleMatchState> =
    UnsafeCell::new(SimpleMatchState::new());

/// Context for a single text variant scan, bundling parameters shared between
/// `scan_variant` and `process_match`.
#[derive(Clone, Copy)]
pub(super) struct ScanContext {
    /// Index of this variant within the [`crate::ProcessedTextMasks`] collection.
    pub(super) text_index: usize,
    /// Bitmask of sequential [`crate::ProcessType`] indices for this variant.
    pub(super) process_type_mask: u64,
    /// Total number of text variants being scanned this call.
    pub(super) num_variants: usize,
    /// If `true`, halt scanning as soon as any rule is fully satisfied.
    pub(super) exit_early: bool,
    /// Whether the scanned text is entirely ASCII; selects the ASCII automaton path.
    pub(super) is_ascii: bool,
}

/// Hot match-evaluation fields for a single rule, accessed during Pass 1.
///
/// Kept separate from [`RuleCold`] so that the hot data fits in fewer cache lines
/// when scanning large rule sets.
#[derive(Debug, Clone)]
pub(super) struct RuleHot {
    /// Per-sub-pattern counters. Indices `0..and_count` are AND segments (initial value
    /// equal to the required occurrence count, decremented toward ≤0 to signal satisfaction);
    /// indices `and_count..` are NOT segments (initial value 0 minus the required absence
    /// count, incremented toward >0 to signal disqualification).
    pub(super) segment_counts: Vec<i32>,
    /// Boundary in `segment_counts` separating AND segment indices from NOT segment indices.
    pub(super) and_count: usize,
    /// `true` when the rule requires the full counter matrix instead of the simple
    /// bitmask path.
    pub(super) use_matrix: bool,
    /// `true` when the rule contains one or more NOT segments.
    pub(super) has_not: bool,
}

/// Cold result-construction fields for a single rule, accessed only in Pass 2.
#[derive(Debug, Clone)]
pub(super) struct RuleCold {
    /// Caller-assigned identifier returned in [`super::SimpleResult`].
    pub(super) word_id: u32,
    /// The original pattern string, stored for inclusion in match results.
    pub(super) word: String,
}

/// Links a deduplicated emitted pattern back to the rule and sub-pattern it belongs to.
///
/// Stored in the flat `ac_dedup_entries` array; a `(start, len)` range in
/// `ac_dedup_ranges` maps each automaton pattern index to its slice of entries.
/// At 8 bytes this struct fits eight entries per cache line (vs. four entries for the
/// former `process_type_mask: u64` field layout).
#[derive(Debug, Clone)]
pub(super) struct PatternEntry {
    /// Index into `rule_hot`/`rule_cold` identifying the owning rule.
    pub(super) rule_idx: u32,
    /// Index into `segment_counts` of the owning rule; identifies which AND or NOT
    /// sub-pattern was matched.
    pub(super) offset: u16,
    /// Sequential process-type table index built during [`super::SimpleMatcher::new`];
    /// used as `1u64 << pt_index` to filter hits from the wrong pipeline.
    pub(super) pt_index: u8,
    /// Classifies this entry for dispatch in `process_match`.
    /// The struct is 8 bytes regardless (padding was here before).
    pub(super) kind: PatternKind,
}
