use std::collections::HashMap;

use matcher_rs::{ProcessType, SimpleMatcher, SimpleMatcherBuilder};

// ---------------------------------------------------------------------------
// AND semantics
// ---------------------------------------------------------------------------

#[test]
fn test_combination() {
    let simple_matcher = SimpleMatcher::new(&HashMap::from([(
        ProcessType::None,
        HashMap::from([
            (1, "hello&world"),
            (2, "hello&world&hello"),
            (3, "hello~world"),
            (4, "hello~world~world"),
            (5, "hello&world~word"),
            (6, "hello&world~word~word"),
        ]),
    )]))
    .unwrap();
    assert!(
        simple_matcher.is_match("hello world"),
        "hello&world should match when both present"
    );
    assert!(
        simple_matcher.is_match("hello hello world"),
        "hello&world&hello requires 2 hellos"
    );
    assert!(
        simple_matcher.is_match("hello word"),
        "hello~world should match when world absent"
    );
}

#[test]
fn test_complex_logical_operators() {
    let matcher = SimpleMatcherBuilder::new()
        // Multiple occurrences (count > 1)
        .add_word(ProcessType::None, 1, "a&a&a")
        // NOT pattern that is a substring of AND pattern
        .add_word(ProcessType::None, 2, "apple~pp")
        // Mixed AND/NOT
        .add_word(ProcessType::None, 3, "a&b~c&d")
        // Overlapping sub-patterns
        .add_word(ProcessType::None, 4, "abc&bc&c")
        .build()
        .unwrap();

    // ID 1: "a&a&a"
    assert!(matcher.is_match("a a a"), "a&a&a should match 'a a a'");
    assert!(!matcher.is_match("a a"), "a&a&a should NOT match 'a a'");

    // ID 2: "apple~pp"
    assert!(
        !matcher.is_match("apple"),
        "apple~pp should NOT match 'apple' because 'pp' is found inside 'apple'"
    );

    // ID 3: "a&b~c&d"
    assert!(matcher.is_match("a b d"), "a&b~c&d should match 'a b d'");
    assert!(
        !matcher.is_match("a b c d"),
        "a&b~c&d should NOT match 'a b c d'"
    );

    // ID 4: "abc&bc&c"
    assert!(
        matcher.is_match("abc"),
        "abc&bc&c should match 'abc' because it contains 'abc', 'bc', and 'c'"
    );
}

#[test]
fn test_count_based_and_logic() {
    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, "a&a&b")
        .build()
        .unwrap();

    assert!(
        matcher.is_match("a a b"),
        "Should match two 'a's and one 'b'"
    );
    assert!(
        !matcher.is_match("a b"),
        "Should NOT match only one 'a' and one 'b'"
    );
}

#[test]
fn test_high_repetition_and() {
    // "a&a&a&a&a&a&a&a&a&a" requires 10 occurrences of "a"
    let pattern = (0..10).map(|_| "a").collect::<Vec<_>>().join("&");
    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, &pattern)
        .build()
        .unwrap();

    let text_10 = "a ".repeat(10);
    let text_9 = "a ".repeat(9);
    assert!(matcher.is_match(&text_10));
    assert!(!matcher.is_match(&text_9));
}

// ---------------------------------------------------------------------------
// NOT semantics
// ---------------------------------------------------------------------------

#[test]
fn test_not_veto_is_order_independent() {
    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, "hello~world")
        .build()
        .unwrap();

    // Positive only -> match
    assert_eq!(matcher.process("hello").len(), 1);

    // NOT token before positive token in text -> veto
    assert_eq!(
        matcher.process("world hello").len(),
        0,
        "NOT should veto even when appearing before the positive token"
    );

    // NOT token after positive satisfaction -> veto
    assert!(
        !matcher.is_match("hello hello world"),
        "world should still veto after hello satisfied the positive side"
    );
    assert_eq!(matcher.process("hello hello world").len(), 0);
}

#[test]
fn test_pure_not_rules_skipped() {
    // Pure-NOT rules (no AND segments) can never fire because the AC automaton only
    // detects presence, not absence. Construction skips them with a warning.
    // Valid rules in the same matcher should still work.
    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, "hello")
        .add_word(ProcessType::None, 2, "~bad")
        .build()
        .unwrap();

    assert!(matcher.is_match("hello world"), "valid rule still works");
    assert!(
        !matcher.is_match("good text"),
        "pure-NOT rule skipped -> no match"
    );
    assert!(
        !matcher.is_match("bad text"),
        "pure-NOT rule skipped -> no match"
    );
    assert!(matcher.process("anything").is_empty());
    assert_eq!(matcher.process("hello").len(), 1);
}

// ---------------------------------------------------------------------------
// Combined AND/NOT
// ---------------------------------------------------------------------------

#[test]
fn test_and_not_segment_order_independence() {
    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, "x&y&z~a~b")
        .build()
        .unwrap();

    // All 6 permutations of {x, y, z} should match (no a/b present)
    let permutations = ["x y z", "x z y", "y x z", "y z x", "z x y", "z y x"];
    for text in permutations {
        assert!(matcher.is_match(text), "should match permutation '{text}'");
    }

    // Any text including "a" or "b" should NOT match
    assert!(!matcher.is_match("x y z a"), "NOT 'a' should veto");
    assert!(!matcher.is_match("x y z b"), "NOT 'b' should veto");
    assert!(
        !matcher.is_match("x y z a b"),
        "NOT 'a' and 'b' should veto"
    );
}

#[test]
fn test_large_overlapping_and_not_set() {
    let mut builder = SimpleMatcherBuilder::new();
    let mut storage = Vec::new();
    for i in 100..200 {
        storage.push(format!("word{}&word{}~not{}", i, i + 1, i));
    }
    for (i, s) in storage.iter().enumerate() {
        builder = builder.add_word(ProcessType::None, (i + 100) as u32, s);
    }
    let matcher = builder.build().unwrap();

    assert!(matcher.is_match("word110 word111"));
    assert!(!matcher.is_match("word110 word111 not110"));

    let results = matcher.process("word110 word111 word120 word121 not120");
    let mut ids: Vec<u32> = results.into_iter().map(|r| r.word_id).collect();
    ids.sort();
    assert_eq!(ids, vec![110]);
}

// ---------------------------------------------------------------------------
// OR semantics
// ---------------------------------------------------------------------------

#[test]
fn test_or_basic() {
    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, "color|colour")
        .build()
        .unwrap();

    assert!(matcher.is_match("color"), "should match first alternative");
    assert!(
        matcher.is_match("colour"),
        "should match second alternative"
    );
    assert!(
        !matcher.is_match("colr"),
        "should not match non-alternative"
    );
}

#[test]
fn test_or_multiple_alternatives() {
    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, "a|b|c|d")
        .build()
        .unwrap();

    assert!(matcher.is_match("a"));
    assert!(matcher.is_match("b"));
    assert!(matcher.is_match("c"));
    assert!(matcher.is_match("d"));
    assert!(!matcher.is_match("e"));
}

#[test]
fn test_or_with_and() {
    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, "color|colour&bright")
        .build()
        .unwrap();

    assert!(
        matcher.is_match("bright color"),
        "(color OR colour) AND bright"
    );
    assert!(
        matcher.is_match("bright colour"),
        "(color OR colour) AND bright"
    );
    assert!(
        !matcher.is_match("bright"),
        "needs at least one alternative"
    );
    assert!(!matcher.is_match("color"), "needs bright too");
}

#[test]
fn test_or_with_not() {
    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, "hello~bad|evil")
        .build()
        .unwrap();

    assert!(matcher.is_match("hello"), "no veto tokens present");
    assert!(
        !matcher.is_match("hello bad"),
        "first OR alternative vetoes"
    );
    assert!(
        !matcher.is_match("hello evil"),
        "second OR alternative vetoes"
    );
    assert!(
        !matcher.is_match("hello bad evil"),
        "both OR alternatives veto"
    );
}

#[test]
fn test_or_combined_and_not() {
    // (a OR b) AND (c OR d) AND NOT (e OR f)
    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, "a|b&c|d~e|f")
        .build()
        .unwrap();

    assert!(matcher.is_match("a c"), "(a) AND (c)");
    assert!(matcher.is_match("b d"), "(b) AND (d)");
    assert!(matcher.is_match("a d"), "(a) AND (d)");
    assert!(matcher.is_match("b c"), "(b) AND (c)");
    assert!(!matcher.is_match("a"), "missing second AND segment");
    assert!(!matcher.is_match("a c e"), "NOT e vetoes");
    assert!(!matcher.is_match("b d f"), "NOT f vetoes");
}

#[test]
fn test_or_result_returns_original_word() {
    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 42, "color|colour")
        .build()
        .unwrap();

    let results = matcher.process("colour is nice");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].word_id, 42);
    // The original rule string (with |) is preserved
    assert_eq!(results[0].word.as_ref(), "color|colour");
}

#[test]
fn test_or_redundant_alternative() {
    // "a|a" is redundant but valid — same as "a"
    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, "a|a")
        .build()
        .unwrap();

    assert!(matcher.is_match("a"));
    assert!(!matcher.is_match("b"));
    let results = matcher.process("a");
    assert_eq!(results.len(), 1);
}

#[test]
fn test_or_with_process_type() {
    let matcher = SimpleMatcher::new(&HashMap::from([(
        ProcessType::VariantNorm,
        HashMap::from([(1, "测试|世界")]),
    )]))
    .unwrap();

    assert!(matcher.is_match("测试"));
    assert!(matcher.is_match("世界"));
    // VariantNorm: Traditional 測試 normalizes to Simplified 测试
    assert!(matcher.is_match("測試"));
}

// ---------------------------------------------------------------------------
// OR edge cases
// ---------------------------------------------------------------------------

#[test]
fn test_or_empty_alternatives_skipped() {
    // Leading/trailing/double pipes produce empty alternatives that are skipped.
    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, "a|")
        .add_word(ProcessType::None, 2, "|b")
        .add_word(ProcessType::None, 3, "c||d")
        .build()
        .unwrap();

    // "a|" -> effectively "a"
    assert!(matcher.is_match("a"));
    // "|b" -> effectively "b"
    assert!(matcher.is_match("b"));
    // "c||d" -> effectively "c|d"
    assert!(matcher.is_match("c"));
    assert!(matcher.is_match("d"));
}

#[test]
fn test_or_pipe_only_pattern() {
    // "|" produces no non-empty alternatives — rule should not match anything
    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, "|")
        .add_word(ProcessType::None, 2, "||")
        .add_word(ProcessType::None, 3, "hello")
        .build()
        .unwrap();

    // Only "hello" rule should work
    assert!(matcher.is_match("hello"));
    assert!(!matcher.is_match("anything else"));
}

#[test]
fn test_or_across_rules_dedup() {
    // Two rules sharing an alternative through dedup
    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, "cat|dog")
        .add_word(ProcessType::None, 2, "dog|bird")
        .build()
        .unwrap();

    let results = matcher.process("dog");
    assert_eq!(results.len(), 2, "dog matches both rules");

    let results = matcher.process("cat");
    assert_eq!(results.len(), 1, "cat matches only rule 1");
    assert_eq!(results[0].word_id, 1);

    let results = matcher.process("bird");
    assert_eq!(results.len(), 1, "bird matches only rule 2");
    assert_eq!(results[0].word_id, 2);
}

// ---------------------------------------------------------------------------
// Edge-case patterns
// ---------------------------------------------------------------------------

#[test]
fn test_operator_only_patterns() {
    // Patterns that are pure operators produce empty segments, all skipped.
    let matcher = SimpleMatcher::new(&HashMap::from([(
        ProcessType::None,
        HashMap::from([(1, "&"), (2, "~"), (3, "&&"), (4, "~~"), (5, "&~&~")]),
    )]))
    .unwrap();

    assert!(!matcher.is_match("hello world"));
    assert!(!matcher.is_match("& ~ && ~~"));
    assert!(matcher.process("anything at all").is_empty());
}

#[test]
fn test_trailing_operator_patterns() {
    // Trailing/leading operators produce empty segments that get stripped.
    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, "hello&")
        .add_word(ProcessType::None, 2, "hello~")
        .add_word(ProcessType::None, 3, "&world")
        .add_word(ProcessType::None, 4, "~world")
        .build()
        .unwrap();

    // "hello&" -> empty trailing segment stripped -> effectively "hello"
    assert!(matcher.is_match("hello"), "trailing & stripped");
    // "&world" -> empty leading segment stripped -> effectively "world"
    assert!(matcher.is_match("world"), "leading & stripped");
    // "hello~" -> empty NOT segment stripped -> effectively "hello"
    assert!(matcher.is_match("hello foo"), "trailing ~ stripped");
    // "~world" -> pure-NOT rule, skipped
    assert!(
        !matcher.is_match("anything"),
        "leading ~ makes pure-NOT rule"
    );
}

#[test]
fn test_pattern_with_nul_byte() {
    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, "hello\0world")
        .build()
        .unwrap();

    assert!(matcher.is_match("hello\0world"));
    assert!(!matcher.is_match("hello world"));
    assert!(!matcher.is_match("helloworld"));
}
