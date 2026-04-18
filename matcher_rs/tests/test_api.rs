use std::{borrow::Cow, collections::HashMap};

use matcher_rs::{ProcessType, SimpleMatcher, SimpleMatcherBuilder, SimpleResult};

// ---------------------------------------------------------------------------
// Construction validation
// ---------------------------------------------------------------------------

#[test]
fn test_construction_validation() {
    // Empty-string-only table → EmptyPatterns error
    assert!(
        SimpleMatcher::new(&HashMap::from([(
            ProcessType::None,
            HashMap::from([(1, "")]),
        )]))
        .is_err()
    );

    // Valid construction succeeds
    let _ = SimpleMatcher::new(&HashMap::from([(
        ProcessType::None,
        HashMap::from([(1, "hello"), (2, "world")]),
    )]))
    .unwrap();

    // Empty builder → EmptyPatterns error
    assert!(SimpleMatcherBuilder::new().build().is_err());

    // Invalid ProcessType bits >= 128 → rejected
    let bad_pt = ProcessType::from_bits_retain(128);
    let result = SimpleMatcher::new(&HashMap::from([(bad_pt, HashMap::from([(1u32, "test")]))]));
    assert!(result.is_err());

    // ProcessType::empty() (raw 0) → rejected; would cause is_match/process
    // divergence
    let empty_pt = ProcessType::empty();
    let result = SimpleMatcher::new(&HashMap::from([(
        empty_pt,
        HashMap::from([(1u32, "hello")]),
    )]));
    assert!(
        result.is_err(),
        "ProcessType::empty() must be rejected at construction"
    );
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("0x00") || err_msg.contains("empty"),
        "error message should identify the empty process type: {err_msg}"
    );
}

// ---------------------------------------------------------------------------
// Builder API
// ---------------------------------------------------------------------------

#[test]
fn test_builder_api() {
    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, "hello")
        .add_word(ProcessType::None, 2, "world")
        .add_word(ProcessType::Delete, 3, "foo")
        .build()
        .unwrap();

    assert!(matcher.is_match("hello"));
    assert!(matcher.is_match("world"));
    assert!(matcher.is_match("f*o*o"), "Delete strips noise chars");
    assert!(!matcher.is_match("unrelated"));

    // Duplicate word_id overwrites: second add_word with same (PT, id) wins
    let m2 = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, "apple")
        .add_word(ProcessType::None, 1, "banana")
        .build()
        .unwrap();

    assert!(!m2.is_match("apple"), "overwritten");
    assert!(m2.is_match("banana"), "final pattern active");
}

// ---------------------------------------------------------------------------
// Empty text
// ---------------------------------------------------------------------------

#[test]
fn test_empty_text() {
    let simple = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, "hello")
        .build()
        .unwrap();

    let general = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, "hello")
        .add_word(ProcessType::VariantNorm, 2, "你好")
        .build()
        .unwrap();

    for (name, m) in [("Simple", &simple), ("General", &general)] {
        assert!(!m.is_match(""), "{name}: is_match('') should be false");
        assert!(
            m.process("").is_empty(),
            "{name}: process('') should be empty"
        );
    }
}

// ---------------------------------------------------------------------------
// process_into semantics
// ---------------------------------------------------------------------------

#[test]
fn test_process_into_semantics() {
    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, "apple")
        .add_word(ProcessType::None, 2, "banana")
        .add_word(ProcessType::None, 3, "cherry")
        .build()
        .unwrap();

    // Matches process() output
    let expected = matcher.process("apple banana");
    let mut results = Vec::new();
    matcher.process_into("apple banana", &mut results);
    assert_eq!(results.len(), expected.len());

    // Clear + reuse
    results.clear();
    matcher.process_into("cherry", &mut results);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].word_id, 3);

    // Append semantics (no clear)
    matcher.process_into("apple", &mut results);
    assert_eq!(results.len(), 2);

    // Empty text no-op
    let len = results.len();
    matcher.process_into("", &mut results);
    assert_eq!(results.len(), len);

    // General mode with sentinel — append semantics preserved
    let general = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, "hello")
        .add_word(ProcessType::VariantNorm, 2, "测试")
        .build()
        .unwrap();

    let sentinel = SimpleResult {
        word_id: 9999,
        word: Cow::Borrowed("sentinel"),
    };
    let mut buf: Vec<SimpleResult<'_>> = vec![sentinel];
    general.process_into("hello 測試", &mut buf);
    assert_eq!(buf[0].word_id, 9999, "sentinel preserved");
    assert_eq!(buf.len(), 3, "sentinel + 2 matches");
}

// ---------------------------------------------------------------------------
// for_each_match
// ---------------------------------------------------------------------------

#[test]
fn test_for_each_match_api() {
    let m = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, "hello")
        .add_word(ProcessType::None, 2, "world")
        .add_word(ProcessType::None, 3, "missing")
        .build()
        .unwrap();

    // Empty text: no callbacks fired
    let mut count = 0;
    let stopped = m.for_each_match("", |_| {
        count += 1;
        false
    });
    assert!(!stopped);
    assert_eq!(count, 0);

    // Finds all matches
    let mut ids = Vec::new();
    m.for_each_match("hello world", |r| {
        ids.push(r.word_id);
        false
    });
    ids.sort();
    assert_eq!(ids, vec![1, 2]);

    // Early exit
    let mut exit_count = 0;
    let stopped = m.for_each_match("hello world", |_| {
        exit_count += 1;
        true // stop after first
    });
    assert!(stopped);
    assert_eq!(exit_count, 1);
}

// ---------------------------------------------------------------------------
// find_match
// ---------------------------------------------------------------------------

#[test]
fn test_find_match_api() {
    let m = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 42, "needle")
        .add_word(ProcessType::None, 1, "a&b")
        .build()
        .unwrap();

    // Found
    assert_eq!(m.find_match("find the needle").unwrap().word_id, 42);

    // Not found / empty
    assert!(m.find_match("no match here").is_none());
    assert!(m.find_match("").is_none());

    // General mode (AND pattern)
    assert!(m.find_match("a b").is_some());
    assert!(m.find_match("a only").is_none());
}

// ---------------------------------------------------------------------------
// SimpleResult shape
// ---------------------------------------------------------------------------

#[test]
fn test_result_word_field() {
    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, "apple&pie")
        .add_word(ProcessType::None, 2, "hello~world")
        .add_word(ProcessType::None, 3, "a&b~c")
        .add_word(ProcessType::None, 42, r"\bcat\b")
        .build()
        .unwrap();

    // Operators preserved in word field
    assert_eq!(matcher.process("apple pie")[0].word, "apple&pie");
    assert_eq!(matcher.process("hello")[0].word, "hello~world");
    assert_eq!(matcher.process("a b")[0].word, "a&b~c");

    // Boundary markers preserved
    let r = matcher.process("the cat");
    let cat_result = r.iter().find(|r| r.word_id == 42).unwrap();
    assert_eq!(cat_result.word.as_ref(), r"\bcat\b");
}

// ---------------------------------------------------------------------------
// Cross-PT word_id
// ---------------------------------------------------------------------------

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
}

// ---------------------------------------------------------------------------
// Stress / large inputs
// ---------------------------------------------------------------------------

#[test]
fn test_large_inputs() {
    // Very long text
    let m1 = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, "needle")
        .build()
        .unwrap();
    let long = "haystack ".repeat(10000) + "needle" + &" haystack".repeat(10000);
    assert!(m1.is_match(&long));

    // Very long pattern (500+ chars)
    let pattern = "a".repeat(500);
    let m2 = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, &pattern)
        .build()
        .unwrap();
    let text = "b".repeat(499) + &pattern + &"b".repeat(499);
    assert!(m2.is_match(&text));
    assert!(!m2.is_match(&"a".repeat(499)));

    // Many words (2000)
    let mut builder = SimpleMatcherBuilder::new();
    let words: Vec<String> = (0..2000u32).map(|i| format!("word{i}")).collect();
    for (i, w) in words.iter().enumerate() {
        builder = builder.add_word(ProcessType::None, i as u32, w);
    }
    let m3 = builder.build().unwrap();
    assert!(m3.is_match("word999"));
    assert!(!m3.is_match("wordXXX"));

    let r = m3.process("word0 word1999");
    let mut ids: Vec<u32> = r.iter().map(|r| r.word_id).collect();
    ids.sort();
    assert!(ids.contains(&0));
    assert!(ids.contains(&1999));
}

// ---------------------------------------------------------------------------
// Batch API
// ---------------------------------------------------------------------------

#[test]
fn test_batch_is_match() {
    let m = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, "hello")
        .add_word(ProcessType::None, 2, "world")
        .build()
        .unwrap();

    let results = m.batch_is_match(&["hello world", "goodbye", "say hello", ""]);
    assert_eq!(results, vec![true, false, true, false]);

    // Empty slice
    assert_eq!(m.batch_is_match(&[]), Vec::<bool>::new());

    // Order is preserved
    let texts: Vec<&str> = (0..100).map(|_| "hello").collect();
    let bools = m.batch_is_match(&texts);
    assert!(bools.iter().all(|&b| b));
}

#[test]
fn test_batch_process() {
    let m = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, "apple")
        .add_word(ProcessType::None, 2, "banana")
        .build()
        .unwrap();

    let results = m.batch_process(&["apple banana", "banana", "cherry", ""]);
    assert_eq!(results.len(), 4);
    assert_eq!(results[0].len(), 2);
    assert_eq!(results[1].len(), 1);
    assert_eq!(results[1][0].word_id, 2);
    assert!(results[2].is_empty());
    assert!(results[3].is_empty());

    // Empty slice
    assert!(m.batch_process(&[]).is_empty());
}

#[test]
fn test_batch_find_match() {
    let m = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 42, "needle")
        .build()
        .unwrap();

    let results = m.batch_find_match(&["find the needle", "nothing here", ""]);
    assert_eq!(results.len(), 3);
    assert_eq!(results[0].as_ref().unwrap().word_id, 42);
    assert!(results[1].is_none());
    assert!(results[2].is_none());

    // Empty slice
    assert!(m.batch_find_match(&[]).is_empty());
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
            let de: ProcessType = serde_json::from_str(&json).unwrap();
            assert_eq!(pt, de, "round-trip failed for {pt:?}, json={json}");
        }
    }

    #[test]
    fn test_serde_rejects_invalid_process_type_bits() {
        for bits in [128u8, 192, 255] {
            let json = bits.to_string();
            let result: Result<ProcessType, _> = serde_json::from_str(&json);
            assert!(result.is_err(), "should reject bits={bits:#04x}");
        }
    }

    #[test]
    fn test_serde_accepts_all_valid_process_type_bits() {
        for bits in 0u8..128 {
            let json = bits.to_string();
            let result: Result<ProcessType, _> = serde_json::from_str(&json);
            assert!(result.is_ok(), "should accept bits={bits:#04x}");
        }
    }
}
