use matcher_rs::{
    ProcessType, SimpleMatcherBuilder, reduce_text_process, reduce_text_process_emit, text_process,
};

// ===========================================================================
// Standalone transform API: text_process, reduce_text_process, etc.
// ===========================================================================

#[test]
fn test_delete_simd_skip_ascii_before_non_ascii() {
    // Regression: SIMD fast-skip in DeleteFindIter incorrectly advanced to the first
    // non-ASCII byte without checking for deletable ASCII bytes before it. Spaces
    // between non-deletable ASCII letters and Chinese characters were not deleted.
    let variants = reduce_text_process(ProcessType::VariantNormDeleteNormalize, "A B 測試 Ａ １");
    assert_eq!(variants[0], "A B 測試 Ａ １");
    assert_eq!(variants[1], "A B 测试 Ａ １");
    assert_eq!(variants[2], "AB测试Ａ１");
    assert_eq!(variants[3], "ab测试a1");
}

#[test]
fn test_reduce_text_process() {
    let variants = reduce_text_process(ProcessType::VariantNormDeleteNormalize, "！Ａ！測試！１！");

    // Step-by-step:
    // 0. Original: "！Ａ！測試！１！"
    // 1. VariantNorm:  "！Ａ！测试！１！"
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
        reduce_text_process_emit(ProcessType::VariantNormDeleteNormalize, "！Ａ！測試！１！");

    // emit behavior: replace-type steps overwrite; Delete appends.
    // 1. Start:    ["！Ａ！測試！１！"]
    // 2. VariantNorm:  ["！Ａ！测试！１！"]  (overwritten)
    // 3. Delete:   ["！Ａ！测试！１！", "Ａ测试１"]  (pushed)
    // 4. Normalize:["！Ａ！测试！１！", "a测试1"]  (overwritten last)

    assert_eq!(variants.len(), 2);
    assert_eq!(variants[0], "！Ａ！测试！１！");
    assert_eq!(variants[1], "a测试1");
}

#[test]
fn test_reduce_text_process_all_combined() {
    let text = reduce_text_process(
        ProcessType::VariantNorm
            | ProcessType::Delete
            | ProcessType::Normalize
            | ProcessType::Romanize
            | ProcessType::RomanizeChar,
        "Ａ！漢語西安１",
    );

    // Final result should be fully normalized romanize
    assert_eq!(text.last().unwrap(), "a han yu xi an1");
}

// ===========================================================================
// Exhaustive process-map validation
// ===========================================================================

const VARIANT_NORM_TEST_DATA: &str = include_str!("../process_map/VARIANT_NORM.txt");
const DELETE_TEST_DATA: &str = include_str!("../process_map/TEXT-DELETE.txt");
const NORM_TEST_DATA: &str = include_str!("../process_map/NORM.txt");
const NUM_NORM_TEST_DATA: &str = include_str!("../process_map/NUM-NORM.txt");
const ROMANIZE_TEST_DATA: &str = include_str!("../process_map/ROMANIZE.txt");

#[test]
fn test_process_map_ascii_invariants() {
    assert!(
        VARIANT_NORM_TEST_DATA.trim().lines().all(|line| !line
            .split('\t')
            .next()
            .expect("Missing VARIANT_NORM key")
            .is_ascii()),
        "VARIANT_NORM.txt should not contain ASCII keys"
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

    for line in ROMANIZE_TEST_DATA.trim().lines() {
        let mut split = line.split('\t');
        let k = split.next().expect("Missing romanize key");
        let v = split.next().expect("Missing romanize value");
        assert!(!k.is_ascii(), "ROMANIZE.txt should not contain ASCII keys");
        assert!(v.is_ascii(), "ROMANIZE.txt values should stay ASCII");
    }
}

#[test]
fn test_process_map_variant_norm_exhaustive() {
    for line in VARIANT_NORM_TEST_DATA.trim().lines() {
        let mut split = line.split('\t');
        let k = split.next().expect("Missing key in VARIANT_NORM.txt");
        let v = split.next().expect("Missing value in VARIANT_NORM.txt");

        assert_eq!(
            k.chars().count(),
            1,
            "VARIANT_NORM key must be one char: {k:?}"
        );
        assert_eq!(
            v.chars().count(),
            1,
            "VARIANT_NORM value must be one char: {v:?}"
        );
        assert_eq!(
            text_process(ProcessType::VariantNorm, k),
            v,
            "VariantNorm failed for {}",
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
fn test_process_map_romanize_exhaustive() {
    for line in ROMANIZE_TEST_DATA.trim().lines() {
        let mut split = line.split('\t');
        let k = split.next().expect("Missing key in ROMANIZE.txt");
        let v = split.next().expect("Missing value in ROMANIZE.txt");
        assert_eq!(k.chars().count(), 1, "ROMANIZE key must be one char: {k:?}");
        assert!(!v.is_empty(), "ROMANIZE value must not be empty for {k:?}");

        assert_eq!(
            text_process(ProcessType::Romanize, k),
            v,
            "Romanize failed for {}",
            k
        );
    }
}

#[test]
fn test_process_map_romanize_char_exhaustive() {
    for line in ROMANIZE_TEST_DATA.trim().lines() {
        let mut split = line.split('\t');
        let k = split.next().expect("Missing key in ROMANIZE.txt");
        let v = split.next().expect("Missing value in ROMANIZE.txt");
        assert_eq!(k.chars().count(), 1, "ROMANIZE key must be one char: {k:?}");
        assert!(!v.is_empty(), "ROMANIZE value must not be empty for {k:?}");

        assert_eq!(
            text_process(ProcessType::RomanizeChar, k),
            v.trim(),
            "RomanizeChar failed for {}",
            k
        );
    }
}

// ===========================================================================
// Individual ProcessTypes through SimpleMatcher
// ===========================================================================

#[test]
fn test_variant_norm() {
    use matcher_rs::SimpleMatcher;
    use std::collections::HashMap;

    let simple_matcher = SimpleMatcher::new(&HashMap::from([(
        ProcessType::VariantNorm,
        HashMap::from([(1, "测试")]),
    )]))
    .unwrap();
    assert!(
        simple_matcher.is_match("測試"),
        "VariantNorm should match traditional variant of 测试"
    );

    let simple_matcher = SimpleMatcher::new(&HashMap::from([(
        ProcessType::VariantNorm,
        HashMap::from([(1, "測試")]),
    )]))
    .unwrap();
    assert!(
        simple_matcher.is_match("测试"),
        "VariantNorm should match simplified variant of 測試"
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
fn test_normalize_strips_diacritics() {
    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::Normalize, 1, "cafe")
        .add_word(ProcessType::Normalize, 2, "c")
        .build()
        .unwrap();

    assert!(matcher.is_match("Café"), "should strip precomposed acute");
    assert!(matcher.is_match("café"), "should strip lowercase acute");
    assert!(matcher.is_match("CAFÉ"), "should strip uppercase acute");
    assert!(matcher.is_match("Ć"), "Ć should normalize to c");
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
fn test_romanize() {
    use matcher_rs::SimpleMatcher;
    use std::collections::HashMap;

    let simple_matcher = SimpleMatcher::new(&HashMap::from([(
        ProcessType::Romanize,
        HashMap::from([(1, "西安")]),
    )]))
    .unwrap();
    assert!(
        simple_matcher.is_match("洗按"),
        "Romanize xi an should match 洗按 (xi an)"
    );
    assert!(
        !simple_matcher.is_match("现"),
        "Romanize xi an should not match 现 (xian without space)"
    );
}

#[test]
fn test_romanize_leaf_does_not_apply_ascii_digit_mappings() {
    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::Romanize, 1, "yi")
        .build()
        .unwrap();

    assert!(
        !matcher.is_match("1"),
        "Romanize should skip ASCII digits because the generated table has no ASCII keys"
    );
}

#[test]
fn test_romanizechar() {
    use matcher_rs::SimpleMatcher;
    use std::collections::HashMap;

    let simple_matcher = SimpleMatcher::new(&HashMap::from([(
        ProcessType::RomanizeChar,
        HashMap::from([(1, "西安")]),
    )]))
    .unwrap();
    assert!(
        simple_matcher.is_match("洗按"),
        "RomanizeChar xi an should match 洗按"
    );
    assert!(
        simple_matcher.is_match("现"),
        "RomanizeChar xi an should match 现 (xian without space)"
    );
    assert!(
        simple_matcher.is_match("xian"),
        "RomanizeChar should match literal xian"
    );
}

#[test]
fn test_romanizechar_leaf_does_not_apply_ascii_digit_mappings() {
    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::RomanizeChar, 1, "yi")
        .build()
        .unwrap();

    assert!(
        !matcher.is_match("1"),
        "RomanizeChar should skip ASCII digits because the generated table has no ASCII keys"
    );
}

// ===========================================================================
// Composite ProcessTypes through SimpleMatcher
// ===========================================================================

#[test]
fn test_cross_variant_matching() {
    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None | ProcessType::Romanize, 1, "apple&西安")
        .build()
        .unwrap();

    assert!(
        matcher.is_match("apple 洗按"),
        "Cross-variant matching should work: 'apple' (None) and '西安' (Romanize)"
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
            ProcessType::VariantNorm | ProcessType::Delete | ProcessType::Normalize,
            1,
            "测试",
        )
        .build()
        .unwrap();

    assert!(
        matcher.is_match("測！試"),
        "Should match with VariantNorm and Delete combined"
    );
}

#[test]
fn test_complex_process_type_interactions() {
    let matcher = SimpleMatcherBuilder::new()
        .add_word(
            ProcessType::VariantNorm | ProcessType::Romanize,
            1,
            "apple&西安",
        )
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
fn test_process_type_variant_norm_romanize() {
    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::VariantNorm | ProcessType::Romanize, 1, "测试")
        .build()
        .unwrap();

    assert!(matcher.is_match("测试"), "simplified direct");
    assert!(
        matcher.is_match("測試"),
        "traditional variant via VariantNorm"
    );
    // Romanize path: different CJK chars with same romanize syllables
    assert!(
        matcher.is_match("策士"),
        "different chars with same romanize 'ce shi'"
    );
}

// ===========================================================================
// Unicode robustness
// ===========================================================================

#[test]
fn test_unicode_emoji_passthrough() {
    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::VariantNormDeleteNormalize, 1, "test")
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

    // process_into under VariantNorm
    let variant_norm_matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::VariantNorm, 1, "测试")
        .build()
        .unwrap();

    results.clear();
    variant_norm_matcher.process_into("測試", &mut results);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].word_id, 1);
}

// ===========================================================================
// Romanize non-ASCII pattern matching
// ===========================================================================

#[test]
fn test_romanize_non_ascii_pattern_match() {
    // End-to-end: a non-ASCII pattern registered under Romanize must still be
    // found when the Romanize-transformed text contains unmapped non-ASCII chars.
    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::Romanize, 1, "한글")
        .build()
        .unwrap();
    // Input: Chinese "你" (converted to romanize) + Korean "한글" (passes through).
    assert!(
        matcher.is_match("你한글"),
        "non-ASCII pattern under Romanize should match when unmapped chars pass through"
    );
    let results = matcher.process("你한글");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].word_id, 1);
}

// ===========================================================================
// ProcessType Display formatting
// ===========================================================================

#[test]
fn test_process_type_display() {
    assert_eq!(format!("{}", ProcessType::None), "none");
    assert_eq!(format!("{}", ProcessType::VariantNorm), "variant_norm");
    assert_eq!(format!("{}", ProcessType::Delete), "delete");
    assert_eq!(format!("{}", ProcessType::Normalize), "normalize");
    assert_eq!(format!("{}", ProcessType::Romanize), "romanize");
    assert_eq!(format!("{}", ProcessType::RomanizeChar), "romanize_char");
    assert_eq!(
        format!("{}", ProcessType::VariantNorm | ProcessType::Delete),
        "variant_norm_delete"
    );
    assert_eq!(
        format!("{}", ProcessType::VariantNormDeleteNormalize),
        "variant_norm_delete_normalize"
    );
    assert_eq!(format!("{}", ProcessType::empty()), "");
}

// ===========================================================================
// RomanizeChar passthrough for unmapped scripts
// ===========================================================================

#[test]
fn test_romanizechar_passthrough_unmapped() {
    // Thai has no Romanize mapping — passes through RomanizeChar unchanged.
    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::RomanizeChar, 1, "สวัสดี")
        .build()
        .unwrap();
    assert!(matcher.is_match("สวัสดี"));
    assert!(!matcher.is_match("sawasdee"));
}

// ===========================================================================
// Streaming scan paths (search.rs coverage)
// ===========================================================================

#[test]
fn test_streaming_scan_normalize() {
    // Normalize + VariantNorm forces General mode; the Normalize leaf uses
    // scan_variant_streaming through the NormalizeMatcher byte iterator.
    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::Normalize, 1, "ab")
        .add_word(ProcessType::VariantNorm, 2, "测试")
        .build()
        .unwrap();

    assert!(matcher.is_match("ＡＢ"), "fullwidth -> normalized to ab");
    assert!(
        matcher.is_match("測試"),
        "traditional -> simplified via VariantNorm"
    );
    let results = matcher.process("ＡＢ 測試");
    assert_eq!(results.len(), 2);
}

#[test]
fn test_streaming_scan_romanize() {
    // Romanize + VariantNorm forces General mode; the Romanize leaf uses
    // scan_variant_streaming through the RomanizeMatcher byte iterator.
    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::Romanize, 1, "xi an")
        .add_word(ProcessType::VariantNorm, 2, "测试")
        .build()
        .unwrap();

    assert!(matcher.is_match("西安"), "romanize conversion");
    assert!(matcher.is_match("測試"), "variant_norm conversion");
}

#[test]
fn test_leaf_ascii_noop_optimization() {
    // VariantNorm is a no-op on pure ASCII. When parent text is ASCII and the leaf
    // step is VariantNorm, the scan reuses parent text (no materialization).
    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::VariantNorm, 1, "hello")
        .add_word(ProcessType::None, 2, "world")
        .build()
        .unwrap();

    assert!(matcher.is_match("hello world"));
    let results = matcher.process("hello world");
    assert_eq!(results.len(), 2);
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

    // VariantNorm should not alter PUA chars
    let result = text_process(ProcessType::VariantNorm, pua);
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

// ===========================================================================
// Japanese / Korean CJK expansion tests
// ===========================================================================

#[test]
fn test_variant_norm_halfwidth_katakana() {
    // Half-width katakana (U+FF71 ｱ) → full-width (U+30A2 ア)
    let result = text_process(ProcessType::VariantNorm, "ｱｲｳ");
    assert_eq!(result.as_ref(), "アイウ");
}

#[test]
fn test_variant_norm_halfwidth_katakana_mixed() {
    // Mixed half-width katakana with ASCII and CJK
    let result = text_process(ProcessType::VariantNorm, "hello ｶﾀｶﾅ world");
    assert_eq!(result.as_ref(), "hello カタカナ world");
}

#[test]
fn test_variant_norm_preserves_fullwidth_katakana() {
    // Full-width katakana should pass through unchanged
    let result = text_process(ProcessType::VariantNorm, "アイウ");
    assert_eq!(result.as_ref(), "アイウ");
}

#[test]
fn test_variant_norm_chinese_still_works() {
    // Existing Chinese T→S behavior is preserved
    let result = text_process(ProcessType::VariantNorm, "國語");
    assert_eq!(result.as_ref(), "国语");
}

#[test]
fn test_romanize_japanese_hiragana() {
    // Hiragana → Romaji
    let result = text_process(ProcessType::Romanize, "あいう");
    assert_eq!(result.as_ref(), " a i u");
}

#[test]
fn test_romanize_japanese_katakana() {
    // Katakana → Romaji
    let result = text_process(ProcessType::Romanize, "カタカナ");
    assert_eq!(result.as_ref(), " ka ta ka na");
}

#[test]
fn test_romanize_korean_hangul() {
    // Korean Hangul → Revised Romanization
    let result = text_process(ProcessType::Romanize, "한글");
    assert_eq!(result.as_ref(), " han geul");
}

#[test]
fn test_romanize_korean_hangul_seoul() {
    let result = text_process(ProcessType::Romanize, "서울");
    assert_eq!(result.as_ref(), " seo ul");
}

#[test]
fn test_romanize_chinese_still_works() {
    // Existing Chinese Pinyin behavior is preserved
    let result = text_process(ProcessType::Romanize, "中国");
    assert_eq!(result.as_ref(), " zhong guo");
}

#[test]
fn test_romanize_char_strips_spaces() {
    // RomanizeChar should strip inter-syllable spaces
    let result = text_process(ProcessType::RomanizeChar, "한글");
    assert_eq!(result.as_ref(), "hangeul");
}

#[test]
fn test_romanize_mixed_cjk_scripts() {
    // Mixed Chinese + Japanese kana + Korean
    let result = text_process(ProcessType::Romanize, "中あ한");
    assert_eq!(result.as_ref(), " zhong a han");
}

#[test]
fn test_matcher_romanize_korean() {
    // Matching via Korean romanization
    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::Romanize, 1, "han")
        .build()
        .unwrap();
    assert!(matcher.is_match("한국"));
}

#[test]
fn test_matcher_romanize_japanese_kana() {
    // Matching via Japanese kana romanization
    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::Romanize, 1, "shi")
        .build()
        .unwrap();
    assert!(matcher.is_match("しんぶん"));
}

#[test]
fn test_matcher_variant_norm_halfwidth() {
    // Matching patterns against half-width katakana input
    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::VariantNorm, 1, "カタカナ")
        .build()
        .unwrap();
    assert!(
        matcher.is_match("ｶﾀｶﾅ"),
        "half-width should match full-width pattern"
    );
}
