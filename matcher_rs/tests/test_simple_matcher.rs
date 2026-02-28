use std::collections::HashMap;

use matcher_rs::{ProcessType, SimpleMatcher, SimpleMatcherBuilder, TextMatcherTrait};

#[test]
fn simple_match_init() {
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
    assert!(!empty_matcher.is_match("test"));
    assert!(!empty_matcher.is_match(""));
}

#[test]
fn simple_match_builder() {
    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, "hello")
        .add_word(ProcessType::None, 2, "world")
        .add_word(ProcessType::Delete, 3, "foo")
        .build();

    assert!(matcher.is_match("hello"));
    assert!(matcher.is_match("world"));
    assert!(matcher.is_match("f*o*o"));
    assert!(!matcher.is_match("hallo warld no split match single"));
}

#[test]
fn simple_match_fanjian() {
    let simple_matcher = SimpleMatcher::new(&HashMap::from([(
        ProcessType::Fanjian,
        HashMap::from([(1, "你好")]),
    )]));
    assert!(simple_matcher.is_match("妳好"));

    let simple_matcher = SimpleMatcher::new(&HashMap::from([(
        ProcessType::Fanjian,
        HashMap::from([(1, "妳好")]),
    )]));
    assert!(simple_matcher.is_match("你好"));
}

#[test]
fn simple_match_delete() {
    let simple_matcher = SimpleMatcher::new(&HashMap::from([(
        ProcessType::Delete,
        HashMap::from([(1, "你好")]),
    )]));
    assert!(simple_matcher.is_match("你！好"));
}

#[test]
fn simple_match_normalize() {
    let simple_matcher = SimpleMatcher::new(&HashMap::from([(
        ProcessType::Normalize,
        HashMap::from([(1, "he11o")]),
    )]));
    assert!(simple_matcher.is_match("ℋЀ⒈㈠Õ"));
}

#[test]
fn simple_match_pinyin() {
    let simple_matcher = SimpleMatcher::new(&HashMap::from([(
        ProcessType::PinYin,
        HashMap::from([(1, "西安")]),
    )]));
    assert!(simple_matcher.is_match("洗按"));
    assert!(!simple_matcher.is_match("现"));
}

#[test]
fn simple_match_pinyinchar() {
    let simple_matcher = SimpleMatcher::new(&HashMap::from([(
        ProcessType::PinYinChar,
        HashMap::from([(1, "西安")]),
    )]));
    assert!(simple_matcher.is_match("洗按"));
    assert!(simple_matcher.is_match("现"));
    assert!(simple_matcher.is_match("xian"));
}

#[test]
fn simple_match_combination() {
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
    assert!(simple_matcher.is_match("hello world"));
    assert!(simple_matcher.is_match("hello hello world"));
    assert!(simple_matcher.is_match("hello word"));
}

#[test]
fn simple_process_iter_matches_process() {
    let matcher = SimpleMatcher::new(&HashMap::from([(
        ProcessType::None,
        HashMap::from([(1u32, "hello"), (2u32, "world")]),
    )]));

    let text = "say hello to the world";

    let mut via_process: Vec<u32> = matcher
        .process(text)
        .into_iter()
        .map(|r| r.word_id)
        .collect();
    let mut via_iter: Vec<u32> = matcher.process_iter(text).map(|r| r.word_id).collect();

    via_process.sort();
    via_iter.sort();

    assert_eq!(
        via_process, via_iter,
        "process_iter must yield same word_ids as process"
    );
}

#[test]
fn simple_process_iter_empty() {
    let matcher = SimpleMatcher::new(&HashMap::from([(
        ProcessType::None,
        HashMap::from([(1u32, "hello")]),
    )]));

    assert_eq!(matcher.process_iter("").count(), 0);
}

#[test]
fn simple_matcher_not_logic_ordering() {
    // Test that NOT logic works even if the NOT token appears BEFORE the positive token.
    let mut builder = SimpleMatcherBuilder::new();
    builder = builder.add_word(ProcessType::None, 1, "hello~world");
    let matcher = builder.build();

    // "world hello" -> "world" triggers NOT, "hello" triggers positive.
    assert_eq!(matcher.process("world hello").len(), 0);
    assert_eq!(matcher.process("hello").len(), 1);
}

#[test]
fn simple_match_overlapping_words() {
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
fn simple_match_whitespace_handling() {
    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, " ")
        .add_word(ProcessType::None, 2, "hello ")
        .build();

    assert!(matcher.is_match(" "));
    assert!(matcher.is_match("hello "));
    assert!(!matcher.is_match("hello")); // Missing space
}

#[test]
fn simple_match_very_long_text() {
    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, "needle")
        .build();

    let long_text = "haystack ".repeat(10000) + "needle" + &" haystack".repeat(10000);
    assert!(matcher.is_match(&long_text));
}
