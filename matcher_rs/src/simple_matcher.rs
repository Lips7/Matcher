use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

#[cfg(not(feature = "vectorscan"))]
use aho_corasick::AhoCorasickKind;
use aho_corasick::{AhoCorasick, AhoCorasickBuilder};
use serde::Serialize;
use tinyvec::TinyVec;

use crate::process::process_matcher::{
    ProcessType, ProcessTypeBitNode, ProcessedTextMasks, build_process_type_tree,
    reduce_text_process_emit, reduce_text_process_with_tree, return_processed_string_to_pool,
};
#[cfg(feature = "vectorscan")]
use crate::vectorscan::{Scratch, VectorscanScanner};

/// Per-rule match state for a single search, keyed by generation ID.
///
/// Stored in a flat `Vec` inside [`SimpleMatchState`], one entry per rule (`WordConf`).
/// Generation IDs implement a sparse-set pattern: comparing a field against the current
/// `SimpleMatchState::generation` determines whether the field was written during this
/// search without requiring a full zero-fill between calls.
///
/// * `matrix_generation` — set to the current generation when this rule is first touched.
/// * `not_generation` — set to the current generation when a NOT sub-pattern fires,
///   permanently disqualifying this rule for the remainder of the search.
/// * `satisfied_mask` — bitmask of AND sub-patterns (up to 64) satisfied so far.
#[derive(Default, Clone, Copy)]
struct WordState {
    matrix_generation: u32,
    not_generation: u32,
    satisfied_mask: u64,
}

/// Reusable per-thread scratch space for a single [`SimpleMatcher`] scan.
///
/// Allocated once and stored in a `thread_local!`; reused across calls via the generation
/// trick in [`WordState`] to avoid clearing the full state between searches.
///
/// * `word_states` — flat array indexed by `word_conf_idx`; one [`WordState`] per rule.
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
    #[cfg(feature = "vectorscan")]
    vectorscan_scratch: Option<Scratch>,
}

impl SimpleMatchState {
    /// Creates an empty `SimpleMatchState` ready for its first search.
    fn new() -> Self {
        Self {
            word_states: Vec::new(),
            matrix: Vec::new(),
            touched_indices: Vec::new(),
            generation: 0,
            #[cfg(feature = "vectorscan")]
            vectorscan_scratch: None,
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
/// * `word` — the original pattern string, borrowed from the compiled `WordConf`.
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

/// Compiled configuration for a single pattern rule, derived from parsing `&`/`~` operators.
///
/// Produced once during [`SimpleMatcher::new`] and stored in `word_conf_list`.
///
/// * `word_id` — caller-assigned identifier returned in [`SimpleResult`].
/// * `word` — the original pattern string (stored for inclusion in results).
/// * `split_bit` — per-sub-pattern counters. Indices `0..not_offset` are AND segments
///   (initial value +1, decremented toward ≤0 to signal satisfaction); indices
///   `not_offset..` are NOT segments (initial value 0, incremented toward >0 to signal
///   disqualification).
/// * `not_offset` — boundary in `split_bit` separating AND from NOT segments.
/// * `expected_mask` — bitmask of AND segments that must all reach ≤0. Non-zero only
///   when `not_offset ≤ 64` and all AND segments appear exactly once (the common, fast case).
/// * `use_matrix` — `true` when the rule requires the full counter matrix (>64 segments,
///   repeated sub-patterns across `&`-splits, or a non-trivial NOT pattern).
#[derive(Debug, Clone)]
struct WordConf {
    word_id: u32,
    word: String,
    split_bit: Vec<i32>,
    not_offset: usize,
    expected_mask: u64,
    use_matrix: bool,
}

/// Links a deduplicated automaton pattern back to the rule and sub-pattern it belongs to.
///
/// Stored in the flat `ac_dedup_entries` array; a `(start, len)` range in
/// `ac_dedup_ranges` maps each automaton pattern index to its slice of entries.
///
/// * `process_type_mask` — bitmask of [`ProcessType`] bits that produced this pattern;
///   used to discard hits from text variants that don't match the rule's pipeline.
/// * `word_conf_idx` — index into `word_conf_list` identifying the owning rule.
/// * `offset` — index into `split_bit` of the owning [`WordConf`]; identifies which
///   AND or NOT sub-pattern was matched.
#[derive(Debug, Clone)]
struct WordConfEntry {
    process_type_mask: u64,
    word_conf_idx: u32,
    offset: u16,
}

/// The underlying scan engine used by [`SimpleMatcher`].
///
/// Selects between standard Aho-Corasick and Vectorscan at runtime based on feature flags
/// and whether any patterns were registered. The standard Aho-Corasick variant is used when
/// the `vectorscan` feature is disabled or when the pattern list is empty.
#[derive(Debug, Clone)]
enum AcMatcher {
    /// Standard Aho-Corasick (DFA or ContiguousNFA depending on the `dfa` feature flag).
    #[cfg_attr(feature = "vectorscan", allow(dead_code))]
    AhoCorasick(AhoCorasick),
    /// Intel Vectorscan (Hyperscan): SIMD-accelerated literal matching.
    /// Only available with the `vectorscan` feature; not supported on Windows or ARM64.
    #[cfg(feature = "vectorscan")]
    Vectorscan(VectorscanScanner),
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
/// ```
///
/// ## Two-Pass Matching
///
/// **Pass 1 — Scan**: The input text is first transformed through the configured
/// [`ProcessType`] pipelines (producing up to 16 variants). All variants are scanned
/// simultaneously with a single Aho-Corasick or Vectorscan pass. Each hit updates a
/// generation-stamped state matrix for the affected rule.
///
/// **Pass 2 — Evaluate**: Touched rules are checked: a rule fires if every AND
/// sub-pattern was satisfied in at least one text variant and no NOT sub-pattern was
/// triggered in any variant.
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
#[derive(Debug, Clone)]
pub struct SimpleMatcher {
    process_type_tree: Vec<ProcessTypeBitNode>,
    ac_matcher: AcMatcher,
    ac_dedup_entries: Vec<WordConfEntry>,
    ac_dedup_ranges: Vec<(usize, usize)>,
    word_conf_list: Vec<WordConf>,
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
    /// 4. Compile the pattern set into an Aho-Corasick (or Vectorscan) automaton.
    /// 5. Build the transformation trie (`ProcessTypeBitNode` tree) for fast text
    ///    pre-processing at match time.
    ///
    /// # Arguments
    /// * `process_type_word_map` — input rule table; the value type `I` must implement
    ///   `AsRef<str>` so both `&str` and `Cow<str>` are accepted.
    pub fn new<'a, I, S1, S2>(
        process_type_word_map: &'a HashMap<ProcessType, HashMap<u32, I, S1>, S2>,
    ) -> SimpleMatcher
    where
        I: AsRef<str> + 'a,
    {
        let word_size: usize = process_type_word_map.values().map(|m| m.len()).sum();

        let mut process_type_set = HashSet::with_capacity(process_type_word_map.len());
        let mut dedup_entries: Vec<Vec<WordConfEntry>> = Vec::with_capacity(word_size);
        let mut word_conf_list: Vec<WordConf> = Vec::with_capacity(word_size);
        let mut word_id_to_idx: HashMap<(ProcessType, u32), usize> =
            HashMap::with_capacity(word_size);

        let mut next_pattern_id: usize = 0;
        let mut dedup_patterns = Vec::with_capacity(word_size);
        let mut pattern_id_map: HashMap<Cow<str>, usize> = HashMap::with_capacity(word_size);

        for (&process_type, simple_word_map) in process_type_word_map {
            let word_process_type = process_type - ProcessType::Delete;
            process_type_set.insert(process_type.bits());

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

                let not_offset = and_splits.len();
                let split_bit = and_splits
                    .values()
                    .copied()
                    .chain(not_splits.values().copied())
                    .collect::<Vec<i32>>();

                let expected_mask = if not_offset > 0 && not_offset <= 64 {
                    u64::MAX >> (64 - not_offset)
                } else {
                    0
                };

                let use_matrix = not_offset > 64
                    || split_bit.len() > 64
                    || split_bit[..not_offset].iter().any(|&v| v != 1)
                    || split_bit[not_offset..].iter().any(|&v| v != 0);

                let word_conf_idx = if let Some(&existing_idx) =
                    word_id_to_idx.get(&(process_type, simple_word_id))
                {
                    word_conf_list[existing_idx] = WordConf {
                        word_id: simple_word_id,
                        word: simple_word.as_ref().to_owned(),
                        split_bit,
                        not_offset,
                        expected_mask,
                        use_matrix,
                    };
                    existing_idx
                } else {
                    let idx = word_conf_list.len();
                    word_id_to_idx.insert((process_type, simple_word_id), idx);
                    word_conf_list.push(WordConf {
                        word_id: simple_word_id,
                        word: simple_word.as_ref().to_owned(),
                        split_bit,
                        not_offset,
                        expected_mask,
                        use_matrix,
                    });
                    idx
                };

                for (offset, &split_word) in and_splits.keys().chain(not_splits.keys()).enumerate()
                {
                    for ac_word in reduce_text_process_emit(word_process_type, split_word) {
                        let Some(&existing_dedup_id) = pattern_id_map.get(ac_word.as_ref()) else {
                            pattern_id_map.insert(ac_word.clone(), next_pattern_id);
                            dedup_entries.push(vec![WordConfEntry {
                                process_type_mask: 1u64 << process_type.bits(),
                                word_conf_idx: word_conf_idx as u32,
                                offset: offset as u16,
                            }]);
                            dedup_patterns.push(ac_word);
                            next_pattern_id += 1;
                            continue;
                        };
                        dedup_entries[existing_dedup_id].push(WordConfEntry {
                            process_type_mask: 1u64 << process_type.bits(),
                            word_conf_idx: word_conf_idx as u32,
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

        #[cfg(feature = "vectorscan")]
        let ac_matcher = if patterns.is_empty() {
            AcMatcher::AhoCorasick(AhoCorasickBuilder::new().build(&patterns).unwrap())
        } else {
            let flags = vec![0u32; patterns.len()];
            AcMatcher::Vectorscan(
                VectorscanScanner::new_literal(&patterns, &flags)
                    .expect("failed to compile vectorscan literal database"),
            )
        };

        #[cfg(not(feature = "vectorscan"))]
        let ac_matcher = {
            #[cfg(feature = "dfa")]
            let aho_corasick_kind = AhoCorasickKind::DFA;
            #[cfg(not(feature = "dfa"))]
            let aho_corasick_kind = AhoCorasickKind::ContiguousNFA;

            AcMatcher::AhoCorasick(
                AhoCorasickBuilder::new()
                    .kind(Some(aho_corasick_kind))
                    .build(&patterns)
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
            word_conf_list,
        }
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

        let processed_times = processed_text_process_type_masks.len();

        for (index, (processed_text, process_type_mask)) in
            processed_text_process_type_masks.iter().enumerate()
        {
            match &self.ac_matcher {
                AcMatcher::AhoCorasick(ac_matcher) => {
                    for ac_dedup_result in ac_matcher.find_overlapping_iter(processed_text.as_ref())
                    {
                        let pattern_idx = ac_dedup_result.pattern().as_usize();
                        if self.process_match(
                            pattern_idx,
                            index,
                            *process_type_mask,
                            processed_times,
                            state,
                            exit_early,
                        ) {
                            return true;
                        }
                    }
                }
                #[cfg(feature = "vectorscan")]
                AcMatcher::Vectorscan(scanner) => {
                    let mut found = false;
                    let mut scratch = state
                        .vectorscan_scratch
                        .take()
                        .unwrap_or_else(|| unsafe { Scratch::new(scanner.as_db_ptr()).unwrap() });

                    let _ = unsafe { scratch.update(scanner.as_db_ptr()) };

                    let _ = scanner.scan_with_scratch(
                        processed_text.as_ref().as_bytes(),
                        &mut scratch,
                        |pattern_idx| {
                            if !found
                                && self.process_match(
                                    pattern_idx,
                                    index,
                                    *process_type_mask,
                                    processed_times,
                                    state,
                                    exit_early,
                                )
                            {
                                found = true;
                                false // stop scanning
                            } else {
                                !found // continue if not found or not exit_early
                            }
                        },
                    );
                    state.vectorscan_scratch = Some(scratch);

                    if found {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Updates rule counters for a single automaton hit (called from Pass 1).
    ///
    /// Looks up all [`WordConfEntry`] records for `pattern_idx`, skipping any rule whose
    /// process-type bitmask doesn't overlap with the current text variant's `process_type_mask`
    /// or that has already been disqualified this generation.
    ///
    /// For an AND sub-pattern hit: decrements the counter and sets the bit in `satisfied_mask`
    /// when the counter reaches ≤0. For a NOT sub-pattern hit: sets `not_generation` to
    /// permanently disqualify the rule. Returns `true` if `exit_early` and a rule just became
    /// fully satisfied.
    #[inline(always)]
    fn process_match(
        &self,
        pattern_idx: usize,
        text_index: usize,
        process_type_mask: u64,
        processed_times: usize,
        state: &mut SimpleMatchState,
        exit_early: bool,
    ) -> bool {
        let generation = state.generation;
        let (start, len) = self.ac_dedup_ranges[pattern_idx];
        for entry in &self.ac_dedup_entries[start..start + len] {
            let &WordConfEntry {
                process_type_mask: match_process_type_mask,
                word_conf_idx,
                offset,
            } = entry;

            let word_conf_idx = word_conf_idx as usize;
            let offset = offset as usize;

            if process_type_mask & match_process_type_mask == 0
                || state.word_states[word_conf_idx].not_generation == generation
            {
                continue;
            }

            let word_conf = &self.word_conf_list[word_conf_idx];

            if state.word_states[word_conf_idx].matrix_generation == generation
                && word_conf.expected_mask > 0
                && word_conf.not_offset == word_conf.split_bit.len()
                && state.word_states[word_conf_idx].satisfied_mask == word_conf.expected_mask
            {
                if exit_early {
                    return true;
                }
                continue;
            }

            if state.word_states[word_conf_idx].matrix_generation != generation {
                state.word_states[word_conf_idx].matrix_generation = generation;
                state.touched_indices.push(word_conf_idx);
                state.word_states[word_conf_idx].satisfied_mask = 0;

                if word_conf.use_matrix {
                    let num_splits = word_conf.split_bit.len();
                    let flat_matrix = &mut state.matrix[word_conf_idx];
                    flat_matrix.clear();
                    flat_matrix.resize(num_splits * processed_times, 0i32);
                    for (s, &bit) in word_conf.split_bit.iter().enumerate() {
                        let row_start = s * processed_times;
                        flat_matrix[row_start..row_start + processed_times].fill(bit);
                    }
                }
            }

            let is_satisfied = if word_conf.use_matrix {
                let flat_matrix = &mut state.matrix[word_conf_idx];
                let bit = &mut flat_matrix[offset * processed_times + text_index];
                *bit += (offset < word_conf.not_offset) as i32 * -2 + 1;

                if offset < word_conf.not_offset {
                    if *bit <= 0 && offset < 64 {
                        state.word_states[word_conf_idx].satisfied_mask |= 1u64 << offset;
                    }
                } else if *bit > 0 {
                    state.word_states[word_conf_idx].not_generation = generation;
                }

                is_rule_satisfied(
                    word_conf,
                    &state.word_states,
                    &state.matrix,
                    word_conf_idx,
                    processed_times,
                )
            } else if offset < word_conf.not_offset {
                if offset < 64 {
                    state.word_states[word_conf_idx].satisfied_mask |= 1u64 << offset;
                }
                let expected_mask = word_conf.expected_mask;
                state.word_states[word_conf_idx].satisfied_mask == expected_mask
            } else {
                state.word_states[word_conf_idx].not_generation = generation;
                false
            };

            if exit_early
                && is_satisfied
                && word_conf.not_offset == word_conf.split_bit.len()
                && state.word_states[word_conf_idx].not_generation != generation
            {
                return true;
            }
        }
        false
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
    pub fn is_match<'a>(&'a self, text: &'a str) -> bool {
        if text.is_empty() {
            return false;
        }

        let processed_text_process_type_masks =
            reduce_text_process_with_tree(&self.process_type_tree, text);

        let result = self.is_match_preprocessed(&processed_text_process_type_masks);

        return_processed_string_to_pool(processed_text_process_type_masks);

        result
    }

    /// Returns all patterns that match `text`.
    ///
    /// Unlike [`is_match`](Self::is_match), this always completes the full two-pass scan
    /// and collects every satisfied rule. Returns an empty `Vec` for empty input.
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
        if text.is_empty() {
            return Vec::new();
        }

        let processed_text_process_type_masks =
            reduce_text_process_with_tree(&self.process_type_tree, text);

        let result = self.process_preprocessed(&processed_text_process_type_masks);

        return_processed_string_to_pool(processed_text_process_type_masks);

        result
    }

    /// Pass 2 (boolean): runs Pass 1 then checks touched rules for any full match.
    ///
    /// Iterates only over `touched_indices` (rules that had at least one sub-pattern hit).
    /// Skips any rule whose `not_generation` equals the current generation (NOT fired).
    /// For rules with `expected_mask`, uses a fast bitmask comparison; otherwise falls back
    /// to scanning the flat counter matrix.
    fn is_match_preprocessed<'a>(
        &'a self,
        processed_text_process_type_masks: &ProcessedTextMasks<'a>,
    ) -> bool {
        SIMPLE_MATCH_STATE.with(|state| {
            let mut state = state.borrow_mut();
            state.prepare(self.word_conf_list.len());

            if self.scan_all_variants(processed_text_process_type_masks, &mut state, true) {
                return true;
            }

            let generation = state.generation;
            let processed_times = processed_text_process_type_masks.len();

            state.touched_indices.iter().any(|&word_conf_idx| {
                if state.word_states[word_conf_idx].not_generation == generation {
                    return false;
                }
                let word_conf = &self.word_conf_list[word_conf_idx];
                is_rule_satisfied(
                    word_conf,
                    &state.word_states,
                    &state.matrix,
                    word_conf_idx,
                    processed_times,
                )
            })
        })
    }

    /// Pass 2 (collect): runs Pass 1 then returns all satisfied rules as [`SimpleResult`]s.
    ///
    /// Same evaluation logic as `is_match_preprocessed` but collects all matches
    /// instead of short-circuiting on the first.
    fn process_preprocessed<'a>(
        &'a self,
        processed_text_process_type_masks: &ProcessedTextMasks<'a>,
    ) -> Vec<SimpleResult<'a>> {
        SIMPLE_MATCH_STATE.with(|state| {
            let mut state = state.borrow_mut();
            state.prepare(self.word_conf_list.len());

            self.scan_all_variants(processed_text_process_type_masks, &mut state, false);

            let generation = state.generation;
            let processed_times = processed_text_process_type_masks.len();

            state
                .touched_indices
                .iter()
                .filter_map(|&word_conf_idx| {
                    if state.word_states[word_conf_idx].not_generation == generation {
                        return None;
                    }
                    let word_conf = &self.word_conf_list[word_conf_idx];
                    let is_satisfied = is_rule_satisfied(
                        word_conf,
                        &state.word_states,
                        &state.matrix,
                        word_conf_idx,
                        processed_times,
                    );
                    is_satisfied.then_some(SimpleResult {
                        word_id: word_conf.word_id,
                        word: Cow::Borrowed(&word_conf.word),
                    })
                })
                .collect()
        })
    }
}

/// Returns `true` if all AND sub-patterns of `word_conf` have been satisfied.
///
/// Uses the bitmask fast-path when `expected_mask > 0` (rules with ≤64 unique AND
/// sub-patterns); falls back to scanning the flat counter matrix otherwise.
#[inline(always)]
fn is_rule_satisfied(
    word_conf: &WordConf,
    word_states: &[WordState],
    matrix: &[TinyVec<[i32; 16]>],
    word_conf_idx: usize,
    processed_times: usize,
) -> bool {
    let expected_mask = word_conf.expected_mask;
    if expected_mask > 0 {
        return word_states[word_conf_idx].satisfied_mask == expected_mask;
    }
    let num_splits = word_conf.split_bit.len();
    let flat_matrix = &matrix[word_conf_idx];
    (0..num_splits).all(|s| {
        flat_matrix[s * processed_times..(s + 1) * processed_times]
            .iter()
            .any(|&bit| bit <= 0)
    })
}
