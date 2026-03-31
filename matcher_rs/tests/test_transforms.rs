use matcher_rs::{
    ProcessType, SimpleMatcherBuilder, reduce_text_process, reduce_text_process_emit, text_process,
};

// ===========================================================================
// Standalone transform API: text_process, reduce_text_process, etc.
// ===========================================================================

#[test]
fn test_text_process() {
    let text = text_process(ProcessType::Fanjian, "~ᗩ~躶~𝚩~軆~Ⲉ~");
    // "躶" (U+8EB6) -> "裸" (U+88F8)
    assert_eq!(text, "~ᗩ~裸~𝚩~軆~Ⲉ~");
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
    let variants = reduce_text_process(ProcessType::FanjianDeleteNormalize, "~ᗩ~躶~𝚩~軆~Ⲉ~");

    // Step-by-step:
    // 0. Original: "~ᗩ~躶~𝚩~軆~Ⲉ~"
    // 1. Fanjian:  "~ᗩ~裸~𝚩~軆~Ⲉ~"
    // 2. Delete:   "ᗩ裸𝚩軆Ⲉ"
    // 3. Normalize:"a裸b軆c"

    assert_eq!(variants.len(), 4);
    assert_eq!(variants[0], "~ᗩ~躶~𝚩~軆~Ⲉ~");
    assert_eq!(variants[1], "~ᗩ~裸~𝚩~軆~Ⲉ~");
    assert_eq!(variants[2], "ᗩ裸𝚩軆Ⲉ");
    assert_eq!(variants[3], "a裸b軆c");
}

#[test]
fn test_reduce_text_process_emit() {
    let variants = reduce_text_process_emit(ProcessType::FanjianDeleteNormalize, "~ᗩ~躶~𝚩~軆~Ⲉ~");

    // emit behavior: replace-type steps overwrite; Delete appends.
    // 1. Start:    ["~ᗩ~躶~𝚩~軆~Ⲉ~"]
    // 2. Fanjian:  ["~ᗩ~裸~𝚩~軆~Ⲉ~"]  (overwritten)
    // 3. Delete:   ["~ᗩ~裸~𝚩~軆~Ⲉ~", "ᗩ裸𝚩軆Ⲉ"]  (pushed)
    // 4. Normalize:["~ᗩ~裸~𝚩~軆~Ⲉ~", "a裸b軆c"]  (overwritten last)

    assert_eq!(variants.len(), 2);
    assert_eq!(variants[0], "~ᗩ~裸~𝚩~軆~Ⲉ~");
    assert_eq!(variants[1], "a裸b軆c");
}

#[test]
fn test_reduce_text_process_all_combined() {
    let text = reduce_text_process(
        ProcessType::Fanjian
            | ProcessType::Delete
            | ProcessType::Normalize
            | ProcessType::PinYin
            | ProcessType::PinYinChar,
        "~ᗩ~躶~𝚩~軆~Ⲉ~ 漢語西安",
    );

    // Final result should be fully normalized pinyin
    assert_eq!(text.last().unwrap(), "a luob tic han yu xi an");
}

#[test]
fn test_dag_specific_outputs() {
    let processed = text_process(ProcessType::Fanjian | ProcessType::Delete, "妳！好");
    assert_eq!(processed, "你好");

    let processed = text_process(ProcessType::Normalize, "ℋЀ⒈㈠Õ");
    assert_eq!(processed, "he11o");
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
fn test_process_map_fanjian_exhaustive() {
    for line in FANJIAN_TEST_DATA.trim().lines() {
        let mut split = line.split('\t');
        let k = split.next().expect("Missing key in FANJIAN.txt");
        let v = split.next().expect("Missing value in FANJIAN.txt");

        // Current implementation is 1-to-1 for Fanjian, truncating v to first char
        let expected_v = v.chars().next().unwrap().to_string();
        assert_eq!(
            text_process(ProcessType::Fanjian, k),
            expected_v,
            "Fanjian failed for {}",
            k
        );
    }
}

#[test]
fn test_process_map_delete_exhaustive() {
    // Test characters from TEXT-DELETE.txt
    for line in DELETE_TEST_DATA.trim().lines() {
        for c in line.chars() {
            let s = c.to_string();
            assert_eq!(
                text_process(ProcessType::Delete, &s),
                "",
                "Delete failed for char '{}' (U+{:04X})",
                c,
                c as u32
            );
        }
    }

    // Test whitespace from WHITE_SPACE constant
    let white_spaces = [
        "\u{0009}", "\u{000A}", "\u{000B}", "\u{000C}", "\u{000D}", "\u{0020}", "\u{0085}",
        "\u{00A0}", "\u{1680}", "\u{2000}", "\u{2001}", "\u{2002}", "\u{2003}", "\u{2004}",
        "\u{2005}", "\u{2006}", "\u{2007}", "\u{2008}", "\u{2009}", "\u{200A}", "\u{200D}",
        "\u{200F}", "\u{2028}", "\u{2029}", "\u{202F}", "\u{205F}", "\u{3000}",
    ];
    for ws in white_spaces {
        assert_eq!(
            text_process(ProcessType::Delete, ws),
            "",
            "Delete failed for whitespace U+{:04X}",
            ws.chars().next().unwrap() as u32
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
        HashMap::from([(1, "你好")]),
    )]))
    .unwrap();
    assert!(
        simple_matcher.is_match("妳好"),
        "Fanjian should match traditional variant of 你好"
    );

    let simple_matcher = SimpleMatcher::new(&HashMap::from([(
        ProcessType::Fanjian,
        HashMap::from([(1, "妳好")]),
    )]))
    .unwrap();
    assert!(
        simple_matcher.is_match("你好"),
        "Fanjian should match simplified variant of 妳好"
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
        HashMap::from([(1, "he11o")]),
    )]))
    .unwrap();
    assert!(
        simple_matcher.is_match("ℋЀ⒈㈠Õ"),
        "Normalize should map fancy chars to 'he11o'"
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
            "你好",
        )
        .build()
        .unwrap();

    assert!(
        matcher.is_match("妳！好"),
        "Should match with Fanjian and Delete combined"
    );
}

#[test]
fn test_complex_process_type_interactions() {
    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::Fanjian | ProcessType::PinYin, 1, "apple&西安")
        .build()
        .unwrap();

    assert!(!matcher.is_match("妳 洗按"));
    assert!(matcher.is_match("apple 妳 洗按"));
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
        .add_word(ProcessType::Fanjian | ProcessType::PinYin, 1, "你好")
        .build()
        .unwrap();

    assert!(matcher.is_match("你好"), "simplified direct");
    assert!(matcher.is_match("妳好"), "traditional variant via Fanjian");
    // PinYin path: different CJK chars with same pinyin syllables
    assert!(
        matcher.is_match("尼号"),
        "different chars with same pinyin 'ni hao'"
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
        .add_word(ProcessType::Fanjian, 1, "你好")
        .build()
        .unwrap();

    results.clear();
    fanjian_matcher.process_into("妳好", &mut results);
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
