use matcher_rs::{
    ProcessType, SimpleMatcherBuilder, reduce_text_process, reduce_text_process_emit, text_process,
};

// ===========================================================================
// Standalone transform API: text_process, reduce_text_process, etc.
// ===========================================================================

#[test]
fn test_text_process() {
    let text = text_process(ProcessType::Fanjian, "測試，臺灣");
    assert_eq!(text, "测试，台湾");
}

#[test]
fn test_delete_simd_skip_ascii_before_non_ascii() {
    // Regression: SIMD fast-skip in DeleteFindIter incorrectly advanced to the first
    // non-ASCII byte without checking for deletable ASCII bytes before it. Spaces
    // between non-deletable ASCII letters and Chinese characters were not deleted.
    let variants = reduce_text_process(ProcessType::FanjianDeleteNormalize, "A B 測試 Ａ １");
    assert_eq!(variants[0], "A B 測試 Ａ １");
    assert_eq!(variants[1], "A B 测试 Ａ １");
    assert_eq!(variants[2], "AB测试Ａ１");
    assert_eq!(variants[3], "ab测试a1");
}

#[test]
fn test_reduce_text_process() {
    let variants = reduce_text_process(ProcessType::FanjianDeleteNormalize, "！Ａ！測試！１！");

    // Step-by-step:
    // 0. Original: "！Ａ！測試！１！"
    // 1. Fanjian:  "！Ａ！测试！１！"
    // 2. Delete:   "Ａ测试１"
    // 3. Normalize:"a测试1"

    assert_eq!(variants.len(), 4);
    assert_eq!(variants[0], "！Ａ！測試！１！");
    assert_eq!(variants[1], "！Ａ！测试！１！");
    assert_eq!(variants[2], "Ａ测试１");
    assert_eq!(variants[3], "a测试1");
}

#[test]
fn test_reduce_text_process_emit() {
    let variants =
        reduce_text_process_emit(ProcessType::FanjianDeleteNormalize, "！Ａ！測試！１！");

    // emit behavior: replace-type steps overwrite; Delete appends.
    // 1. Start:    ["！Ａ！測試！１！"]
    // 2. Fanjian:  ["！Ａ！测试！１！"]  (overwritten)
    // 3. Delete:   ["！Ａ！测试！１！", "Ａ测试１"]  (pushed)
    // 4. Normalize:["！Ａ！测试！１！", "a测试1"]  (overwritten last)

    assert_eq!(variants.len(), 2);
    assert_eq!(variants[0], "！Ａ！测试！１！");
    assert_eq!(variants[1], "a测试1");
}

#[test]
fn test_reduce_text_process_all_combined() {
    let text = reduce_text_process(
        ProcessType::Fanjian
            | ProcessType::Delete
            | ProcessType::Normalize
            | ProcessType::PinYin
            | ProcessType::PinYinChar,
        "Ａ！漢語西安１",
    );

    // Final result should be fully normalized pinyin
    assert_eq!(text.last().unwrap(), "a han yu xi an1");
}

#[test]
fn test_dag_specific_outputs() {
    let processed = text_process(ProcessType::Fanjian | ProcessType::Delete, "測！試");
    assert_eq!(processed, "测试");

    let processed = text_process(ProcessType::Normalize, "ＡＢⅣ①℉");
    assert_eq!(processed, "ab41°f");
}

// ===========================================================================
// Exhaustive process-map validation
// ===========================================================================

const FANJIAN_TEST_DATA: &str = include_str!("../process_map/FANJIAN.txt");
const DELETE_TEST_DATA: &str = include_str!("../process_map/TEXT-DELETE.txt");
const NORM_TEST_DATA: &str = include_str!("../process_map/NORM.txt");
const NUM_NORM_TEST_DATA: &str = include_str!("../process_map/NUM-NORM.txt");
const PINYIN_TEST_DATA: &str = include_str!("../process_map/PINYIN.txt");

#[test]
fn test_process_map_ascii_invariants() {
    assert!(
        FANJIAN_TEST_DATA.trim().lines().all(|line| !line
            .split('\t')
            .next()
            .expect("Missing FANJIAN key")
            .is_ascii()),
        "FANJIAN.txt should not contain ASCII keys"
    );

    let mut saw_ascii_delete = false;
    for token in DELETE_TEST_DATA.trim().lines() {
        let cp = u32::from_str_radix(
            token
                .strip_prefix("U+")
                .expect("TEXT-DELETE entries must use U+XXXX format"),
            16,
        )
        .expect("TEXT-DELETE entry must contain a valid hexadecimal codepoint");
        saw_ascii_delete |= cp < 0x80;
    }
    assert!(
        saw_ascii_delete,
        "TEXT-DELETE.txt should contain ASCII delete entries"
    );

    for data in [NORM_TEST_DATA, NUM_NORM_TEST_DATA] {
        for line in data.trim().lines() {
            let mut split = line.split('\t');
            let k = split.next().expect("Missing normalize key");
            let v = split.next().expect("Missing normalize value");
            assert!(
                !k.is_ascii() || v.is_ascii(),
                "ASCII normalize key must map to ASCII output: {k:?} -> {v:?}"
            );
        }
    }

    for line in PINYIN_TEST_DATA.trim().lines() {
        let mut split = line.split('\t');
        let k = split.next().expect("Missing pinyin key");
        let v = split.next().expect("Missing pinyin value");
        assert!(!k.is_ascii(), "PINYIN.txt should not contain ASCII keys");
        assert!(v.is_ascii(), "PINYIN.txt values should stay ASCII");
    }
}

#[test]
fn test_process_map_fanjian_exhaustive() {
    for line in FANJIAN_TEST_DATA.trim().lines() {
        let mut split = line.split('\t');
        let k = split.next().expect("Missing key in FANJIAN.txt");
        let v = split.next().expect("Missing value in FANJIAN.txt");

        assert_eq!(k.chars().count(), 1, "FANJIAN key must be one char: {k:?}");
        assert_eq!(
            v.chars().count(),
            1,
            "FANJIAN value must be one char: {v:?}"
        );
        assert_eq!(
            text_process(ProcessType::Fanjian, k),
            v,
            "Fanjian failed for {}",
            k
        );
    }
}

#[test]
fn test_process_map_delete_exhaustive() {
    for token in DELETE_TEST_DATA.trim().lines() {
        let cp = u32::from_str_radix(
            token
                .strip_prefix("U+")
                .expect("TEXT-DELETE entries must use U+XXXX format"),
            16,
        )
        .expect("TEXT-DELETE entry must contain a valid hexadecimal codepoint");
        let ws = char::from_u32(cp).unwrap().to_string();
        assert_eq!(
            text_process(ProcessType::Delete, &ws),
            "",
            "Delete failed for codepoint U+{cp:04X}"
        );
    }
}

#[test]
fn test_process_map_normalize_exhaustive() {
    use std::collections::HashMap;
    let mut merged_map = HashMap::new();

    // Merging logic matches the step registry: NORM then NUM_NORM overwrites
    for data in [NORM_TEST_DATA, NUM_NORM_TEST_DATA] {
        for line in data.trim().lines() {
            let mut split = line.split('\t');
            let k = split.next().expect("Missing key");
            let v = split.next().expect("Missing value");
            if k != v {
                merged_map.insert(k, v);
            }
        }
    }

    for (k, v) in merged_map {
        assert_eq!(
            text_process(ProcessType::Normalize, k),
            v,
            "Normalize failed for {}",
            k
        );
    }
}

#[test]
fn test_process_map_pinyin_exhaustive() {
    for line in PINYIN_TEST_DATA.trim().lines() {
        let mut split = line.split('\t');
        let k = split.next().expect("Missing key in PINYIN.txt");
        let v = split.next().expect("Missing value in PINYIN.txt");
        assert_eq!(k.chars().count(), 1, "PINYIN key must be one char: {k:?}");
        assert!(!v.is_empty(), "PINYIN value must not be empty for {k:?}");

        assert_eq!(
            text_process(ProcessType::PinYin, k),
            v,
            "PinYin failed for {}",
            k
        );
    }
}

#[test]
fn test_process_map_pinyin_char_exhaustive() {
    for line in PINYIN_TEST_DATA.trim().lines() {
        let mut split = line.split('\t');
        let k = split.next().expect("Missing key in PINYIN.txt");
        let v = split.next().expect("Missing value in PINYIN.txt");
        assert_eq!(k.chars().count(), 1, "PINYIN key must be one char: {k:?}");
        assert!(!v.is_empty(), "PINYIN value must not be empty for {k:?}");

        assert_eq!(
            text_process(ProcessType::PinYinChar, k),
            v.trim(),
            "PinYinChar failed for {}",
            k
        );
    }
}

// ===========================================================================
// Individual ProcessTypes through SimpleMatcher
// ===========================================================================

#[test]
fn test_fanjian() {
    use matcher_rs::SimpleMatcher;
    use std::collections::HashMap;

    let simple_matcher = SimpleMatcher::new(&HashMap::from([(
        ProcessType::Fanjian,
        HashMap::from([(1, "测试")]),
    )]))
    .unwrap();
    assert!(
        simple_matcher.is_match("測試"),
        "Fanjian should match traditional variant of 测试"
    );

    let simple_matcher = SimpleMatcher::new(&HashMap::from([(
        ProcessType::Fanjian,
        HashMap::from([(1, "測試")]),
    )]))
    .unwrap();
    assert!(
        simple_matcher.is_match("测试"),
        "Fanjian should match simplified variant of 測試"
    );
}

#[test]
fn test_delete() {
    use matcher_rs::SimpleMatcher;
    use std::collections::HashMap;

    let simple_matcher = SimpleMatcher::new(&HashMap::from([(
        ProcessType::Delete,
        HashMap::from([(1, "你好")]),
    )]))
    .unwrap();
    assert!(
        simple_matcher.is_match("你！好"),
        "Delete should strip noise char '！'"
    );
}

#[test]
fn test_normalize() {
    use matcher_rs::SimpleMatcher;
    use std::collections::HashMap;

    let simple_matcher = SimpleMatcher::new(&HashMap::from([(
        ProcessType::Normalize,
        HashMap::from([(1, "ab41°f")]),
    )]))
    .unwrap();
    assert!(
        simple_matcher.is_match("ＡＢⅣ①℉"),
        "Normalize should map compatibility chars via NFKC + casefold"
    );
}

#[test]
fn test_normalize_leaf_applies_ascii_mappings() {
    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::Normalize, 1, "i")
        .build()
        .unwrap();

    assert!(
        matcher.is_match("I"),
        "Normalize leaf path should apply ASCII mappings like I -> i"
    );
}

#[test]
fn test_pinyin() {
    use matcher_rs::SimpleMatcher;
    use std::collections::HashMap;

    let simple_matcher = SimpleMatcher::new(&HashMap::from([(
        ProcessType::PinYin,
        HashMap::from([(1, "西安")]),
    )]))
    .unwrap();
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
fn test_pinyin_leaf_does_not_apply_ascii_digit_mappings() {
    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::PinYin, 1, "yi")
        .build()
        .unwrap();

    assert!(
        !matcher.is_match("1"),
        "PinYin should skip ASCII digits because the generated table has no ASCII keys"
    );
}

#[test]
fn test_pinyinchar() {
    use matcher_rs::SimpleMatcher;
    use std::collections::HashMap;

    let simple_matcher = SimpleMatcher::new(&HashMap::from([(
        ProcessType::PinYinChar,
        HashMap::from([(1, "西安")]),
    )]))
    .unwrap();
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
fn test_pinyinchar_leaf_does_not_apply_ascii_digit_mappings() {
    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::PinYinChar, 1, "yi")
        .build()
        .unwrap();

    assert!(
        !matcher.is_match("1"),
        "PinYinChar should skip ASCII digits because the generated table has no ASCII keys"
    );
}

// ===========================================================================
// Composite ProcessTypes through SimpleMatcher
// ===========================================================================

#[test]
fn test_cross_variant_matching() {
    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None | ProcessType::PinYin, 1, "apple&西安")
        .build()
        .unwrap();

    assert!(
        matcher.is_match("apple 洗按"),
        "Cross-variant matching should work: 'apple' (None) and '西安' (Pinyin)"
    );
}

#[test]
fn test_not_disqualification_across_variants() {
    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None | ProcessType::Delete, 1, "apple~pie")
        .build()
        .unwrap();

    assert!(
        !matcher.is_match("apple p.i.e"),
        "NOT disqualification should be global across variants"
    );
}

#[test]
fn test_complex_dag_transformations() {
    let matcher = SimpleMatcherBuilder::new()
        .add_word(
            ProcessType::Fanjian | ProcessType::Delete | ProcessType::Normalize,
            1,
            "测试",
        )
        .build()
        .unwrap();

    assert!(
        matcher.is_match("測！試"),
        "Should match with Fanjian and Delete combined"
    );
}

#[test]
fn test_complex_process_type_interactions() {
    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::Fanjian | ProcessType::PinYin, 1, "apple&西安")
        .build()
        .unwrap();

    assert!(!matcher.is_match("測 洗按"));
    assert!(matcher.is_match("apple 測 洗按"));
}

#[test]
fn test_process_type_none_with_delete() {
    // Composite None|Delete matches both raw text and delete-stripped text.
    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None | ProcessType::Delete, 1, "hello")
        .build()
        .unwrap();

    assert!(matcher.is_match("hello"), "raw match via None");
    assert!(matcher.is_match("h.e.l.l.o"), "stripped match via Delete");
    assert!(!matcher.is_match("hallo"));
}

#[test]
fn test_process_type_fanjian_pinyin() {
    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::Fanjian | ProcessType::PinYin, 1, "测试")
        .build()
        .unwrap();

    assert!(matcher.is_match("测试"), "simplified direct");
    assert!(matcher.is_match("測試"), "traditional variant via Fanjian");
    // PinYin path: different CJK chars with same pinyin syllables
    assert!(
        matcher.is_match("策士"),
        "different chars with same pinyin 'ce shi'"
    );
}

// ===========================================================================
// Unicode robustness
// ===========================================================================

#[test]
fn test_unicode_emoji_passthrough() {
    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::FanjianDeleteNormalize, 1, "test")
        .build()
        .unwrap();

    // Emoji and ZWJ sequences should not crash or corrupt matching
    let text = "test 👨\u{200D}👩\u{200D}👧\u{200D}👦 🎉";
    assert!(matcher.is_match(text));
    let results = matcher.process(text);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].word_id, 1);
}

#[test]
fn test_unicode_combining_marks() {
    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, "cafe")
        .build()
        .unwrap();

    // "cafe\u{0301}" = "café" with combining acute accent (5 codepoints, but "cafe" is a prefix)
    assert!(
        matcher.is_match("cafe\u{0301}"),
        "cafe should match as byte-prefix of cafe + combining accent"
    );
    let results = matcher.process("cafe\u{0301}");
    assert_eq!(results.len(), 1);
}

#[test]
fn test_process_into_with_transforms() {
    // process_into under Delete (currently only tested under ProcessType::None)
    let delete_matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::Delete, 1, "hello&world")
        .build()
        .unwrap();

    let mut results = Vec::new();
    delete_matcher.process_into("h.e.l.l.o w.o.r.l.d", &mut results);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].word_id, 1);

    // process_into under Fanjian
    let fanjian_matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::Fanjian, 1, "测试")
        .build()
        .unwrap();

    results.clear();
    fanjian_matcher.process_into("測試", &mut results);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].word_id, 1);
}

// ===========================================================================
// PinYin non-ASCII pattern matching
// ===========================================================================

#[test]
fn test_pinyin_non_ascii_pattern_match() {
    // End-to-end: a non-ASCII pattern registered under PinYin must still be
    // found when the PinYin-transformed text contains unmapped non-ASCII chars.
    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::PinYin, 1, "한글")
        .build()
        .unwrap();
    // Input: Chinese "你" (converted to pinyin) + Korean "한글" (passes through).
    assert!(
        matcher.is_match("你한글"),
        "non-ASCII pattern under PinYin should match when unmapped chars pass through"
    );
    let results = matcher.process("你한글");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].word_id, 1);
}

// ===========================================================================
// Unicode edge cases
// ===========================================================================

#[test]
fn test_unicode_private_use_area() {
    // U+E000..U+F8FF are Private Use Area chars — should pass through transforms unchanged
    let pua = "\u{E000}\u{E001}\u{F8FF}";
    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, pua)
        .build()
        .unwrap();
    assert!(matcher.is_match(pua));

    // Fanjian should not alter PUA chars
    let result = text_process(ProcessType::Fanjian, pua);
    assert_eq!(result.as_ref(), pua);
}

#[test]
fn test_unicode_4byte_emoji_pattern() {
    let emoji_pattern = "test\u{1F389}"; // test🎉
    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, emoji_pattern)
        .build()
        .unwrap();
    assert!(matcher.is_match("test\u{1F389}"));
    assert!(!matcher.is_match("test"));
    assert!(!matcher.is_match("\u{1F389}"));
}

#[test]
fn test_unicode_delete_strips_punctuation() {
    // Delete set includes common punctuation like periods
    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::Delete, 1, "helloworld")
        .build()
        .unwrap();
    assert!(matcher.is_match("h.e.l.l.o.w.o.r.l.d"));
    assert!(matcher.is_match("hello...world!!!"));
}

#[test]
fn test_all_process_type_bits_no_panic() {
    // Every valid ProcessType bit combination (0..63) should not panic in text_process
    let input = "Hello 你好世界 test123";
    for bits in 0u8..64 {
        let pt = ProcessType::from_bits_retain(bits);
        let _ = text_process(pt, input);
    }
}
