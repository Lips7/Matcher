use matcher_rs::{ProcessType, SimpleMatcherBuilder};

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
fn test_large_number_of_splits() {
    let mut pattern = "a0".to_string();
    for i in 1..65 {
        pattern.push_str(&format!("&a{}", i));
    }

    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, &pattern)
        .build();

    let mut text = "a0".to_string();
    for i in 1..65 {
        text.push_str(&format!(" a{}", i));
    }

    assert!(matcher.is_match(&text), "Large AND pattern should match");

    let mut missing_text = "a0".to_string();
    for i in 1..64 {
        missing_text.push_str(&format!(" a{}", i));
    }
    assert!(
        !matcher.is_match(&missing_text),
        "Large AND pattern should NOT match if one part is missing"
    );
}

#[test]
fn test_cross_variant_matching() {
    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None | ProcessType::PinYin, 1, "apple&西安")
        .build();

    assert!(
        matcher.is_match("apple 洗按"),
        "Cross-variant matching should work: 'apple' (None) and '西安' (Pinyin)"
    );
}

#[test]
fn test_not_disqualification_across_variants() {
    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None | ProcessType::Delete, 1, "apple~pie")
        .build();

    assert!(
        !matcher.is_match("apple p.i.e"),
        "NOT disqualification should be global across variants"
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

#[test]
fn test_complex_dag_transformations() {
    let matcher = SimpleMatcherBuilder::new()
        .add_word(
            ProcessType::Fanjian | ProcessType::Delete | ProcessType::Normalize,
            1,
            "你好",
        )
        .build();

    assert!(
        matcher.is_match("妳！好"),
        "Should match with Fanjian and Delete combined"
    );
}

#[test]
fn test_same_word_id_different_process_types() {
    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, "apple")
        .add_word(ProcessType::Delete, 1, "banana")
        .build();

    assert!(matcher.is_match("apple"));
    assert!(matcher.is_match("b.a.n.a.n.a"));

    let results = matcher.process("apple b.a.n.a.n.a");
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].word_id, 1);
    assert_eq!(results[1].word_id, 1);
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
    let matcher = builder.build();

    assert!(matcher.is_match("word110 word111"));
    assert!(!matcher.is_match("word110 word111 not110"));

    let results = matcher.process("word110 word111 word120 word121 not120");
    let mut ids: Vec<u32> = results.into_iter().map(|r| r.word_id).collect();
    ids.sort();
    assert_eq!(ids, vec![110]);
}

#[test]
fn test_not_with_multiple_occurrences() {
    let matcher = SimpleMatcherBuilder::new()
        // Must have "a" twice, and NOT have "b" twice
        .add_word(ProcessType::None, 1, "a&a~b~b")
        .build();

    assert!(
        matcher.is_match("a a b"),
        "a&a~b~b should match if 'b' only occurs once"
    );
    assert!(
        !matcher.is_match("a a b b"),
        "a&a~b~b should NOT match if 'b' occurs twice"
    );
}

#[test]
fn test_complex_process_type_interactions() {
    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::Fanjian | ProcessType::PinYin, 1, "apple&西安")
        .build();

    assert!(!matcher.is_match("妳 洗按"));
    assert!(matcher.is_match("apple 妳 洗按"));
}
