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
        .build()
        .unwrap();

    assert!(!matcher.is_match(""), "empty text always false");
    assert!(matcher.is_match("alpha beta gamma"));
    assert!(!matcher.is_match("delta"));

    let results = matcher.process("alpha beta gamma");
    assert_eq!(results.len(), 3);
}

#[test]
fn test_search_mode_general() {
    // Rules across multiple PTs -> General
    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, "hello")
        .add_word(ProcessType::VariantNorm, 2, "测试")
        .build()
        .unwrap();

    assert!(matcher.is_match("hello"));
    assert!(matcher.is_match("測試"));

    let results = matcher.process("hello 測試");
    assert_eq!(results.len(), 2);
}

// ---------------------------------------------------------------------------
// DIRECT_RULE_BIT and PatternDispatch
// ---------------------------------------------------------------------------

#[test]
fn test_direct_rule_bit_fast_path() {
    // Mixed: same sub-pattern "hello" used in both a simple rule and a compound rule.
    // This forces Entries dispatch instead of DirectRule for the shared pattern.
    let mixed = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, "hello")
        .add_word(ProcessType::None, 2, "hello&world")
        .build()
        .unwrap();

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
        .build()
        .unwrap();

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
        .build()
        .unwrap();

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
        .build()
        .unwrap();

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
        .build()
        .unwrap();

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
        .build()
        .unwrap();

    // 65 unique AND segments -> matrix fallback
    let parts_65: Vec<String> = (0..65).map(|i| format!("w{i}")).collect();
    let pattern_65 = parts_65.join("&");
    let matcher_65 = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, &pattern_65)
        .build()
        .unwrap();

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
fn test_bitmask_not_boundary() {
    // 63 AND + 1 NOT = 64 total -> bitmask path (at capacity)
    let and_parts_63: Vec<String> = (0..63).map(|i| format!("w{i}")).collect();
    let pattern_63_not = format!("{}~veto", and_parts_63.join("&"));
    let matcher_63 = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, &pattern_63_not)
        .build()
        .unwrap();

    // 64 AND + 1 NOT = 65 total -> matrix fallback
    let and_parts_64: Vec<String> = (0..64).map(|i| format!("w{i}")).collect();
    let pattern_64_not = format!("{}~veto", and_parts_64.join("&"));
    let matcher_64 = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, &pattern_64_not)
        .build()
        .unwrap();

    let text_63 = and_parts_63.join(" ");
    let text_64 = and_parts_64.join(" ");

    // Both should match without veto
    assert!(matcher_63.is_match(&text_63), "63 AND + 1 NOT: match");
    assert!(matcher_64.is_match(&text_64), "64 AND + 1 NOT: match");

    // Both should be vetoed with NOT present
    let text_63_veto = format!("{text_63} veto");
    let text_64_veto = format!("{text_64} veto");
    assert!(
        !matcher_63.is_match(&text_63_veto),
        "63 AND + 1 NOT: vetoed"
    );
    assert!(
        !matcher_64.is_match(&text_64_veto),
        "64 AND + 1 NOT: vetoed"
    );
}

#[test]
fn test_and_count_one_not_matrix() {
    // 10 unique AND segments (each count=1) -> bitmask path, NOT matrix.
    let parts: Vec<&str> = vec!["a", "b", "c", "d", "e", "f", "g", "h", "i", "j"];
    let pattern = parts.join("&");

    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, &pattern)
        .build()
        .unwrap();

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
        .build()
        .unwrap();

    assert!(matcher.is_match("ab"), "direct match");
    assert!(matcher.is_match("a*b"), "noise char stripped");
    assert!(matcher.is_match("a b"), "space stripped");
    assert!(matcher.is_match("a!!b"), "multiple noise chars stripped");
    assert!(!matcher.is_match("ac"), "no match");
}

#[test]
fn test_variant_norm_delete_pattern_indexing() {
    // VariantNorm|Delete: pattern is VariantNorm-emitted (测试), text gets both VariantNorm + Delete.
    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::VariantNorm | ProcessType::Delete, 1, "测试")
        .build()
        .unwrap();

    assert!(matcher.is_match("测试"), "simplified direct");
    assert!(matcher.is_match("測試"), "traditional -> VariantNorm path");
    assert!(
        matcher.is_match("测！试"),
        "simplified + noise -> Delete path"
    );
    assert!(
        matcher.is_match("測！試"),
        "traditional + noise -> VariantNorm + Delete"
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
        .build()
        .unwrap();

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
        .build()
        .unwrap();

    let mut ids: Vec<u32> = matcher
        .process("你好 abc")
        .into_iter()
        .map(|result| result.word_id)
        .collect();
    ids.sort_unstable();

    assert_eq!(ids, vec![1, 2]);
}

// ---------------------------------------------------------------------------
// Threaded compilation: both ASCII and non-ASCII engines
// ---------------------------------------------------------------------------

#[test]
fn test_compile_both_ascii_and_non_ascii_engines() {
    // 150 ASCII + 150 CJK patterns forces the (has_ascii=true, has_non_ascii=true)
    // branch in compile_automata, which uses thread::scope for parallel construction.
    let ascii_words: Vec<String> = (0..150u32).map(|i| format!("ascii{i:03}")).collect();
    let cjk_words: Vec<String> = (0..150u32).map(|i| format!("测试{i:03}")).collect();
    let mut builder = SimpleMatcherBuilder::new();
    for (i, word) in ascii_words.iter().enumerate() {
        builder = builder.add_word(ProcessType::None, i as u32, word);
    }
    for (i, word) in cjk_words.iter().enumerate() {
        builder = builder.add_word(ProcessType::None, i as u32 + 1000, word);
    }
    let matcher = builder.build().unwrap();

    assert!(matcher.is_match("ascii042"));
    assert!(matcher.is_match("测试099"));
    assert!(!matcher.is_match("missing"));

    let results = matcher.process("ascii000 测试000 some text");
    let mut ids: Vec<u32> = results.iter().map(|r| r.word_id).collect();
    ids.sort();
    assert!(ids.contains(&0));
    assert!(ids.contains(&1000));
}

// ---------------------------------------------------------------------------
// DFA/AC streaming iteration paths
// ---------------------------------------------------------------------------

#[test]
fn test_dfa_streaming_via_variant_norm() {
    // ASCII patterns under VariantNorm: on ASCII text the VariantNorm leaf is a no-op,
    // but the streaming codepath in BytewiseMatcher::for_each_match_value_from_iter
    // is exercised because the tree walk visits the VariantNorm leaf with an iterator.
    let words: Vec<String> = (0..100u32).map(|i| format!("word{i:03}")).collect();
    let mut builder = SimpleMatcherBuilder::new();
    for (i, word) in words.iter().enumerate() {
        builder = builder.add_word(ProcessType::VariantNorm, i as u32, word);
    }
    let matcher = builder.build().unwrap();

    assert!(matcher.is_match("word042"));
    let results = matcher.process("word000 word099");
    assert_eq!(results.len(), 2);
}

#[test]
fn test_charwise_streaming_via_variant_norm_delete() {
    // Non-ASCII patterns under VariantNorm|Delete: the charwise engine's streaming
    // iterator path is exercised when the Delete leaf emits bytes through
    // CharwiseMatcher::for_each_match_value_from_iter.
    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::VariantNorm | ProcessType::Delete, 1, "测试")
        .add_word(ProcessType::VariantNorm | ProcessType::Delete, 2, "你好")
        .build()
        .unwrap();

    assert!(matcher.is_match("測！試"), "traditional + noise");
    assert!(matcher.is_match("你！好"), "simplified + noise");
    let results = matcher.process("測！試 你！好");
    assert_eq!(results.len(), 2);
}

// ---------------------------------------------------------------------------
// Engine routing: ASCII vs charwise dispatch
// ---------------------------------------------------------------------------

#[test]
fn test_ascii_only_text_routing() {
    // Matcher with both ASCII and CJK patterns
    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, "hello")
        .add_word(ProcessType::None, 2, "你好")
        .build()
        .unwrap();

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
        .build()
        .unwrap();

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

// ---------------------------------------------------------------------------
// Density-based engine dispatch
// ---------------------------------------------------------------------------

#[test]
fn test_density_dispatch_boundary() {
    // Both ASCII and CJK patterns registered — verify matches are found
    // regardless of which side of the 0.67 density threshold the text falls on.
    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, "needle")
        .add_word(ProcessType::None, 2, "针")
        .build()
        .unwrap();

    // ~30% non-ASCII density (bytewise path): mostly ASCII with some CJK padding
    // "needle" (6 bytes ASCII) + " 针" (1+3=4 bytes) + " aaaa..." (padding)
    // Non-ASCII: 3 bytes out of ~30 -> ~10% density -> bytewise
    let low_density = format!("needle 针 {}", "a".repeat(50));
    assert!(matcher.is_match(&low_density), "low density: ASCII match");
    let results = matcher.process(&low_density);
    let ids: Vec<u32> = results.iter().map(|r| r.word_id).collect();
    assert!(ids.contains(&1), "low density: needle found");
    assert!(ids.contains(&2), "low density: 针 found");

    // ~80% non-ASCII density (charwise path): mostly CJK with the ASCII needle embedded
    // Each CJK char is 3 bytes, so 20 CJK chars = 60 non-ASCII bytes
    // "needle" = 6 ASCII bytes, total ~66 bytes, density = 60/66 ≈ 0.91
    let high_density = format!(
        "{}needle{}",
        "你好世界测试国语中文".repeat(1),
        "你好世界测试国语中文".repeat(1)
    );
    assert!(
        matcher.is_match(&high_density),
        "high density: ASCII match in CJK text"
    );
    let results = matcher.process(&high_density);
    let ids: Vec<u32> = results.iter().map(|r| r.word_id).collect();
    assert!(ids.contains(&1), "high density: needle found");
}

// ---------------------------------------------------------------------------
// Sequential matcher reuse (thread-local state isolation)
// ---------------------------------------------------------------------------

#[test]
fn test_sequential_matcher_reuse() {
    // Verify that using two different matchers sequentially on the same thread
    // does not leak state via thread-local storage.
    let matcher_a = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, "alpha")
        .add_word(ProcessType::None, 2, "beta")
        .build()
        .unwrap();

    let matcher_b = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 10, "gamma")
        .add_word(ProcessType::None, 20, "delta")
        .build()
        .unwrap();

    // Use matcher_a first
    assert!(matcher_a.is_match("alpha beta"));
    let results_a = matcher_a.process("alpha beta");
    assert_eq!(results_a.len(), 2);

    // Use matcher_b — should not be affected by matcher_a's prior state
    assert!(
        !matcher_b.is_match("alpha"),
        "matcher_b should not know alpha"
    );
    assert!(matcher_b.is_match("gamma delta"));
    let results_b = matcher_b.process("gamma delta");
    assert_eq!(results_b.len(), 2);
    let ids_b: Vec<u32> = results_b.iter().map(|r| r.word_id).collect();
    assert!(ids_b.contains(&10));
    assert!(ids_b.contains(&20));

    // Use matcher_a again — should still work correctly
    assert!(matcher_a.is_match("alpha"));
    assert!(
        !matcher_a.is_match("gamma"),
        "matcher_a should not know gamma"
    );
}
