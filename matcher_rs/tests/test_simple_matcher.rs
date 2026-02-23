use std::collections::HashMap;

use matcher_rs::{ProcessType, SimpleMatcher, SimpleMatcherBuilder, SimpleWord, TextMatcherTrait};

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
            (1, SimpleWord::from("hello").and("world")),
            (2, SimpleWord::from("hello").and("world").and("hello")),
            (3, SimpleWord::from("hello").not("world")),
            (4, SimpleWord::from("hello").not("world").not("world")),
            (5, SimpleWord::from("hello").and("world").not("word")),
            (
                6,
                SimpleWord::from("hello")
                    .and("world")
                    .not("word")
                    .not("word"),
            ),
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
