//! Construction of [`super::SimpleMatcher`] — rule parsing, emitted-pattern
//! deduplication, and matcher compilation.
//!
//! The construction pipeline has three stages:
//!
//! 1. **Index table** ([`SimpleMatcher::build_process_type_index_table`]) —
//!    assigns a compact sequential index (0..N) to each distinct
//!    [`ProcessType`] present in the input.
//!
//! 2. **Rule parsing** ([`SimpleMatcher::parse_rules`]) — splits each rule
//!    string on `&`/`~` operators, then splits each segment on `|` to extract
//!    OR alternatives, counts repeated sub-patterns, determines
//!    bitmask-vs-matrix mode, emits transformed sub-patterns via
//!    [`reduce_text_process_emit`], and deduplicates them into a global pattern
//!    table with attached [`PatternEntry`] metadata.
//!
//! 3. **Engine compilation** ([`ScanPlan::compile`]) — builds Aho-Corasick
//!    automata from the deduplicated patterns and wires up the value map that
//!    connects automaton hits back to rule entries.
//!
//! The final [`SimpleMatcher`] stores three immutable pieces: the process-type
//! tree for text transformation, the [`ScanPlan`] automata for pattern
//! scanning, and the [`RuleSet`] for result production.

use std::{
    borrow::Cow,
    collections::{HashMap, HashSet},
};

use foldhash::HashMapExt;

type FoldHashMap<K, V> = HashMap<K, V, foldhash::fast::FixedState>;

use super::{
    SimpleMatcher,
    error::MatcherError,
    pattern::{BITMASK_CAPACITY, PROCESS_TYPE_TABLE_SIZE, PatternEntry, PatternKind},
    rule::{Rule, RuleInfo, RuleSet, SatisfactionMethod},
    scan::ScanPlan,
    tree::build_process_type_tree,
};
use crate::process::{ProcessType, reduce_text_process_emit};

/// Fully parsed matcher construction output before scan-engine compilation.
///
/// This is the intermediate representation produced by
/// [`SimpleMatcher::parse_rules`] and consumed by [`ScanPlan::compile`]. It
/// owns the deduplicated pattern strings, their attached [`PatternEntry`]
/// metadata, and the compiled [`RuleSet`].
pub(super) struct ParsedRules<'a> {
    /// Deduplicated emitted patterns in scan order.
    ///
    /// Index `i` corresponds to `dedup_entries[i]`. Patterns may be borrowed
    /// from the input table or owned when text transformation produced a
    /// new string.
    pub(super) dedup_patterns: Vec<Cow<'a, str>>,
    /// Rule entries attached to each deduplicated pattern.
    ///
    /// `dedup_entries[i]` lists every [`PatternEntry`] that should fire when
    /// `dedup_patterns[i]` is matched by the automaton.
    pub(super) dedup_entries: Vec<Vec<PatternEntry>>,
    /// Per-rule hot and cold metadata used by the scan and result phases.
    pub(super) rules: RuleSet,
}

/// Construction helpers for turning rule tables into an executable matcher.
impl SimpleMatcher {
    /// Compiles the provided process-type rule table into a [`SimpleMatcher`].
    ///
    /// Prefer [`crate::SimpleMatcherBuilder`] at call sites; this entry point
    /// exists mainly for direct table construction and serde-driven use
    /// cases.
    ///
    /// # Construction flow
    ///
    /// 1. Build the compact process-type index table.
    /// 2. Parse all rules into deduplicated patterns and rule metadata.
    /// 3. Build the process-type transformation tree and recompute masks using
    ///    compact indices.
    /// 4. Compile Aho-Corasick automata via the scan plan.
    /// 5. Assemble and return the immutable [`SimpleMatcher`].
    ///
    /// # Errors
    ///
    /// Returns [`MatcherError`] if:
    ///
    /// - The pattern set is empty after parsing (no scannable patterns).
    /// - Any [`ProcessType`] key in `process_type_word_map` is zero
    ///   (`ProcessType::empty()`) or has bit 7 set (undefined). Use
    ///   [`ProcessType::None`] for raw-text matching.
    /// - The underlying Aho-Corasick automaton construction (`daachorse` or
    ///   `aho-corasick`) fails internally.
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
    ///     .insert(2, "foo&bar");
    ///
    /// let matcher = SimpleMatcher::new(&table).unwrap();
    /// assert!(matcher.is_match("hello world"));
    /// assert!(matcher.is_match("foo and bar"));
    /// assert!(!matcher.is_match("foo only"));
    /// ```
    pub fn new<'a, I, S1, S2>(
        process_type_word_map: &'a HashMap<ProcessType, HashMap<u32, I, S1>, S2>,
    ) -> Result<SimpleMatcher, MatcherError>
    where
        I: AsRef<str> + 'a,
    {
        for &pt in process_type_word_map.keys() {
            if pt.is_empty() || (pt.bits() as usize) >= PROCESS_TYPE_TABLE_SIZE {
                return Err(MatcherError::invalid_process_type(pt.bits()));
            }
        }

        // Merge buckets that collide after normalization (e.g. VariantNorm
        // and None|VariantNorm both normalize to VariantNorm). Later entries
        // for the same (pt, word_id) overwrite earlier ones.
        let mut normalized_process_type_word_map: HashMap<ProcessType, HashMap<u32, &I>> =
            HashMap::new();
        for (&pt, words) in process_type_word_map {
            let bucket = normalized_process_type_word_map
                .entry(pt.normalize())
                .or_default();
            for (&id, word) in words {
                bucket.insert(id, word);
            }
        }

        let process_type_set: HashSet<ProcessType> =
            normalized_process_type_word_map.keys().copied().collect();

        let process_type_index_table = Self::build_process_type_index_table(&process_type_set);

        let parsed =
            Self::parse_rules(&normalized_process_type_word_map, &process_type_index_table);

        if parsed.dedup_patterns.is_empty() {
            return Err(MatcherError::EmptyPatterns);
        }

        let process_type_tree =
            build_process_type_tree(&process_type_set, &process_type_index_table);

        let scan = ScanPlan::compile(
            &parsed.dedup_patterns,
            parsed.dedup_entries,
            parsed.rules.rule_info(),
        )?;
        let is_match_fast = process_type_tree[0].children.is_empty()
            && scan.patterns().all_single_and(parsed.rules.rule_info())
            && !scan.patterns().has_boundary();

        Ok(SimpleMatcher {
            tree: process_type_tree,
            scan,
            rules: parsed.rules,
            is_match_fast,
        })
    }

    /// Assigns each used composite process type a compact sequential index.
    ///
    /// The returned array is indexed by raw [`ProcessType::bits()`] and maps to
    /// a dense `u8` index starting at 0. [`ProcessType::None`] always gets
    /// index 0. Unused entries are set to `u8::MAX`.
    ///
    /// These compact indices are stored in [`PatternEntry::process_type_index`]
    /// and used to build the `process_type_mask` bitmask in
    /// [`ScanContext`](super::state::ScanContext).
    fn build_process_type_index_table(
        process_type_set: &HashSet<ProcessType>,
    ) -> [u8; PROCESS_TYPE_TABLE_SIZE] {
        let mut process_type_index_table = [u8::MAX; PROCESS_TYPE_TABLE_SIZE];
        let mut next_pt_idx: u8 = 0;

        process_type_index_table[ProcessType::None.bits() as usize] = next_pt_idx;
        next_pt_idx += 1;

        for &pt in process_type_set {
            let bits = pt.bits() as usize;
            if bits < PROCESS_TYPE_TABLE_SIZE && process_type_index_table[bits] == u8::MAX {
                process_type_index_table[bits] = next_pt_idx;
                next_pt_idx += 1;
            }
        }

        process_type_index_table
    }

    /// Parses the raw rule table into deduplicated emitted patterns and rule
    /// metadata.
    ///
    /// This stage handles logical operators (`&`/`~`/`|`), repeated sub-pattern
    /// counts, and the delete-adjusted emit behavior described in the
    /// design docs.
    ///
    /// # OR alternatives
    ///
    /// Each segment (delimited by `&`/`~`) may contain `|`-separated
    /// alternatives. All alternatives within a segment share the same
    /// offset and kind — any single alternative matching satisfies that
    /// segment. `|` binds tighter than `&`/`~`, so `"a|b&c|d~e|f"` means (a
    /// OR b) AND (c OR d) AND NOT (e OR f).
    ///
    /// # Delete-adjusted indexing
    ///
    /// Each rule is indexed under `process_type - ProcessType::Delete` rather
    /// than the full `ProcessType`. Delete-normalized text is what the
    /// automaton scans, so patterns must NOT themselves be
    /// Delete-transformed before indexing — they are stored verbatim
    /// and matched against the already-deleted text variants. The sub-patterns
    /// are then emitted through [`reduce_text_process_emit`] which applies
    /// only the non-Delete portion of the transformation (VariantNorm,
    /// Normalize, Romanize, etc.).
    ///
    /// # Deduplication
    ///
    /// Identical emitted pattern strings (after transformation) are assigned a
    /// single automaton slot. Multiple [`PatternEntry`] values referencing
    /// different rules are collected in the same bucket of `dedup_entries`.
    #[optimize(speed)]
    fn parse_rules<'a, I, S1, S2>(
        process_type_word_map: &'a HashMap<ProcessType, HashMap<u32, I, S1>, S2>,
        process_type_index_table: &[u8; PROCESS_TYPE_TABLE_SIZE],
    ) -> ParsedRules<'a>
    where
        I: AsRef<str> + 'a,
    {
        let rule_count_hint: usize = process_type_word_map.values().map(|map| map.len()).sum();

        let mut dedup_entries: Vec<Vec<PatternEntry>> = Vec::with_capacity(rule_count_hint);
        let mut rules: Vec<Rule> = Vec::with_capacity(rule_count_hint);
        let mut rule_infos: Vec<RuleInfo> = Vec::with_capacity(rule_count_hint);
        let mut rule_key_to_idx: FoldHashMap<(ProcessType, u32), usize> =
            FoldHashMap::with_capacity(rule_count_hint);

        let mut next_pattern_id: usize = 0;
        let mut dedup_patterns = Vec::with_capacity(rule_count_hint);
        let mut pattern_id_map: FoldHashMap<Cow<'_, str>, usize> =
            FoldHashMap::with_capacity(rule_count_hint);

        let mut and_splits: FoldHashMap<&str, i32> = FoldHashMap::new();
        let mut not_splits: FoldHashMap<&str, i32> = FoldHashMap::new();
        for (&process_type, rule_map) in process_type_word_map {
            let word_process_type = process_type - ProcessType::Delete;

            for (&rule_id, rule_str) in rule_map {
                if rule_str.as_ref().is_empty() {
                    continue;
                }

                and_splits.clear();
                not_splits.clear();

                // ── Split on &/~ operators ────────────────────
                let mut start = 0;
                let mut current_is_not = false;

                let mut count_segment = |segment: &'a str, is_not: bool| {
                    if segment.is_empty() {
                        return;
                    }
                    if is_not {
                        let entry = not_splits.entry(segment).or_insert(1);
                        *entry -= 1;
                    } else {
                        let entry = and_splits.entry(segment).or_insert(0);
                        *entry += 1;
                    }
                };

                for (index, marker) in rule_str.as_ref().match_indices(['&', '~']) {
                    count_segment(&rule_str.as_ref()[start..index], current_is_not);
                    current_is_not = marker == "~";
                    start = index + 1;
                }
                count_segment(&rule_str.as_ref()[start..], current_is_not);

                // Pure-NOT rules (no AND segments) are unsatisfiable — skip.
                if and_splits.is_empty() {
                    continue;
                }

                // ── Determine satisfaction method ────────────────
                let and_count = and_splits.len();
                let segment_counts: Vec<i32> = and_splits
                    .values()
                    .copied()
                    .chain(not_splits.values().copied())
                    .collect();

                let (method, has_not) = determine_satisfaction_method(and_count, &segment_counts);
                let info = RuleInfo {
                    and_count: and_count as u8,
                    method,
                    has_not,
                };

                // ── Upsert rule ──────────────────────────────
                let rule_idx =
                    if let Some(&existing_idx) = rule_key_to_idx.get(&(process_type, rule_id)) {
                        rules[existing_idx] = Rule {
                            segment_counts,
                            rule_id,
                            pattern: rule_str.as_ref().to_owned(),
                        };
                        rule_infos[existing_idx] = info;
                        existing_idx
                    } else {
                        let idx = rules.len();
                        rule_key_to_idx.insert((process_type, rule_id), idx);
                        rules.push(Rule {
                            segment_counts,
                            rule_id,
                            pattern: rule_str.as_ref().to_owned(),
                        });
                        rule_infos.push(info);
                        idx
                    };

                // ── Emit deduplicated patterns per segment ───
                for (offset, &segment_key) in and_splits.keys().chain(not_splits.keys()).enumerate()
                {
                    assert!(
                        offset < 256,
                        "rule has {offset} segments; PatternEntry::offset is u8 (max 255)"
                    );
                    assert!(
                        and_count < 256,
                        "rule has {and_count} AND segments; RuleInfo::and_count is u8 (max 255)"
                    );

                    let kind = if offset < and_count {
                        PatternKind::And
                    } else {
                        PatternKind::Not
                    };

                    // Split on '|' for OR alternatives within this segment.
                    // Each alternative becomes a separate AC pattern mapping to the
                    // same segment offset — any single alternative matching satisfies
                    // the segment.
                    for alternative in segment_key.split('|') {
                        if alternative.is_empty() {
                            continue;
                        }
                        // Parse \b word boundary markers at start/end of the alternative.
                        let (boundary_flags, inner) = parse_boundary_markers(alternative);
                        if inner.is_empty() {
                            continue;
                        }
                        for ac_word in reduce_text_process_emit(word_process_type, inner) {
                            let process_type_index =
                                process_type_index_table[process_type.bits() as usize];
                            let entry = PatternEntry {
                                rule_idx: rule_idx as u32,
                                offset: offset as u8,
                                process_type_index,
                                kind,
                                boundary: boundary_flags,
                            };
                            let Some(&dedup_id) = pattern_id_map.get(ac_word.as_ref()) else {
                                pattern_id_map.insert(ac_word.clone(), next_pattern_id);
                                dedup_entries.push(vec![entry]);
                                dedup_patterns.push(ac_word);
                                next_pattern_id += 1;
                                continue;
                            };
                            dedup_entries[dedup_id].push(entry);
                        }
                    }
                }
            }
        }

        ParsedRules {
            dedup_patterns,
            dedup_entries,
            rules: RuleSet::new(rules, rule_infos),
        }
    }
}

/// Word boundary flag constants.
pub(super) const BOUNDARY_LEFT: u8 = 1;
pub(super) const BOUNDARY_RIGHT: u8 = 2;

/// Parses `\b` markers at the start and end of a pattern string.
///
/// Returns `(boundary_flags, inner)` where `boundary_flags` encodes which
/// boundaries are required (bit 0 = left, bit 1 = right) and `inner` is
/// the pattern with markers stripped.
fn parse_boundary_markers(s: &str) -> (u8, &str) {
    let mut flags = 0u8;
    let mut inner = s;
    if inner.starts_with("\\b") {
        flags |= BOUNDARY_LEFT;
        inner = &inner[2..];
    }
    if inner.ends_with("\\b") {
        flags |= BOUNDARY_RIGHT;
        inner = &inner[..inner.len() - 2];
    }
    (flags, inner)
}

/// Selects the satisfaction tracking strategy based on segment shape.
///
/// `segment_counts` layout: `[and_0, ..., and_{n-1}, not_0, ...]`.
/// AND entries hold required hit counts (usually 1); NOT entries start at 0.
///
/// Returns `(method, has_not)`.
fn determine_satisfaction_method(
    and_count: usize,
    segment_counts: &[i32],
) -> (SatisfactionMethod, bool) {
    let use_matrix = and_count > BITMASK_CAPACITY
        || segment_counts.len() > BITMASK_CAPACITY
        || segment_counts[..and_count].iter().any(|&v| v != 1)
        || segment_counts[and_count..].iter().any(|&v| v != 0);
    let has_not = and_count != segment_counts.len();
    let method = match (use_matrix, and_count == 1) {
        (true, _) => SatisfactionMethod::Matrix,
        (false, true) => SatisfactionMethod::Immediate,
        (false, false) => SatisfactionMethod::Bitmask,
    };
    (method, has_not)
}

#[cfg(test)]
mod tests {
    use super::{super::pattern::PatternKind, *};
    use crate::process::ProcessType;

    fn single_rule_table(
        pt: ProcessType,
        word_id: u32,
        pattern: &str,
    ) -> HashMap<ProcessType, HashMap<u32, String>> {
        let mut table = HashMap::new();
        table
            .entry(pt)
            .or_insert_with(HashMap::new)
            .insert(word_id, pattern.to_owned());
        table
    }

    #[test]
    fn test_process_type_index_table() {
        // None always gets index 0, even when not in the key set
        let keys: HashSet<ProcessType> = [ProcessType::VariantNorm, ProcessType::Delete].into();
        let table = SimpleMatcher::build_process_type_index_table(&keys);
        assert_eq!(table[ProcessType::None.bits() as usize], 0);

        // Multiple PTs get sequential indices; unused entries are u8::MAX
        let keys: HashSet<ProcessType> = [
            ProcessType::None,
            ProcessType::VariantNorm,
            ProcessType::Delete,
        ]
        .into();
        let table = SimpleMatcher::build_process_type_index_table(&keys);
        assert_eq!(table[ProcessType::None.bits() as usize], 0);
        let fj = table[ProcessType::VariantNorm.bits() as usize];
        let del = table[ProcessType::Delete.bits() as usize];
        assert!(fj == 1 || fj == 2);
        assert!(del == 1 || del == 2);
        assert_ne!(fj, del);
        for (i, &val) in table.iter().enumerate() {
            let pt_bits = i as u8;
            if pt_bits != ProcessType::None.bits()
                && pt_bits != ProcessType::VariantNorm.bits()
                && pt_bits != ProcessType::Delete.bits()
            {
                assert_eq!(val, u8::MAX);
            }
        }
    }

    #[test]
    fn test_parse_rules_simple() {
        let table = single_rule_table(ProcessType::None, 1, "hello");
        let process_type_index_table =
            SimpleMatcher::build_process_type_index_table(&table.keys().copied().collect());
        let parsed = SimpleMatcher::parse_rules(&table, &process_type_index_table);

        assert_eq!(parsed.dedup_patterns.len(), 1);
        assert_eq!(parsed.dedup_patterns[0].as_ref(), "hello");
        assert_eq!(parsed.dedup_entries.len(), 1);
        assert_eq!(parsed.dedup_entries[0].len(), 1);
        assert_eq!(parsed.dedup_entries[0][0].kind, PatternKind::And);
    }

    #[test]
    fn test_parse_rules_operators() {
        // AND operator: "a&b" → 2 patterns, both kind=And
        let table = single_rule_table(ProcessType::None, 1, "a&b");
        let process_type_index_table =
            SimpleMatcher::build_process_type_index_table(&table.keys().copied().collect());
        let parsed = SimpleMatcher::parse_rules(&table, &process_type_index_table);
        assert_eq!(parsed.dedup_patterns.len(), 2);
        let kinds: Vec<_> = parsed
            .dedup_entries
            .iter()
            .flat_map(|bucket| bucket.iter().map(|e| e.kind))
            .collect();
        assert!(kinds.iter().all(|k| *k == PatternKind::And));
        assert_eq!(parsed.rules.len(), 1);

        // NOT operator: "a~b" → 2 patterns, 1 And + 1 Not
        let table = single_rule_table(ProcessType::None, 1, "a~b");
        let process_type_index_table =
            SimpleMatcher::build_process_type_index_table(&table.keys().copied().collect());
        let parsed = SimpleMatcher::parse_rules(&table, &process_type_index_table);
        assert_eq!(parsed.dedup_patterns.len(), 2);
        let all_entries: Vec<_> = parsed
            .dedup_entries
            .iter()
            .flat_map(|bucket| bucket.iter())
            .collect();
        let and_count = all_entries
            .iter()
            .filter(|e| e.kind == PatternKind::And)
            .count();
        let not_count = all_entries
            .iter()
            .filter(|e| e.kind == PatternKind::Not)
            .count();
        assert_eq!(and_count, 1);
        assert_eq!(not_count, 1);
    }
}
