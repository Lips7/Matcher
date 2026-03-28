use matcher_rs::{ProcessType, SimpleMatcherBuilder};

// ---------------------------------------------------------------------------
// SearchMode paths
// ---------------------------------------------------------------------------

#[test]
fn test_search_mode_all_simple() {
    // Only single-literal patterns under ProcessType::None -> AllSimple
    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, "alpha")
        .add_word(ProcessType::None, 2, "beta")
        .add_word(ProcessType::None, 3, "gamma")
        .build();

    assert!(!matcher.is_match(""), "empty text always false");
    assert!(matcher.is_match("alpha beta gamma"));
    assert!(!matcher.is_match("delta"));

    let results = matcher.process("alpha beta gamma");
    assert_eq!(results.len(), 3);
}

#[test]
fn test_search_mode_single_process_type() {
    // All rules under one non-None PT with operators -> SingleProcessType
    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::Delete, 1, "hello&world")
        .add_word(ProcessType::Delete, 2, "foo")
        .build();

    assert!(
        matcher.is_match("h.e.l.l.o w.o.r.l.d"),
        "Delete strips noise chars"
    );
    assert!(matcher.is_match("foo"));
    assert!(!matcher.is_match("hello"));
}

#[test]
fn test_search_mode_general() {
    // Rules across multiple PTs -> General
    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, "hello")
        .add_word(ProcessType::Fanjian, 2, "你好")
        .build();

    assert!(matcher.is_match("hello"));
    assert!(matcher.is_match("妳好"));

    let results = matcher.process("hello 妳好");
    assert_eq!(results.len(), 2);
}

#[test]
fn test_search_mode_equivalence() {
    // All three SearchMode paths should agree on equivalent logical matching.

    // AllSimple: single literals under None
    let all_simple = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, "alpha")
        .add_word(ProcessType::None, 2, "beta")
        .build();

    // SingleProcessType: same literals under Delete (text already clean, no noise)
    let single_pt = SimpleMatcherBuilder::new()
        .add_word(ProcessType::Delete, 1, "alpha")
        .add_word(ProcessType::Delete, 2, "beta")
        .build();

    // General: split across two PTs
    let general = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, "alpha")
        .add_word(ProcessType::Delete, 2, "beta")
        .build();

    let texts = ["alpha", "beta", "alpha beta", "gamma", ""];
    for text in texts {
        let r_simple = all_simple.is_match(text);
        let r_single = single_pt.is_match(text);
        let r_general = general.is_match(text);
        assert_eq!(
            r_simple, r_single,
            "AllSimple vs SinglePT disagree on '{text}'"
        );
        assert_eq!(
            r_simple, r_general,
            "AllSimple vs General disagree on '{text}'"
        );

        let ids_simple: Vec<u32> = all_simple
            .process(text)
            .into_iter()
            .map(|r| r.word_id)
            .collect();
        let ids_single: Vec<u32> = single_pt
            .process(text)
            .into_iter()
            .map(|r| r.word_id)
            .collect();
        let ids_general: Vec<u32> = general
            .process(text)
            .into_iter()
            .map(|r| r.word_id)
            .collect();

        // Sort before comparing since result order may differ across modes
        let mut s1 = ids_simple.clone();
        let mut s2 = ids_single.clone();
        let mut s3 = ids_general.clone();
        s1.sort();
        s2.sort();
        s3.sort();
        assert_eq!(s1, s2, "AllSimple vs SinglePT ids disagree on '{text}'");
        assert_eq!(s1, s3, "AllSimple vs General ids disagree on '{text}'");
    }
}

// ---------------------------------------------------------------------------
// DIRECT_RULE_BIT and PatternDispatch
// ---------------------------------------------------------------------------

#[test]
fn test_direct_rule_bit_fast_path() {
    // AllSimple: all rules are simple literals under ProcessType::None
    let simple = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, "hello")
        .add_word(ProcessType::None, 2, "world")
        .build();

    assert!(simple.is_match("hello world"));
    let results = simple.process("hello world");
    assert_eq!(results.len(), 2);

    // Mixed: same sub-pattern "hello" used in both a simple rule and a compound rule.
    // This forces Entries dispatch instead of DirectRule for the shared pattern.
    let mixed = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, "hello")
        .add_word(ProcessType::None, 2, "hello&world")
        .build();

    let r1 = mixed.process("hello");
    assert_eq!(r1.len(), 1);
    assert_eq!(r1[0].word_id, 1);

    let r2 = mixed.process("hello world");
    assert_eq!(r2.len(), 2);
    let mut ids: Vec<u32> = r2.iter().map(|r| r.word_id).collect();
    ids.sort();
    assert_eq!(ids, vec![1, 2]);
}

#[test]
fn test_shared_subpattern_across_rules() {
    // "hello" is a sub-pattern shared by both rules -> PatternDispatch::Entries
    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, "hello&world")
        .add_word(ProcessType::None, 2, "hello&earth")
        .build();

    let r1 = matcher.process("hello world");
    assert_eq!(r1.len(), 1);
    assert_eq!(r1[0].word_id, 1);

    let r2 = matcher.process("hello earth");
    assert_eq!(r2.len(), 1);
    assert_eq!(r2[0].word_id, 2);

    let r3 = matcher.process("hello world earth");
    assert_eq!(r3.len(), 2);
    let mut ids: Vec<u32> = r3.iter().map(|r| r.word_id).collect();
    ids.sort();
    assert_eq!(ids, vec![1, 2]);
}

// ---------------------------------------------------------------------------
// Bitmask vs matrix dispatch
// ---------------------------------------------------------------------------

#[test]
fn test_matrix_repeated_and_segments() {
    // "a&a&b&b&b" -> and_splits: {a:2, b:3} -> use_matrix because counts != 1
    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, "a&a&b&b&b")
        .build();

    assert!(matcher.is_match("a a b b b"), "2a + 3b should match");
    assert!(!matcher.is_match("a b b b"), "1a + 3b should not match");
    assert!(!matcher.is_match("a a b b"), "2a + 2b should not match");
    assert!(!matcher.is_match("a a"), "2a + 0b should not match");

    let results = matcher.process("a a b b b");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].word_id, 1);
}

#[test]
fn test_matrix_triggered_by_not_count() {
    // "a~b~b" -> not_splits: {b: -1} -> use_matrix because not_count != 0
    // Veto fires only when "b" appears twice (counter goes from -1 -> 0 -> 1 > 0)
    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, "a~b~b")
        .build();

    assert!(matcher.is_match("a"), "a without b should match");
    assert!(
        matcher.is_match("a b"),
        "a with 1 b should match (below threshold)"
    );
    assert!(
        !matcher.is_match("a b b"),
        "a with 2 b should not match (veto)"
    );
    assert!(
        !matcher.is_match("a b b b"),
        "a with 3 b should not match (veto)"
    );
}

#[test]
fn test_matrix_combined_with_not_veto() {
    // 63 unique AND segments + 2 NOT segments = 65 total > BITMASK_CAPACITY(64)
    let and_parts: Vec<String> = (0..63).map(|i| format!("a{i}")).collect();
    let pattern = format!("{}~notX~notY", and_parts.join("&"));

    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, &pattern)
        .build();

    let all_ands = and_parts.join(" ");
    assert!(
        matcher.is_match(&all_ands),
        "all ANDs without NOTs should match"
    );

    let with_not = format!("{all_ands} notX");
    assert!(!matcher.is_match(&with_not), "NOT should veto the match");
}

#[test]
fn test_bitmask_boundary_64_vs_65() {
    // 64 unique AND segments -> bitmask path (exactly at capacity)
    let parts_64: Vec<String> = (0..64).map(|i| format!("w{i}")).collect();
    let pattern_64 = parts_64.join("&");
    let matcher_64 = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, &pattern_64)
        .build();

    // 65 unique AND segments -> matrix fallback
    let parts_65: Vec<String> = (0..65).map(|i| format!("w{i}")).collect();
    let pattern_65 = parts_65.join("&");
    let matcher_65 = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, &pattern_65)
        .build();

    let text_64 = parts_64.join(" ");
    let text_65 = parts_65.join(" ");
    let text_64_missing_last = parts_64[..63].join(" ");
    let text_65_missing_last = parts_65[..64].join(" ");

    assert!(
        matcher_64.is_match(&text_64),
        "64 segments: all present -> match"
    );
    assert!(
        !matcher_64.is_match(&text_64_missing_last),
        "64 segments: one missing -> no match"
    );
    assert!(
        matcher_65.is_match(&text_65),
        "65 segments: all present -> match"
    );
    assert!(
        !matcher_65.is_match(&text_65_missing_last),
        "65 segments: one missing -> no match"
    );

    assert_eq!(matcher_64.process(&text_64).len(), 1);
    assert_eq!(matcher_65.process(&text_65).len(), 1);
}

#[test]
fn test_and_count_one_not_matrix() {
    // 10 unique AND segments (each count=1) -> bitmask path, NOT matrix.
    let parts: Vec<&str> = vec!["a", "b", "c", "d", "e", "f", "g", "h", "i", "j"];
    let pattern = parts.join("&");

    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, &pattern)
        .build();

    let text_all = parts.join(" ");
    assert!(matcher.is_match(&text_all), "all present -> match");

    // Remove each segment one at a time and verify no match
    for skip in &parts {
        let text_missing: String = parts
            .iter()
            .filter(|p| *p != skip)
            .copied()
            .collect::<Vec<_>>()
            .join(" ");
        assert!(
            !matcher.is_match(&text_missing),
            "missing '{skip}' -> no match"
        );
    }

    let results = matcher.process(&text_all);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].word, pattern);
}

// ---------------------------------------------------------------------------
// Pattern indexing under Delete
// ---------------------------------------------------------------------------

#[test]
fn test_delete_adjusted_pattern_indexing() {
    // Pattern "ab" under Delete: stored verbatim in AC, text is delete-stripped before scan.
    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::Delete, 1, "ab")
        .build();

    assert!(matcher.is_match("ab"), "direct match");
    assert!(matcher.is_match("a*b"), "noise char stripped");
    assert!(matcher.is_match("a b"), "space stripped");
    assert!(matcher.is_match("a!!b"), "multiple noise chars stripped");
    assert!(!matcher.is_match("ac"), "no match");
}

#[test]
fn test_fanjian_delete_pattern_indexing() {
    // Fanjian|Delete: pattern is Fanjian-emitted (你好), text gets both Fanjian + Delete.
    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::Fanjian | ProcessType::Delete, 1, "你好")
        .build();

    assert!(matcher.is_match("你好"), "simplified direct");
    assert!(matcher.is_match("妳好"), "traditional -> Fanjian path");
    assert!(
        matcher.is_match("你！好"),
        "simplified + noise -> Delete path"
    );
    assert!(
        matcher.is_match("妳！好"),
        "traditional + noise -> Fanjian + Delete"
    );
}

// ---------------------------------------------------------------------------
// AC automaton behavior: overlapping patterns, mixed engines
// ---------------------------------------------------------------------------

#[test]
fn test_overlapping_words() {
    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, "hello")
        .add_word(ProcessType::None, 2, "hello world")
        .add_word(ProcessType::None, 3, "world")
        .build();

    let results = matcher.process("hello world");
    let mut ids: Vec<u32> = results.into_iter().map(|r| r.word_id).collect();
    ids.sort();

    assert_eq!(ids, vec![1, 2, 3]);
}

#[test]
fn test_mixed_ascii_and_cjk_rules_on_non_ascii_text() {
    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, "abc")
        .add_word(ProcessType::None, 2, "你好")
        .build();

    let mut ids: Vec<u32> = matcher
        .process("你好 abc")
        .into_iter()
        .map(|result| result.word_id)
        .collect();
    ids.sort_unstable();

    assert_eq!(ids, vec![1, 2]);
}

// ---------------------------------------------------------------------------
// ASCII engine routing
// ---------------------------------------------------------------------------

#[test]
fn test_ascii_only_text_routing() {
    // Matcher with both ASCII and CJK patterns
    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, "hello")
        .add_word(ProcessType::None, 2, "你好")
        .build();

    // Pure ASCII text: ASCII engine handles "hello", charwise engine handles "你好"
    let r1 = matcher.process("hello world");
    assert_eq!(r1.len(), 1);
    assert_eq!(r1[0].word_id, 1);

    // Mixed text: both engines fire
    let r2 = matcher.process("hello 你好");
    let mut ids: Vec<u32> = r2.into_iter().map(|r| r.word_id).collect();
    ids.sort();
    assert_eq!(ids, vec![1, 2]);

    // CJK-only text: only charwise engine matches
    let r3 = matcher.process("你好世界");
    assert_eq!(r3.len(), 1);
    assert_eq!(r3[0].word_id, 2);
}

#[test]
fn test_ascii_engine_only_when_no_non_ascii_patterns() {
    // All patterns are ASCII -> non_ascii_matcher may be None.
    // Non-ASCII text should still find ASCII substrings.
    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, "hello")
        .add_word(ProcessType::None, 2, "world")
        .build();

    assert!(
        matcher.is_match("hello 世界"),
        "ASCII substring in non-ASCII text"
    );
    assert!(
        !matcher.is_match("你好世界"),
        "no ASCII pattern in CJK text"
    );

    let results = matcher.process("hello 世界 world");
    let mut ids: Vec<u32> = results.into_iter().map(|r| r.word_id).collect();
    ids.sort();
    assert_eq!(ids, vec![1, 2]);
}
