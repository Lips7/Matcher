use std::collections::HashMap;

use matcher_rs::{ProcessType, SimpleMatcher, SimpleMatcherBuilder};

// ---------------------------------------------------------------------------
// Construction: HashMap API
// ---------------------------------------------------------------------------

#[test]
fn test_init() {
    let _ = SimpleMatcher::new(&HashMap::from([(
        ProcessType::None,
        HashMap::from([(1, "")]),
    )]))
    .unwrap();
    let _ = SimpleMatcher::new(&HashMap::from([(
        ProcessType::None,
        HashMap::from([(1, "hello"), (2, "world")]),
    )]))
    .unwrap();
    // Boundary conditions
    let empty_map: HashMap<ProcessType, HashMap<u32, &str>> = HashMap::new();
    let empty_matcher = SimpleMatcher::new(&empty_map).unwrap();
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
fn test_duplicate_word_id_same_process_type() {
    // Same (PT, word_id) inserted twice via HashMap -> second pattern overwrites.
    // HashMap itself deduplicates, so only "banana" survives.
    let matcher = SimpleMatcher::new(&HashMap::from([(
        ProcessType::None,
        HashMap::from([(1, "banana")]),
    )]))
    .unwrap();

    assert!(
        !matcher.is_match("apple"),
        "overwritten pattern should not match"
    );
    assert!(matcher.is_match("banana"), "final pattern should match");

    let results = matcher.process("banana");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].word, "banana");
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
    let matcher = SimpleMatcherBuilder::new().build().unwrap();

    assert!(!matcher.is_match("anything"));
    assert!(!matcher.is_match(""));
    assert!(matcher.process("hello world").is_empty());
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

    // Use unique-length words to avoid substring overlaps in Aho-Corasick
    let results = matcher.process("word0 word1999");
    let mut ids: Vec<u32> = results.iter().map(|r| r.word_id).collect();
    ids.sort();
    assert!(ids.contains(&0), "word0 should match");
    assert!(ids.contains(&1999), "word1999 should match");
}

#[test]
fn test_builder_duplicate_overwrite() {
    // Second add_word with same (PT, word_id) overwrites the first.
    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, "apple")
        .add_word(ProcessType::None, 1, "banana")
        .build()
        .unwrap();

    assert!(!matcher.is_match("apple"), "overwritten");
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

    // SingleProcessType (with operator)
    let single_pt = SimpleMatcherBuilder::new()
        .add_word(ProcessType::Delete, 1, "hello&world")
        .build()
        .unwrap();

    // General (multiple PTs)
    let general = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, "hello")
        .add_word(ProcessType::Fanjian, 2, "你好")
        .build()
        .unwrap();

    for (name, m) in [
        ("AllSimple", &all_simple),
        ("SinglePT", &single_pt),
        ("General", &general),
    ] {
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
// Serde
// ---------------------------------------------------------------------------

#[test]
fn test_serde_round_trip_process_type() {
    let types = [
        ProcessType::None,
        ProcessType::Fanjian,
        ProcessType::Delete,
        ProcessType::Normalize,
        ProcessType::PinYin,
        ProcessType::PinYinChar,
        ProcessType::DeleteNormalize,
        ProcessType::FanjianDeleteNormalize,
        ProcessType::Fanjian | ProcessType::PinYin,
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
    for bits in [64u8, 128, 192, 255] {
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
    for bits in 0u8..64 {
        let json = bits.to_string();
        let result: Result<ProcessType, _> = serde_json::from_str(&json);
        assert!(
            result.is_ok(),
            "ProcessType deserialization should accept bits={bits:#04x}"
        );
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
fn test_process_result_order_stability() {
    let mut builder = SimpleMatcherBuilder::new();
    let words: Vec<String> = (0..10).map(|i| format!("pattern{i}")).collect();
    for (i, w) in words.iter().enumerate() {
        builder = builder.add_word(ProcessType::None, i as u32, w);
    }
    let matcher = builder.build().unwrap();

    let text = words.join(" ");
    let baseline: Vec<u32> = matcher
        .process(&text)
        .into_iter()
        .map(|r| r.word_id)
        .collect();
    assert!(!baseline.is_empty());

    for _ in 0..100 {
        let ids: Vec<u32> = matcher
            .process(&text)
            .into_iter()
            .map(|r| r.word_id)
            .collect();
        assert_eq!(ids, baseline, "result ordering must be stable across calls");
    }
}
