use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use aho_corasick::{AhoCorasick, AhoCorasickBuilder, AhoCorasickKind};
use daachorse::{
    CharwiseDoubleArrayAhoCorasick, CharwiseDoubleArrayAhoCorasickBuilder,
    MatchKind as DoubleArrayAhoCorasickMatchKind,
};
use serde::Serialize;
use tinyvec::TinyVec;

use crate::process::process_matcher::{
    ProcessType, ProcessTypeBitNode, ProcessedTextMasks, build_process_type_tree,
    reduce_text_process_emit, return_processed_string_to_pool, walk_process_tree,
};

/// Threshold for selecting the bitmask fast-path over the matrix fallback.
///
/// Rules with ≤ 64 AND/NOT segments use a `u64` bitmask to track satisfaction;
/// rules with more segments use the 2-D counter matrix in [`SimpleMatchState`].
const BITMASK_CAPACITY: usize = 64;

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
struct WordState {
    matrix_generation: u32,
    not_generation: u32,
    satisfied_generation: u32,
    satisfied_mask: u64,
}

/// Reusable per-thread scratch space for a single [`SimpleMatcher`] scan.
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
struct SimpleMatchState {
    word_states: Vec<WordState>,
    matrix: Vec<TinyVec<[i32; 16]>>,
    touched_indices: Vec<usize>,
    generation: u32,
}

impl SimpleMatchState {
    /// Creates an empty `SimpleMatchState` ready for its first search.
    fn new() -> Self {
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
    fn prepare(&mut self, size: usize) {
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
    static SIMPLE_MATCH_STATE: RefCell<SimpleMatchState> = RefCell::new(SimpleMatchState::new());
}

/// Mapping from [`ProcessType`] to a `{word_id → pattern}` dictionary.
///
/// The primary input to [`SimpleMatcher::new`]. Each outer key selects the
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

/// A single match returned by [`SimpleMatcher::process`].
///
/// # Fields
/// * `word_id` — the caller-assigned identifier from the input [`SimpleTable`].
/// * `word` — the original pattern string, borrowed from the compiled rule.
///
/// # Examples
///
/// ```rust
/// use matcher_rs::{SimpleMatcherBuilder, ProcessType};
///
/// let matcher = SimpleMatcherBuilder::new()
///     .add_word(ProcessType::None, 42, "hello")
///     .build();
///
/// let results = matcher.process("say hello");
/// assert_eq!(results[0].word_id, 42);
/// assert_eq!(results[0].word, "hello");
/// ```
#[derive(Serialize, Debug)]
pub struct SimpleResult<'a> {
    pub word_id: u32,
    pub word: Cow<'a, str>,
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
struct RuleHot {
    segment_counts: Vec<i32>,
    and_count: usize,
    expected_mask: u64,
    use_matrix: bool,
    num_splits: u16,
}

/// Cold result-construction fields for a single pattern rule, accessed only in Pass 2.
///
/// * `word_id` — caller-assigned identifier returned in [`SimpleResult`].
/// * `word` — the original pattern string (stored for inclusion in results).
#[derive(Debug, Clone)]
struct RuleCold {
    word_id: u32,
    word: String,
}

/// Links a deduplicated automaton pattern back to the rule and sub-pattern it belongs to.
///
/// Stored in the flat `ac_dedup_entries` array; a `(start, len)` range in
/// `ac_dedup_ranges` maps each automaton pattern index to its slice of entries.
///
/// * `process_type_mask` — bitmask of [`ProcessType`] bits that produced this pattern;
///   used to discard hits from text variants that don't match the rule's pipeline.
/// * `rule_idx` — index into `rule_hot`/`rule_cold` identifying the owning rule.
/// * `offset` — index into `segment_counts` of the owning rule; identifies which
///   AND or NOT sub-pattern was matched.
#[derive(Debug, Clone)]
struct PatternEntry {
    process_type_mask: u64,
    rule_idx: u32,
    offset: u16,
}

/// The underlying scan engine used by [`SimpleMatcher`].
#[derive(Clone)]
enum InternalMatcher {
    /// Standard Aho-Corasick (DFA or ContiguousNFA depending on the `dfa` feature flag).
    AhoCorasick(AhoCorasick),
    /// Double-array Aho-Corasick (DAAC) matcher, optimized for CJK text.
    DoubleArrayAhoCorasick(CharwiseDoubleArrayAhoCorasick<u32>),
}

/// Multi-pattern matcher with logical operators and text normalization.
///
/// Prefer constructing via [`crate::SimpleMatcherBuilder`] rather than calling [`new`](Self::new) directly.
///
/// ## Pattern Syntax
///
/// Each pattern string may contain two special operators:
///
/// | Operator | Meaning |
/// |----------|---------|
/// | `&` | All adjacent sub-patterns must appear (order-independent AND) |
/// | `~` | The following sub-pattern must be **absent** (NOT) |
///
/// ```text
/// "apple&pie"      -- fires only when both "apple" and "pie" appear
/// "banana~peel"    -- fires when "banana" appears but "peel" does not
/// "a&b~c"          -- fires when both "a" and "b" appear and "c" does not
/// "a&a~b~b"        -- fires when "a" appears twice and "b" appears fewer than twice
/// ```
///
/// ## Two-Pass Matching
///
/// **Pass 1 — Scan**: The input text is first transformed through the configured
/// [`ProcessType`] pipelines (producing up to 16 variants). All variants are scanned
/// simultaneously with a single Aho-Corasick pass. Each hit updates a
/// generation-stamped state matrix for the affected rule.
///
/// **Pass 2 — Evaluate**: Touched rules are checked: a rule fires if every AND
/// sub-pattern was satisfied in at least one text variant and no NOT sub-pattern was
/// triggered in any variant.
///
/// Composite process types can match across variants. For example,
/// `ProcessType::None | ProcessType::PinYin` lets one sub-pattern match the raw text and
/// another match the Pinyin-transformed variant during the same search. NOT segments are
/// global across those variants: if a veto pattern appears in any variant, the rule fails.
///
/// ## Thread Safety
///
/// `SimpleMatcher` is `Send + Sync`. All mutable scan state is stored in thread-local
/// `SimpleMatchState` instances, so concurrent calls from different threads are
/// independent with no contention.
///
/// ## Examples
///
/// ```rust
/// use matcher_rs::{SimpleMatcherBuilder, ProcessType};
///
/// let matcher = SimpleMatcherBuilder::new()
///     .add_word(ProcessType::None, 1, "apple&pie")
///     .add_word(ProcessType::None, 2, "banana~peel")
///     .build();
///
/// assert!(matcher.is_match("I like apple and pie"));
/// assert!(!matcher.is_match("I like banana peel"));
///
/// let results = matcher.process("apple and pie");
/// assert_eq!(results.len(), 1);
/// assert_eq!(results[0].word_id, 1);
/// ```
#[derive(Clone)]
pub struct SimpleMatcher {
    process_type_tree: Vec<ProcessTypeBitNode>,
    ac_matcher: InternalMatcher,
    ac_dedup_entries: Vec<PatternEntry>,
    ac_dedup_ranges: Vec<(usize, usize)>,
    rule_hot: Vec<RuleHot>,
    rule_cold: Vec<RuleCold>,
}

impl SimpleMatcher {
    /// Compiles a new [`SimpleMatcher`] from a `{ProcessType → {word_id → pattern}}` map.
    ///
    /// Prefer [`SimpleMatcherBuilder`](crate::SimpleMatcherBuilder) for a more ergonomic API.
    ///
    /// Construction is O(patterns × normalized_variants) and should happen once at startup.
    /// The steps are:
    /// 1. Parse `&`/`~` operators in each pattern into AND and NOT sub-patterns.
    /// 2. For each sub-pattern, generate all normalized text variants via
    ///    [`reduce_text_process_emit`].
    /// 3. Deduplicate all variants across all rules and process types into a single
    ///    pattern set.
    /// 4. Compile the pattern set into an Aho-Corasick automaton.
    /// 5. Build the transformation trie (`ProcessTypeBitNode` tree) for fast text
    ///    pre-processing at match time.
    ///
    /// One subtle detail is that sub-patterns are indexed under `process_type -
    /// ProcessType::Delete`, not the full `process_type`. `Delete` is applied to the input
    /// text variants before the automaton scan, so the indexed sub-pattern should stay in the
    /// same deleted-text coordinate space rather than being delete-processed twice.
    ///
    /// # Arguments
    /// * `process_type_word_map` — input rule table; the value type `I` must implement
    ///   `AsRef<str>` so both `&str` and `Cow<str>` are accepted.
    ///
    /// # Panics
    /// Panics if the Aho-Corasick automaton fails to compile. This should
    /// only happen if the de-duplicated pattern set is internally inconsistent, which cannot
    /// occur with well-formed input.
    pub fn new<'a, I, S1, S2>(
        process_type_word_map: &'a HashMap<ProcessType, HashMap<u32, I, S1>, S2>,
    ) -> SimpleMatcher
    where
        I: AsRef<str> + 'a,
    {
        let word_size: usize = process_type_word_map.values().map(|m| m.len()).sum();

        let mut process_type_set: HashSet<ProcessType> =
            HashSet::with_capacity(process_type_word_map.len());
        let mut dedup_entries: Vec<Vec<PatternEntry>> = Vec::with_capacity(word_size);
        let mut rule_hot: Vec<RuleHot> = Vec::with_capacity(word_size);
        let mut rule_cold: Vec<RuleCold> = Vec::with_capacity(word_size);
        let mut word_id_to_idx: HashMap<(ProcessType, u32), usize> =
            HashMap::with_capacity(word_size);

        let mut next_pattern_id: usize = 0;
        let mut dedup_patterns = Vec::with_capacity(word_size);
        let mut pattern_id_map: HashMap<Cow<str>, usize> = HashMap::with_capacity(word_size);

        for (&process_type, simple_word_map) in process_type_word_map {
            let word_process_type = process_type - ProcessType::Delete;
            process_type_set.insert(process_type);

            for (&simple_word_id, simple_word) in simple_word_map {
                if simple_word.as_ref().is_empty() {
                    continue;
                }
                let mut and_splits: HashMap<&str, i32> = HashMap::new();
                let mut not_splits: HashMap<&str, i32> = HashMap::new();

                let mut start = 0;
                let mut current_is_not = false;

                let mut add_sub_word = |word: &'a str, is_not: bool| {
                    if word.is_empty() {
                        return;
                    }
                    if is_not {
                        let entry = not_splits.entry(word).or_insert(1);
                        *entry -= 1;
                    } else {
                        let entry = and_splits.entry(word).or_insert(0);
                        *entry += 1;
                    }
                };

                for (index, char) in simple_word.as_ref().match_indices(['&', '~']) {
                    add_sub_word(&simple_word.as_ref()[start..index], current_is_not);
                    current_is_not = char == "~";
                    start = index + 1;
                }
                add_sub_word(&simple_word.as_ref()[start..], current_is_not);

                if and_splits.is_empty() && not_splits.is_empty() {
                    continue;
                }

                let and_count = and_splits.len();
                let segment_counts = and_splits
                    .values()
                    .copied()
                    .chain(not_splits.values().copied())
                    .collect::<Vec<i32>>();

                let expected_mask = if and_count > 0 && and_count <= BITMASK_CAPACITY {
                    u64::MAX >> (BITMASK_CAPACITY - and_count)
                } else {
                    0
                };

                let num_splits = segment_counts.len() as u16;
                let use_matrix = and_count > BITMASK_CAPACITY
                    || segment_counts.len() > BITMASK_CAPACITY
                    || segment_counts[..and_count].iter().any(|&v| v != 1)
                    || segment_counts[and_count..].iter().any(|&v| v != 0);

                let rule_idx = if let Some(&existing_idx) =
                    word_id_to_idx.get(&(process_type, simple_word_id))
                {
                    rule_hot[existing_idx] = RuleHot {
                        segment_counts,
                        and_count,
                        expected_mask,
                        use_matrix,
                        num_splits,
                    };
                    rule_cold[existing_idx] = RuleCold {
                        word_id: simple_word_id,
                        word: simple_word.as_ref().to_owned(),
                    };
                    existing_idx
                } else {
                    let idx = rule_hot.len();
                    word_id_to_idx.insert((process_type, simple_word_id), idx);
                    rule_hot.push(RuleHot {
                        segment_counts,
                        and_count,
                        expected_mask,
                        use_matrix,
                        num_splits,
                    });
                    rule_cold.push(RuleCold {
                        word_id: simple_word_id,
                        word: simple_word.as_ref().to_owned(),
                    });
                    idx
                };

                for (offset, &split_word) in and_splits.keys().chain(not_splits.keys()).enumerate()
                {
                    for ac_word in reduce_text_process_emit(word_process_type, split_word) {
                        let Some(&existing_dedup_id) = pattern_id_map.get(ac_word.as_ref()) else {
                            pattern_id_map.insert(ac_word.clone(), next_pattern_id);
                            dedup_entries.push(vec![PatternEntry {
                                process_type_mask: 1u64 << process_type.bits(),
                                rule_idx: rule_idx as u32,
                                offset: offset as u16,
                            }]);
                            dedup_patterns.push(ac_word);
                            next_pattern_id += 1;
                            continue;
                        };
                        dedup_entries[existing_dedup_id].push(PatternEntry {
                            process_type_mask: 1u64 << process_type.bits(),
                            rule_idx: rule_idx as u32,
                            offset: offset as u16,
                        });
                    }
                }
            }
        }

        let process_type_tree = build_process_type_tree(&process_type_set);

        let patterns = dedup_patterns
            .iter()
            .map(|ac_word| ac_word.as_ref())
            .collect::<Vec<_>>();

        let ac_matcher = if false {
            InternalMatcher::AhoCorasick({
                #[cfg(feature = "dfa")]
                let aho_corasick_kind = AhoCorasickKind::DFA;
                #[cfg(not(feature = "dfa"))]
                let aho_corasick_kind = AhoCorasickKind::ContiguousNFA;

                AhoCorasickBuilder::new()
                    .kind(Some(aho_corasick_kind))
                    .build(patterns)
                    .unwrap()
            })
        } else {
            InternalMatcher::DoubleArrayAhoCorasick(
                CharwiseDoubleArrayAhoCorasickBuilder::new()
                    .match_kind(DoubleArrayAhoCorasickMatchKind::Standard)
                    .build(patterns)
                    .unwrap(),
            )
        };

        let mut ac_dedup_entries = Vec::with_capacity(dedup_entries.iter().map(|v| v.len()).sum());
        let mut ac_dedup_ranges = Vec::with_capacity(dedup_entries.len());
        for entries in dedup_entries {
            let start = ac_dedup_entries.len();
            let len = entries.len();
            ac_dedup_entries.extend(entries);
            ac_dedup_ranges.push((start, len));
        }

        SimpleMatcher {
            process_type_tree,
            ac_matcher,
            ac_dedup_entries,
            ac_dedup_ranges,
            rule_hot,
            rule_cold,
        }
    }

    /// Returns `true` if `text` satisfies at least one registered pattern.
    ///
    /// Equivalent to `!self.process(text).is_empty()` but short-circuits as soon as the
    /// first matching rule is found, making it significantly faster when a match is expected.
    /// Returns `false` immediately for empty input.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use matcher_rs::{SimpleMatcherBuilder, ProcessType};
    ///
    /// let matcher = SimpleMatcherBuilder::new()
    ///     .add_word(ProcessType::None, 1, "hello")
    ///     .add_word(ProcessType::None, 2, "world")
    ///     .build();
    ///
    /// assert!(matcher.is_match("hello there"));
    /// assert!(matcher.is_match("beautiful world"));
    /// assert!(!matcher.is_match("hi planet!"));
    /// ```
    pub fn is_match(&self, text: &str) -> bool {
        if text.is_empty() {
            return false;
        }
        let tree = &self.process_type_tree;
        let max_pt = tree.len();
        SIMPLE_MATCH_STATE.with(|state_cell| {
            let mut state = state_cell.borrow_mut();
            state.prepare(self.rule_hot.len());
            let (text_masks, stopped) =
                walk_process_tree::<true, _>(tree, text, &mut |txt, idx, mask| {
                    self.scan_variant(txt, idx, mask, max_pt, &mut state, true)
                });
            if stopped {
                return_processed_string_to_pool(text_masks);
                return true;
            }
            let generation = state.generation;
            let result = state.touched_indices.iter().any(|&rule_idx| {
                if state.word_states[rule_idx].not_generation == generation {
                    return false;
                }
                Self::is_rule_satisfied(
                    &self.rule_hot[rule_idx],
                    &state.word_states,
                    &state.matrix,
                    rule_idx,
                    max_pt,
                )
            });
            return_processed_string_to_pool(text_masks);
            result
        })
    }

    /// Returns all patterns that match `text`.
    ///
    /// Unlike [`is_match`](Self::is_match), this always completes the full two-pass scan
    /// and collects every satisfied rule. Returns an empty `Vec` for empty input.
    /// Results are appended in discovery order, which is deterministic for a given matcher
    /// and input but should not be treated as a stable sort order.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use matcher_rs::{SimpleMatcherBuilder, ProcessType};
    ///
    /// let matcher = SimpleMatcherBuilder::new()
    ///     .add_word(ProcessType::None, 1, "apple")
    ///     .add_word(ProcessType::None, 2, "banana")
    ///     .build();
    ///
    /// let results = matcher.process("I have an apple and a banana");
    /// assert_eq!(results.len(), 2);
    /// ```
    pub fn process<'a>(&'a self, text: &'a str) -> Vec<SimpleResult<'a>> {
        let mut results = Vec::new();
        self.process_into(text, &mut results);
        results
    }

    /// Appends all patterns that match `text` to `results`.
    ///
    /// Like [`process`](Self::process) but reuses a caller-supplied buffer, avoiding a
    /// `Vec` allocation per call. Useful in high-throughput loops where the caller can
    /// clear and reuse the same `Vec` across iterations.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use matcher_rs::{SimpleMatcherBuilder, ProcessType, SimpleResult};
    ///
    /// let matcher = SimpleMatcherBuilder::new()
    ///     .add_word(ProcessType::None, 1, "apple")
    ///     .build();
    ///
    /// let mut results: Vec<SimpleResult> = Vec::new();
    /// matcher.process_into("I have an apple", &mut results);
    /// assert_eq!(results.len(), 1);
    /// ```
    pub fn process_into<'a>(&'a self, text: &'a str, results: &mut Vec<SimpleResult<'a>>) {
        if text.is_empty() {
            return;
        }
        let (processed, _) =
            walk_process_tree::<false, _>(&self.process_type_tree, text, &mut |_, _, _| false);
        self.process_preprocessed_into(&processed, results);
        return_processed_string_to_pool(processed);
    }

    /// Runs both Pass 1 and Pass 2, appending all satisfied rules to `results`.
    fn process_preprocessed_into<'a>(
        &'a self,
        processed_text_process_type_masks: &ProcessedTextMasks<'a>,
        results: &mut Vec<SimpleResult<'a>>,
    ) {
        SIMPLE_MATCH_STATE.with(|state| {
            let mut state = state.borrow_mut();
            state.prepare(self.rule_hot.len());

            self.scan_all_variants(processed_text_process_type_masks, &mut state, false);

            let generation = state.generation;
            let num_variants = processed_text_process_type_masks.len();

            for &rule_idx in &state.touched_indices {
                if state.word_states[rule_idx].not_generation == generation {
                    continue;
                }
                if Self::is_rule_satisfied(
                    &self.rule_hot[rule_idx],
                    &state.word_states,
                    &state.matrix,
                    rule_idx,
                    num_variants,
                ) {
                    let cold = &self.rule_cold[rule_idx];
                    results.push(SimpleResult {
                        word_id: cold.word_id,
                        word: Cow::Borrowed(&cold.word),
                    });
                }
            }
        });
    }

    /// Pass 1: scans all text variants with the automaton, updating [`SimpleMatchState`].
    ///
    /// For each text variant in `processed_text_process_type_masks` the automaton finds
    /// all overlapping sub-pattern hits. Each hit is dispatched to [`Self::process_match`],
    /// which updates the affected rule's counters. If `exit_early` is `true`, scanning
    /// halts as soon as a rule becomes fully satisfied (used by `is_match_preprocessed`).
    ///
    /// Returns `true` only when `exit_early` is `true` and at least one rule fired early.
    fn scan_all_variants<'a>(
        &'a self,
        processed_text_process_type_masks: &ProcessedTextMasks<'a>,
        state: &mut SimpleMatchState,
        exit_early: bool,
    ) -> bool {
        if self.ac_dedup_ranges.is_empty() {
            return false;
        }

        let num_variants = processed_text_process_type_masks.len();

        for (index, (processed_text, process_type_mask)) in
            processed_text_process_type_masks.iter().enumerate()
        {
            if *process_type_mask == 0 {
                continue;
            }
            if self.scan_variant(
                processed_text.as_ref(),
                index,
                *process_type_mask,
                num_variants,
                state,
                exit_early,
            ) {
                return true;
            }
        }
        false
    }

    /// Scans one pre-processed text variant through the automaton and evaluates rule counters.
    ///
    /// This is the inner loop of Pass 1 + Pass 2 for a single text variant produced by the
    /// transformation pipeline.
    ///
    /// # Arguments
    /// * `processed_text` — the transformed text variant to scan.
    /// * `index` — ordinal of this variant among all variants generated for the current input.
    ///   Used by matrix-path rules to track which variant a repeated AND/NOT segment hit came
    ///   from, preventing a single sub-pattern matched in multiple variants from counting twice
    ///   for the same variant slot.
    /// * `process_type_mask` — bitmask of the `ProcessType` bits that produced this variant,
    ///   used to filter which rules are eligible for this scan.
    /// * `num_variants` — total number of variants being scanned; required by the matrix path
    ///   to index the per-variant counter columns.
    /// * `state` — mutable per-call match state (word states, counters, touched list).
    /// * `exit_early` — if `true`, return `true` as soon as any rule is fully satisfied
    ///   (used by [`SimpleMatcher::is_match`] for short-circuiting).
    ///
    /// Returns `true` if a rule was fully satisfied and `exit_early` is set; `false` otherwise.
    #[inline(always)]
    fn scan_variant(
        &self,
        processed_text: &str,
        index: usize,
        process_type_mask: u64,
        num_variants: usize,
        state: &mut SimpleMatchState,
        exit_early: bool,
    ) -> bool {
        // `index` identifies which processed text variant this scan came from, so matrix-path
        // rules can track repeated AND / NOT segments per variant.
        match &self.ac_matcher {
            InternalMatcher::AhoCorasick(ac_matcher) => {
                for ac_dedup_result in ac_matcher.find_overlapping_iter(processed_text) {
                    let pattern_idx = ac_dedup_result.pattern().as_usize();
                    if self.process_match(
                        pattern_idx,
                        index,
                        process_type_mask,
                        num_variants,
                        state,
                        exit_early,
                    ) {
                        return true;
                    }
                }
                false
            }
            InternalMatcher::DoubleArrayAhoCorasick(ac_matcher) => {
                for ac_dedup_result in ac_matcher.find_overlapping_iter(processed_text) {
                    let pattern_idx = ac_dedup_result.value() as usize;
                    if self.process_match(
                        pattern_idx,
                        index,
                        process_type_mask,
                        num_variants,
                        state,
                        exit_early,
                    ) {
                        return true;
                    }
                }
                false
            }
        }
    }

    /// Updates rule counters for a single automaton hit (called from Pass 1).
    ///
    /// Looks up all [`PatternEntry`] records for `pattern_idx`, skipping any rule whose
    /// process-type bitmask doesn't overlap with the current text variant's `process_type_mask`
    /// or that has already been disqualified this generation.
    ///
    /// For an AND sub-pattern hit: decrements the counter and sets the bit in `satisfied_mask`
    /// when the counter reaches ≤0. For a NOT sub-pattern hit: sets `not_generation` to
    /// permanently disqualify the rule. Returns `true` if `exit_early` and a rule just became
    /// fully satisfied.
    ///
    /// Repeated sub-patterns such as `a&a&a` are represented as counters rather than booleans,
    /// so the rule is satisfied only after enough hits arrive. Rules that do not fit the simple
    /// bitmask fast-path fall back to the per-rule matrix, which tracks counts per text variant.
    #[inline(always)]
    fn process_match(
        &self,
        pattern_idx: usize,
        text_index: usize,
        process_type_mask: u64,
        num_variants: usize,
        state: &mut SimpleMatchState,
        exit_early: bool,
    ) -> bool {
        let generation = state.generation;
        let (start, len) = self.ac_dedup_ranges[pattern_idx];
        for entry in &self.ac_dedup_entries[start..start + len] {
            let &PatternEntry {
                process_type_mask: match_process_type_mask,
                rule_idx,
                offset,
            } = entry;

            let rule_idx = rule_idx as usize;
            let offset = offset as usize;

            if process_type_mask & match_process_type_mask == 0
                || state.word_states[rule_idx].not_generation == generation
            {
                continue;
            }

            let rule = &self.rule_hot[rule_idx];

            if state.word_states[rule_idx].satisfied_generation == generation {
                if exit_early {
                    return true;
                }
                continue;
            }

            if state.word_states[rule_idx].matrix_generation != generation {
                state.word_states[rule_idx].matrix_generation = generation;
                state.touched_indices.push(rule_idx);
                state.word_states[rule_idx].satisfied_mask = 0;

                if rule.use_matrix {
                    Self::init_matrix(
                        &mut state.matrix[rule_idx],
                        &rule.segment_counts,
                        num_variants,
                    );
                }
            }

            let is_satisfied = if rule.use_matrix {
                let flat_matrix = &mut state.matrix[rule_idx];
                let bit = &mut flat_matrix[offset * num_variants + text_index];
                if offset < rule.and_count {
                    *bit -= 1; // AND segment: counts down toward satisfaction (≤0 = satisfied)
                } else {
                    *bit += 1; // NOT segment: counts up toward disqualification (>0 = fired)
                }

                if offset < rule.and_count {
                    if *bit <= 0 && offset < BITMASK_CAPACITY {
                        state.word_states[rule_idx].satisfied_mask |= 1u64 << offset;
                    }
                } else if *bit > 0 {
                    state.word_states[rule_idx].not_generation = generation;
                }

                Self::is_rule_satisfied(
                    rule,
                    &state.word_states,
                    &state.matrix,
                    rule_idx,
                    num_variants,
                )
            } else if offset < rule.and_count {
                if offset < BITMASK_CAPACITY {
                    state.word_states[rule_idx].satisfied_mask |= 1u64 << offset;
                }
                let expected_mask = rule.expected_mask;
                let satisfied = state.word_states[rule_idx].satisfied_mask == expected_mask;
                if satisfied && rule.and_count == rule.num_splits as usize {
                    state.word_states[rule_idx].satisfied_generation = generation;
                }
                satisfied
            } else {
                state.word_states[rule_idx].not_generation = generation;
                false
            };

            if exit_early
                && is_satisfied
                && rule.and_count == rule.num_splits as usize
                && state.word_states[rule_idx].not_generation != generation
            {
                return true;
            }
        }
        false
    }

    /// Returns `true` if all AND sub-patterns of `rule` have been satisfied.
    ///
    /// Uses the bitmask fast-path when `expected_mask > 0` (rules with ≤64 unique AND
    /// sub-patterns); falls back to scanning the flat counter matrix otherwise.
    #[inline(always)]
    fn is_rule_satisfied(
        rule: &RuleHot,
        word_states: &[WordState],
        matrix: &[TinyVec<[i32; 16]>],
        rule_idx: usize,
        num_variants: usize,
    ) -> bool {
        let expected_mask = rule.expected_mask;
        if expected_mask > 0 {
            return word_states[rule_idx].satisfied_mask == expected_mask;
        }
        let num_splits = rule.num_splits as usize;
        let flat_matrix = &matrix[rule_idx];
        (0..num_splits).all(|s| {
            flat_matrix[s * num_variants..(s + 1) * num_variants]
                .iter()
                .any(|&bit| bit <= 0)
        })
    }

    /// Initializes the flat counter matrix for a rule on its first touch in a generation.
    ///
    /// Marked `#[cold]` because the matrix path is rare (rules with >64 segments or
    /// repeated sub-patterns). Extracting it helps the compiler optimize the common
    /// bitmask fast-path layout in `process_match`.
    #[cold]
    #[inline(never)]
    fn init_matrix(
        flat_matrix: &mut TinyVec<[i32; 16]>,
        segment_counts: &[i32],
        num_variants: usize,
    ) {
        let num_splits = segment_counts.len();
        flat_matrix.clear();
        flat_matrix.resize(num_splits * num_variants, 0i32);
        for (s, &bit) in segment_counts.iter().enumerate() {
            let row_start = s * num_variants;
            flat_matrix[row_start..row_start + num_variants].fill(bit);
        }
    }
}
