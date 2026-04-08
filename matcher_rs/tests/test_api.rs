use std::{borrow::Cow, collections::HashMap};

use matcher_rs::{ProcessType, SimpleMatcher, SimpleMatcherBuilder, SimpleResult};

// ---------------------------------------------------------------------------
// Construction: HashMap API
// ---------------------------------------------------------------------------

#[test]
fn test_init() {
    assert!(
        SimpleMatcher::new(&HashMap::from([(
            ProcessType::None,
            HashMap::from([(1, "")]),
        )]))
        .is_err(),
        "empty-string-only table should return EmptyPatterns error"
    );
    let _ = SimpleMatcher::new(&HashMap::from([(
        ProcessType::None,
        HashMap::from([(1, "hello"), (2, "world")]),
    )]))
    .unwrap();
}

// ---------------------------------------------------------------------------
// Construction: Builder API
// ---------------------------------------------------------------------------

#[test]
fn test_builder() {
    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, "hello")
        .add_word(ProcessType::None, 2, "world")
        .add_word(ProcessType::Delete, 3, "foo")
        .build()
        .unwrap();

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
fn test_builder_zero_words() {
    let result = SimpleMatcherBuilder::new().build();
    assert!(
        result.is_err(),
        "empty builder should return EmptyPatterns error"
    );
}

#[test]
fn test_builder_many_words() {
    let mut builder = SimpleMatcherBuilder::new();
    let mut storage = Vec::new();
    for i in 0..2000u32 {
        storage.push(format!("word{i}"));
    }
    for (i, word) in storage.iter().enumerate() {
        builder = builder.add_word(ProcessType::None, i as u32, word);
    }
    let matcher = builder.build().unwrap();

    assert!(matcher.is_match("word999"), "specific word matches");
    assert!(!matcher.is_match("wordXXX"), "absent word doesn't match");

    let results = matcher.process("word0 word1999");
    let mut ids: Vec<u32> = results.iter().map(|r| r.word_id).collect();
    ids.sort();
    assert!(ids.contains(&0), "word0 should match");
    assert!(ids.contains(&1999), "word1999 should match");
}

#[test]
fn test_duplicate_word_id_overwrite() {
    // Second add_word with same (PT, word_id) overwrites the first.
    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, "apple")
        .add_word(ProcessType::None, 1, "banana")
        .build()
        .unwrap();

    assert!(!matcher.is_match("apple"), "overwritten pattern gone");
    assert!(matcher.is_match("banana"), "final pattern active");
    assert_eq!(matcher.process("banana")[0].word, "banana");
}

// ---------------------------------------------------------------------------
// Matching contracts: is_match, process, process_into
// ---------------------------------------------------------------------------

#[test]
fn test_empty_text_matching() {
    // AllSimple
    let all_simple = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, "hello")
        .build()
        .unwrap();

    // General
    let general = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, "hello")
        .add_word(ProcessType::VariantNorm, 2, "你好")
        .build()
        .unwrap();

    for (name, m) in [("AllSimple", &all_simple), ("General", &general)] {
        assert!(!m.is_match(""), "{name}: is_match('') should be false");
        assert!(
            m.process("").is_empty(),
            "{name}: process('') should be empty"
        );
    }
}

#[test]
fn test_process_into_reuse() {
    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, "apple")
        .add_word(ProcessType::None, 2, "banana")
        .add_word(ProcessType::None, 3, "cherry")
        .build()
        .unwrap();

    // process_into on empty vec matches process
    let expected = matcher.process("apple banana");
    let mut results = Vec::new();
    matcher.process_into("apple banana", &mut results);
    assert_eq!(results.len(), expected.len());
    let ids_expected: Vec<u32> = expected.iter().map(|r| r.word_id).collect();
    let ids_actual: Vec<u32> = results.iter().map(|r| r.word_id).collect();
    assert_eq!(ids_actual, ids_expected);

    // clear + reuse with different text
    results.clear();
    matcher.process_into("cherry", &mut results);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].word_id, 3);

    // append semantics (no clear)
    matcher.process_into("apple", &mut results);
    assert_eq!(results.len(), 2);

    // empty text does not modify buffer
    let len_before = results.len();
    matcher.process_into("", &mut results);
    assert_eq!(results.len(), len_before);
}

#[test]
fn test_process_into_append_general_mode() {
    // General mode (mixed PTs) with pre-seeded buffer — verify append semantics.
    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, "hello")
        .add_word(ProcessType::VariantNorm, 2, "测试")
        .build()
        .unwrap();

    let sentinel = SimpleResult {
        word_id: 9999,
        word: Cow::Borrowed("sentinel"),
    };
    let mut results: Vec<SimpleResult<'_>> = vec![sentinel];
    matcher.process_into("hello 測試", &mut results);

    assert_eq!(results[0].word_id, 9999, "sentinel preserved");
    assert_eq!(results[0].word, "sentinel");
    assert_eq!(results.len(), 3, "sentinel + 2 matches");
}

// ---------------------------------------------------------------------------
// SimpleResult shape and content
// ---------------------------------------------------------------------------

#[test]
fn test_result_word_field_correctness() {
    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, "apple&pie")
        .add_word(ProcessType::None, 2, "hello~world")
        .add_word(ProcessType::None, 3, "a&b~c")
        .build()
        .unwrap();

    // "apple&pie" match
    let r1 = matcher.process("apple pie");
    assert_eq!(r1.len(), 1);
    assert_eq!(
        r1[0].word, "apple&pie",
        "word field should be the full original pattern"
    );

    // "hello~world" match (text has hello but not world)
    let r2 = matcher.process("hello");
    assert_eq!(r2.len(), 1);
    assert_eq!(r2[0].word, "hello~world");

    // "a&b~c" match (text has a and b but not c)
    let r3 = matcher.process("a b");
    assert_eq!(r3.len(), 1);
    assert_eq!(r3[0].word, "a&b~c");
}

#[test]
fn test_same_word_id_different_process_types() {
    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, "apple")
        .add_word(ProcessType::Delete, 1, "banana")
        .build()
        .unwrap();

    assert!(matcher.is_match("apple"));
    assert!(matcher.is_match("b.a.n.a.n.a"));

    let results = matcher.process("apple b.a.n.a.n.a");
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].word_id, 1);
    assert_eq!(results[1].word_id, 1);
}

// ---------------------------------------------------------------------------
// Serde (requires `serde` feature)
// ---------------------------------------------------------------------------

#[cfg(feature = "serde")]
mod serde_tests {
    use matcher_rs::ProcessType;

    #[test]
    fn test_serde_round_trip_process_type() {
        let types = [
            ProcessType::None,
            ProcessType::VariantNorm,
            ProcessType::Delete,
            ProcessType::Normalize,
            ProcessType::Romanize,
            ProcessType::RomanizeChar,
            ProcessType::DeleteNormalize,
            ProcessType::VariantNormDeleteNormalize,
            ProcessType::VariantNorm | ProcessType::Romanize,
        ];

        for pt in types {
            let json = serde_json::to_string(&pt).unwrap();
            let deserialized: ProcessType = serde_json::from_str(&json).unwrap();
            assert_eq!(
                pt, deserialized,
                "ProcessType {pt:?} did not survive round-trip: json={json}"
            );
        }
    }

    #[test]
    fn test_serde_rejects_invalid_process_type_bits() {
        for bits in [128u8, 192, 255] {
            let json = bits.to_string();
            let result: Result<ProcessType, _> = serde_json::from_str(&json);
            assert!(
                result.is_err(),
                "ProcessType deserialization should reject bits={bits:#04x}"
            );
        }
    }

    #[test]
    fn test_serde_accepts_all_valid_process_type_bits() {
        for bits in 0u8..128 {
            let json = bits.to_string();
            let result: Result<ProcessType, _> = serde_json::from_str(&json);
            assert!(
                result.is_ok(),
                "ProcessType deserialization should accept bits={bits:#04x}"
            );
        }
    }
}

#[test]
fn test_invalid_process_type_in_construction() {
    let bad_pt = ProcessType::from_bits_retain(128);
    let mut table = HashMap::new();
    let mut words = HashMap::new();
    words.insert(1u32, "test");
    table.insert(bad_pt, words);
    let result = SimpleMatcher::new(&table);
    assert!(
        result.is_err(),
        "construction should reject ProcessType with bits >= 64"
    );
}

// ---------------------------------------------------------------------------
// Debug + Display formatting
// ---------------------------------------------------------------------------

#[test]
fn test_debug_format() {
    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, "hello")
        .add_word(ProcessType::None, 2, "world")
        .build()
        .unwrap();

    let debug = format!("{matcher:?}");
    assert!(debug.contains("SimpleMatcher"), "debug output: {debug}");
    assert!(debug.contains("rule_count"), "debug output: {debug}");
}

// ---------------------------------------------------------------------------
// Stress and edge cases
// ---------------------------------------------------------------------------

#[test]
fn test_whitespace_handling() {
    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, " ")
        .add_word(ProcessType::None, 2, "hello ")
        .build()
        .unwrap();

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
        .build()
        .unwrap();

    let long_text = "haystack ".repeat(10000) + "needle" + &" haystack".repeat(10000);
    assert!(matcher.is_match(&long_text));
}

#[test]
fn test_very_long_pattern() {
    // 500+ char pattern — verify construction and scanning don't blow up.
    let pattern = "a".repeat(500);
    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, &pattern)
        .build()
        .unwrap();

    let text = "b".repeat(499) + &pattern + &"b".repeat(499);
    assert!(matcher.is_match(&text), "long pattern should match");
    assert!(
        !matcher.is_match(&"a".repeat(499)),
        "one char short should not match"
    );

    let results = matcher.process(&text);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].word_id, 1);
}

// ---------------------------------------------------------------------------
// for_each_match
// ---------------------------------------------------------------------------

#[test]
fn test_for_each_match_empty_text() {
    let m = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, "hello")
        .build()
        .unwrap();
    let mut count = 0;
    let stopped = m.for_each_match("", |_| {
        count += 1;
        false
    });
    assert!(!stopped);
    assert_eq!(count, 0);
}

#[test]
fn test_for_each_match_all_simple() {
    let m = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, "hello")
        .add_word(ProcessType::None, 2, "world")
        .add_word(ProcessType::None, 3, "missing")
        .build()
        .unwrap();

    let mut ids = Vec::new();
    m.for_each_match("hello world", |r| {
        ids.push(r.word_id);
        false
    });
    ids.sort();
    assert_eq!(ids, vec![1, 2]);
}

#[test]
fn test_for_each_match_early_exit() {
    let m = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, "a")
        .add_word(ProcessType::None, 2, "b")
        .add_word(ProcessType::None, 3, "c")
        .build()
        .unwrap();

    let mut count = 0;
    let stopped = m.for_each_match("a b c", |_| {
        count += 1;
        true // stop after first
    });
    assert!(stopped);
    assert_eq!(count, 1);
}

#[test]
fn test_for_each_match_general_mode() {
    let m = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, "hello&world")
        .add_word(ProcessType::None, 2, "foo~bar")
        .build()
        .unwrap();

    let mut ids = Vec::new();
    m.for_each_match("hello world foo", |r| {
        ids.push(r.word_id);
        false
    });
    ids.sort();
    assert_eq!(ids, vec![1, 2]);
}

#[test]
fn test_for_each_match_general_not_veto() {
    let m = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, "hello~world")
        .build()
        .unwrap();

    let mut ids = Vec::new();
    m.for_each_match("hello world", |r| {
        ids.push(r.word_id);
        false
    });
    assert!(ids.is_empty(), "NOT veto should prevent match");
}

// ---------------------------------------------------------------------------
// find_match
// ---------------------------------------------------------------------------

#[test]
fn test_find_match_found() {
    let m = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 42, "needle")
        .build()
        .unwrap();

    let r = m.find_match("find the needle");
    assert_eq!(r.unwrap().word_id, 42);
}

#[test]
fn test_find_match_none() {
    let m = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, "needle")
        .build()
        .unwrap();

    assert!(m.find_match("no match here").is_none());
    assert!(m.find_match("").is_none());
}

#[test]
fn test_find_match_general() {
    let m = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, "a&b")
        .build()
        .unwrap();

    assert!(m.find_match("a b").is_some());
    assert!(m.find_match("a only").is_none());
}

// ---------------------------------------------------------------------------
// process_iter
// ---------------------------------------------------------------------------

#[test]
fn test_process_iter_empty_text() {
    let m = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, "hello")
        .build()
        .unwrap();

    let iter = m.process_iter("");
    assert_eq!(iter.len(), 0);
    assert_eq!(iter.collect::<Vec<_>>(), Vec::<SimpleResult>::new());
}

#[test]
fn test_process_iter_equivalence_with_process() {
    let m = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, "hello")
        .add_word(ProcessType::None, 2, "world")
        .add_word(ProcessType::None, 3, "missing")
        .build()
        .unwrap();

    let text = "hello world";
    let from_process = m.process(text);
    let from_iter: Vec<_> = m.process_iter(text).collect();

    let mut ids_process: Vec<u32> = from_process.iter().map(|r| r.word_id).collect();
    let mut ids_iter: Vec<u32> = from_iter.iter().map(|r| r.word_id).collect();
    ids_process.sort();
    ids_iter.sort();
    assert_eq!(ids_process, ids_iter);
}

#[test]
fn test_process_iter_general_equivalence() {
    let m = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, "a&b")
        .add_word(ProcessType::None, 2, "c~d")
        .add_word(ProcessType::None, 3, "e")
        .build()
        .unwrap();

    let text = "a b c e";
    let mut ids_process: Vec<u32> = m.process(text).iter().map(|r| r.word_id).collect();
    let mut ids_iter: Vec<u32> = m.process_iter(text).map(|r| r.word_id).collect();
    ids_process.sort();
    ids_iter.sort();
    assert_eq!(ids_process, ids_iter);
}

#[test]
fn test_process_iter_exact_size() {
    let m = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, "a")
        .add_word(ProcessType::None, 2, "b")
        .add_word(ProcessType::None, 3, "c")
        .build()
        .unwrap();

    let mut iter = m.process_iter("a b c");
    assert_eq!(iter.len(), 3);
    iter.next();
    assert_eq!(iter.len(), 2);
    iter.next();
    assert_eq!(iter.len(), 1);
    iter.next();
    assert_eq!(iter.len(), 0);
    assert!(iter.next().is_none());
    assert_eq!(iter.len(), 0);
}

#[test]
fn test_process_iter_double_ended() {
    let m = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, "a")
        .add_word(ProcessType::None, 2, "b")
        .add_word(ProcessType::None, 3, "c")
        .build()
        .unwrap();

    let mut iter = m.process_iter("a b c");
    let first = iter.next().unwrap();
    let last = iter.next_back().unwrap();
    assert_ne!(first.word_id, last.word_id);
    assert_eq!(iter.len(), 1);
}

#[test]
fn test_process_iter_take() {
    let m = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, "a")
        .add_word(ProcessType::None, 2, "b")
        .add_word(ProcessType::None, 3, "c")
        .build()
        .unwrap();

    let first_two: Vec<_> = m.process_iter("a b c").take(2).collect();
    assert_eq!(first_two.len(), 2);
}

#[test]
fn test_process_iter_no_match() {
    let m = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, "hello")
        .build()
        .unwrap();

    let iter = m.process_iter("goodbye");
    assert_eq!(iter.len(), 0);
    assert_eq!(iter.collect::<Vec<_>>().len(), 0);
}

#[test]
fn test_process_iter_debug() {
    let m = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, "hello")
        .build()
        .unwrap();

    let iter = m.process_iter("hello");
    let debug = format!("{iter:?}");
    assert!(debug.contains("SimpleMatchIter"));
    assert!(debug.contains("remaining"));
}

#[test]
fn test_process_iter_with_transforms() {
    let m = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, "hello")
        .add_word(ProcessType::VariantNorm, 2, "你好")
        .build()
        .unwrap();

    let mut ids: Vec<u32> = m
        .process_iter("hello 測試 你好")
        .map(|r| r.word_id)
        .collect();
    ids.sort();

    let mut expected: Vec<u32> = m
        .process("hello 測試 你好")
        .iter()
        .map(|r| r.word_id)
        .collect();
    expected.sort();

    assert_eq!(ids, expected);
}
