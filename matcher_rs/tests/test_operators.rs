use std::collections::HashMap;

use matcher_rs::{ProcessType, SimpleMatcher, SimpleMatcherBuilder};

// ---------------------------------------------------------------------------
// Basic AND / NOT
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
    )]));
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
        .build();

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
        .build();

    assert!(
        matcher.is_match("a a b"),
        "Should match two 'a's and one 'b'"
    );
    assert!(
        !matcher.is_match("a b"),
        "Should NOT match only one 'a' and one 'b'"
    );
}

// ---------------------------------------------------------------------------
// NOT semantics: ordering, veto, pure-NOT
// ---------------------------------------------------------------------------

#[test]
fn test_not_logic_ordering() {
    // NOT logic works even if the NOT token appears BEFORE the positive token.
    let mut builder = SimpleMatcherBuilder::new();
    builder = builder.add_word(ProcessType::None, 1, "hello~world");
    let matcher = builder.build();

    // "world hello" -> "world" triggers NOT, "hello" triggers positive.
    assert_eq!(matcher.process("world hello").len(), 0);
    assert_eq!(matcher.process("hello").len(), 1);
}

#[test]
fn test_not_can_veto_after_positive_completion() {
    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, "hello~world")
        .build();

    assert!(
        !matcher.is_match("hello hello world"),
        "world should still veto after hello satisfied the positive side"
    );
    assert_eq!(matcher.process("hello hello world").len(), 0);
}

#[test]
fn test_pure_not_rule_never_fires() {
    // "~bad" has 0 AND segments -> auto-positive. But AC only fires on "bad" which vetoes.
    // If "bad" is absent, the rule is never touched. Either way, no match.
    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, "~bad")
        .build();

    assert!(
        !matcher.is_match("good text"),
        "rule never touched -> no match"
    );
    assert!(
        !matcher.is_match("bad text"),
        "touched but vetoed -> no match"
    );
    assert!(!matcher.is_match(""), "empty text always false");
    assert!(matcher.process("anything").is_empty());
}

// ---------------------------------------------------------------------------
// Degenerate / edge-case patterns
// ---------------------------------------------------------------------------

#[test]
fn test_operator_only_patterns() {
    // Patterns that are pure operators produce empty segments, all skipped.
    let matcher = SimpleMatcher::new(&HashMap::from([(
        ProcessType::None,
        HashMap::from([(1, "&"), (2, "~"), (3, "&&"), (4, "~~"), (5, "&~&~")]),
    )]));

    assert!(!matcher.is_match("hello world"));
    assert!(!matcher.is_match("& ~ && ~~"));
    assert!(matcher.process("anything at all").is_empty());
}

// ---------------------------------------------------------------------------
// Large rule sets
// ---------------------------------------------------------------------------

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
    let matcher = builder.build();

    assert!(matcher.is_match("word110 word111"));
    assert!(!matcher.is_match("word110 word111 not110"));

    let results = matcher.process("word110 word111 word120 word121 not120");
    let mut ids: Vec<u32> = results.into_iter().map(|r| r.word_id).collect();
    ids.sort();
    assert_eq!(ids, vec![110]);
}

// ---------------------------------------------------------------------------
// Segment order independence
// ---------------------------------------------------------------------------

#[test]
fn test_and_not_segment_order_independence() {
    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, "x&y&z~a~b")
        .build();

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
