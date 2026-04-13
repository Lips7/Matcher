use std::collections::HashMap;

use matcher_rs::{ProcessType, SimpleMatcher, SimpleMatcherBuilder};

// ---------------------------------------------------------------------------
// AND semantics
// ---------------------------------------------------------------------------

#[test]
fn test_and_requires_all_segments() {
    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, "hello&world")
        .add_word(ProcessType::None, 2, "hello&world&hello")
        .add_word(ProcessType::None, 3, "a&a&b")
        .add_word(ProcessType::None, 4, "abc&bc&c")
        .build()
        .unwrap();

    // ID 1: both segments present
    assert!(matcher.is_match("hello world"));
    assert!(!matcher.is_match("hello"));

    // ID 2: requires 2× "hello" + 1× "world"
    assert!(matcher.is_match("hello hello world"));
    {
        // Isolated: 1× "hello" + 1× "world" is NOT enough for "hello&world&hello"
        let m2 = SimpleMatcherBuilder::new()
            .add_word(ProcessType::None, 2, "hello&world&hello")
            .build()
            .unwrap();
        assert!(!m2.is_match("hello world"));
        assert!(m2.is_match("hello hello world"));
    }

    // ID 3: count-based — needs 2×a + 1×b
    assert!(matcher.is_match("a a b"));
    assert!(!matcher.is_match("a b"));

    // ID 4: overlapping substrings — "abc" contains "abc", "bc", and "c"
    assert!(matcher.is_match("abc"));

    // High repetition: "a&a&...&a" (10×) requires 10 occurrences
    let pattern_10 = (0..10).map(|_| "a").collect::<Vec<_>>().join("&");
    let m10 = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, &pattern_10)
        .build()
        .unwrap();
    assert!(m10.is_match(&"a ".repeat(10)));
    assert!(!m10.is_match(&"a ".repeat(9)));
}

// ---------------------------------------------------------------------------
// NOT semantics
// ---------------------------------------------------------------------------

#[test]
fn test_not_vetoes_match() {
    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, "hello~world")
        .add_word(ProcessType::None, 2, "apple~pp")
        .build()
        .unwrap();

    // ID 1: positive only → match
    assert_eq!(matcher.process("hello").len(), 1);
    // NOT token before positive in text → veto (order-independent)
    assert_eq!(matcher.process("world hello").len(), 0);
    // NOT token after positive satisfaction → still vetoes
    assert!(!matcher.is_match("hello hello world"));

    // ID 2: "pp" is a substring of "apple" → self-vetoing
    assert!(!matcher.is_match("apple"));
}

#[test]
fn test_not_veto_global_across_variants() {
    // NOT firing in ANY scan variant (original or deleted) kills the rule.
    // None|Delete normalizes to Delete, which scans both original and deleted.
    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None | ProcessType::Delete, 1, "apple~pie")
        .build()
        .unwrap();

    assert!(
        !matcher.is_match("apple p.i.e"),
        "Delete-transformed text contains 'pie' → NOT veto"
    );
}

#[test]
fn test_pure_not_rules_dropped() {
    // Pure-NOT rules (no AND segments) can never fire — silently skipped.
    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, "hello")
        .add_word(ProcessType::None, 2, "~bad")
        .build()
        .unwrap();

    assert!(matcher.is_match("hello world"));
    assert!(!matcher.is_match("good text"));
    assert!(!matcher.is_match("bad text"));
    assert_eq!(matcher.process("hello").len(), 1);
}

// ---------------------------------------------------------------------------
// Combined AND/NOT
// ---------------------------------------------------------------------------

#[test]
fn test_and_not_combined() {
    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, "x&y&z~a~b")
        .add_word(ProcessType::None, 2, "a&b~c&d")
        .build()
        .unwrap();

    // ID 1: all permutations of {x,y,z} should match
    for text in ["x y z", "x z y", "y x z", "y z x", "z x y", "z y x"] {
        assert!(matcher.is_match(text), "should match permutation '{text}'");
    }
    assert!(!matcher.is_match("x y z a"), "NOT 'a' vetoes");
    assert!(!matcher.is_match("x y z b"), "NOT 'b' vetoes");

    // ID 2: "a&b~c&d"
    assert!(matcher.is_match("a b d"));
    assert!(!matcher.is_match("a b c d"), "NOT 'c' vetoes");
}

// ---------------------------------------------------------------------------
// OR semantics
// ---------------------------------------------------------------------------

#[test]
fn test_or_alternatives() {
    // Basic alternatives
    let m1 = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, "color|colour")
        .add_word(ProcessType::None, 2, "a|b|c|d")
        .build()
        .unwrap();

    assert!(m1.is_match("color"));
    assert!(m1.is_match("colour"));
    assert!(!m1.is_match("xyz"), "no alternative present");
    for ch in ["a", "b", "c", "d"] {
        assert!(m1.is_match(ch));
    }
    assert!(!m1.is_match("e"));

    // Edge cases: empty alternatives stripped
    let m2 = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, "a|")    // effectively "a"
        .add_word(ProcessType::None, 2, "|b")    // effectively "b"
        .add_word(ProcessType::None, 3, "c||d")  // effectively "c|d"
        .build()
        .unwrap();

    assert!(m2.is_match("a"));
    assert!(m2.is_match("b"));
    assert!(m2.is_match("c"));
    assert!(m2.is_match("d"));

    // Pipe-only patterns produce no rules, but other rules still work
    let m3 = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, "|")
        .add_word(ProcessType::None, 2, "||")
        .add_word(ProcessType::None, 3, "hello")
        .build()
        .unwrap();

    assert!(m3.is_match("hello"));
    assert!(!m3.is_match("anything else"));

    // Redundant alternative: "a|a" same as "a"
    let m4 = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, "a|a")
        .build()
        .unwrap();

    assert!(m4.is_match("a"));
    assert!(!m4.is_match("b"));
}

#[test]
fn test_or_with_and_and_not() {
    let matcher = SimpleMatcherBuilder::new()
        // (color OR colour) AND bright
        .add_word(ProcessType::None, 1, "color|colour&bright")
        // hello AND NOT (bad OR evil)
        .add_word(ProcessType::None, 2, "hello~bad|evil")
        // (a OR b) AND (c OR d) AND NOT (e OR f)
        .add_word(ProcessType::None, 3, "a|b&c|d~e|f")
        .build()
        .unwrap();

    // ID 1
    assert!(matcher.is_match("bright color"));
    assert!(matcher.is_match("bright colour"));
    assert!(!matcher.is_match("bright"));
    assert!(!matcher.is_match("color"));

    // ID 2
    assert!(matcher.is_match("hello"));
    assert!(!matcher.is_match("hello bad"));
    assert!(!matcher.is_match("hello evil"));

    // ID 3
    assert!(matcher.is_match("a c"));
    assert!(matcher.is_match("b d"));
    assert!(!matcher.is_match("a"));
    assert!(!matcher.is_match("a c e"), "NOT e vetoes");
    assert!(!matcher.is_match("b d f"), "NOT f vetoes");
}

#[test]
fn test_or_result_preserves_original_word() {
    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 42, "color|colour")
        .build()
        .unwrap();

    let results = matcher.process("colour is nice");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].word_id, 42);
    assert_eq!(results[0].word.as_ref(), "color|colour");
}

#[test]
fn test_or_shared_across_rules() {
    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, "cat|dog")
        .add_word(ProcessType::None, 2, "dog|bird")
        .build()
        .unwrap();

    assert_eq!(matcher.process("dog").len(), 2, "dog matches both rules");
    let r1 = matcher.process("cat");
    assert_eq!(r1.len(), 1);
    assert_eq!(r1[0].word_id, 1);
    let r2 = matcher.process("bird");
    assert_eq!(r2.len(), 1);
    assert_eq!(r2[0].word_id, 2);
}

// ---------------------------------------------------------------------------
// Word boundary (\b) semantics
// ---------------------------------------------------------------------------

#[test]
fn test_boundary_enforcement() {
    let both = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, r"\bcat\b")
        .build()
        .unwrap();

    // Both boundaries
    let cases_both: &[(&str, bool)] = &[
        ("the cat sat", true),
        ("cat", true),
        ("cat ", true),
        (" cat", true),
        ("cat!", true),
        (",cat,", true),
        ("(cat)", true),
        ("cat.dog", true),
        ("concatenate", false),
        ("scat", false),
        ("cats", false),
        ("_cat_", false), // underscore is word char
        ("my_cat_name", false),
    ];
    for &(text, expected) in cases_both {
        assert_eq!(both.is_match(text), expected, r"\bcat\b on {text:?}");
    }

    // Left boundary only
    let left = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, r"\bcat")
        .build()
        .unwrap();

    assert!(left.is_match("cat"));
    assert!(left.is_match("cats"));
    assert!(left.is_match("catch"));
    assert!(!left.is_match("scat"));
    assert!(!left.is_match("concatenate"));

    // Right boundary only
    let right = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, r"cat\b")
        .build()
        .unwrap();

    assert!(right.is_match("cat"));
    assert!(right.is_match("scat"));
    assert!(!right.is_match("cats"));
    assert!(!right.is_match("catch"));
}

#[test]
fn test_boundary_with_operators() {
    let matcher = SimpleMatcherBuilder::new()
        // AND + boundary
        .add_word(ProcessType::None, 1, r"\bcat\b&\bdog\b")
        // NOT + boundary
        .add_word(ProcessType::None, 2, r"\bcat\b~\bcatch\b")
        // OR + boundary
        .add_word(ProcessType::None, 3, r"\bcolor\b|\bcolour\b")
        .build()
        .unwrap();

    // ID 1: AND
    assert!(matcher.is_match("cat and dog"));
    assert!(!matcher.is_match("cats and dogs"));

    // ID 2: NOT
    assert!(matcher.is_match("the cat"));
    assert!(matcher.is_match("the cat catches fish")); // "catches" ≠ \bcatch\b
    assert!(!matcher.is_match("the cat catch"));

    // ID 3: OR
    assert!(matcher.is_match("nice color"));
    assert!(matcher.is_match("nice colour"));
    assert!(!matcher.is_match("colorful"));
}

#[test]
fn test_boundary_mixed_with_non_boundary() {
    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, r"\bcat\b")
        .add_word(ProcessType::None, 2, "cat")
        .build()
        .unwrap();

    // "concatenate" — only rule 2 matches (no boundary)
    let r1 = matcher.process("concatenate");
    assert_eq!(r1.len(), 1);
    assert_eq!(r1[0].word_id, 2);

    // "the cat" — both rules match
    assert_eq!(matcher.process("the cat").len(), 2);
}

#[test]
fn test_boundary_with_normalize() {
    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::Normalize, 1, r"\bcat\b")
        .build()
        .unwrap();

    assert!(matcher.is_match("the CAT sat"));
    assert!(!matcher.is_match("CONCATENATE"));
}

// ---------------------------------------------------------------------------
// Edge-case patterns
// ---------------------------------------------------------------------------

#[test]
fn test_edge_case_patterns() {
    // Operator-only patterns produce no valid rules → EmptyPatterns error
    let result = SimpleMatcher::new(&HashMap::from([(
        ProcessType::None,
        HashMap::from([(1, "&"), (2, "~"), (3, "&&"), (4, "~~"), (5, "&~&~")]),
    )]));
    assert!(result.is_err(), "operator-only patterns should be rejected");

    // Trailing/leading operators produce empty segments that get stripped
    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, "hello&")   // → "hello"
        .add_word(ProcessType::None, 2, "hello~")   // → "hello"
        .add_word(ProcessType::None, 3, "&world")   // → "world"
        .add_word(ProcessType::None, 4, "~world")   // pure-NOT, skipped
        .build()
        .unwrap();

    assert!(matcher.is_match("hello"));
    assert!(matcher.is_match("world"));

    // Null byte in patterns
    let m_nul = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, "hello\0world")
        .build()
        .unwrap();

    assert!(m_nul.is_match("hello\0world"));
    assert!(!m_nul.is_match("hello world"));
    assert!(!m_nul.is_match("helloworld"));
}
