use std::collections::HashMap;

use matcher_rs::{ProcessType, SimpleMatcher, SimpleMatcherBuilder};

#[test]
fn test_init() {
    let _ = SimpleMatcher::new(&HashMap::from([(
        ProcessType::None,
        HashMap::from([(1, "")]),
    )]));
    let _ = SimpleMatcher::new(&HashMap::from([(
        ProcessType::None,
        HashMap::from([(1, "hello"), (2, "world")]),
    )]));
    // Boundary conditions
    let empty_map: HashMap<ProcessType, HashMap<u32, &str>> = HashMap::new();
    let empty_matcher = SimpleMatcher::new(&empty_map);
    assert!(
        !empty_matcher.is_match("test"),
        "empty matcher should never match"
    );
    assert!(
        !empty_matcher.is_match(""),
        "empty matcher should never match empty string"
    );
}

#[test]
fn test_builder() {
    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, "hello")
        .add_word(ProcessType::None, 2, "world")
        .add_word(ProcessType::Delete, 3, "foo")
        .build();

    assert!(matcher.is_match("hello"), "should match 'hello'");
    assert!(matcher.is_match("world"), "should match 'world'");
    assert!(
        matcher.is_match("f*o*o"),
        "Delete should strip noise chars, matching 'foo'"
    );
    assert!(
        !matcher.is_match("hallo warld no split match single"),
        "should not match unrelated text"
    );
}

#[test]
fn test_fanjian() {
    let simple_matcher = SimpleMatcher::new(&HashMap::from([(
        ProcessType::Fanjian,
        HashMap::from([(1, "你好")]),
    )]));
    assert!(
        simple_matcher.is_match("妳好"),
        "Fanjian should match traditional variant of 你好"
    );

    let simple_matcher = SimpleMatcher::new(&HashMap::from([(
        ProcessType::Fanjian,
        HashMap::from([(1, "妳好")]),
    )]));
    assert!(
        simple_matcher.is_match("你好"),
        "Fanjian should match simplified variant of 妳好"
    );
}

#[test]
fn test_delete() {
    let simple_matcher = SimpleMatcher::new(&HashMap::from([(
        ProcessType::Delete,
        HashMap::from([(1, "你好")]),
    )]));
    assert!(
        simple_matcher.is_match("你！好"),
        "Delete should strip noise char '！'"
    );
}

#[test]
fn test_normalize() {
    let simple_matcher = SimpleMatcher::new(&HashMap::from([(
        ProcessType::Normalize,
        HashMap::from([(1, "he11o")]),
    )]));
    assert!(
        simple_matcher.is_match("ℋЀ⒈㈠Õ"),
        "Normalize should map fancy chars to 'he11o'"
    );
}

#[test]
fn test_pinyin() {
    let simple_matcher = SimpleMatcher::new(&HashMap::from([(
        ProcessType::PinYin,
        HashMap::from([(1, "西安")]),
    )]));
    assert!(
        simple_matcher.is_match("洗按"),
        "PinYin xi an should match 洗按 (xi an)"
    );
    assert!(
        !simple_matcher.is_match("现"),
        "PinYin xi an should not match 现 (xian without space)"
    );
}

#[test]
fn test_pinyinchar() {
    let simple_matcher = SimpleMatcher::new(&HashMap::from([(
        ProcessType::PinYinChar,
        HashMap::from([(1, "西安")]),
    )]));
    assert!(
        simple_matcher.is_match("洗按"),
        "PinYinChar xi an should match 洗按"
    );
    assert!(
        simple_matcher.is_match("现"),
        "PinYinChar xi an should match 现 (xian without space)"
    );
    assert!(
        simple_matcher.is_match("xian"),
        "PinYinChar should match literal xian"
    );
}

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
fn test_not_logic_ordering() {
    // Test that NOT logic works even if the NOT token appears BEFORE the positive token.
    let mut builder = SimpleMatcherBuilder::new();
    builder = builder.add_word(ProcessType::None, 1, "hello~world");
    let matcher = builder.build();

    // "world hello" -> "world" triggers NOT, "hello" triggers positive.
    assert_eq!(matcher.process("world hello").len(), 0);
    assert_eq!(matcher.process("hello").len(), 1);
}

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

    // It should match "hello", "hello world", and "world"
    assert_eq!(ids, vec![1, 2, 3]);
}

#[test]
fn test_whitespace_handling() {
    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, " ")
        .add_word(ProcessType::None, 2, "hello ")
        .build();

    assert!(matcher.is_match(" "), "single space pattern should match");
    assert!(
        matcher.is_match("hello "),
        "should match 'hello ' with trailing space"
    );
    assert!(
        !matcher.is_match("hello"),
        "should not match 'hello' without trailing space"
    );
}

#[test]
fn test_very_long_text() {
    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, "needle")
        .build();

    let long_text = "haystack ".repeat(10000) + "needle" + &" haystack".repeat(10000);
    assert!(matcher.is_match(&long_text));
}
