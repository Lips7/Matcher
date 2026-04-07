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
        VARIANT_NORM_TEST_DATA
            .trim()
            .lines()
            .filter(|l| !l.starts_with('#'))
            .all(|line| !line
                .split('\t')
                .next()
                .expect("Missing VARIANT_NORM key")
                .is_ascii()),
        "VARIANT_NORM.txt should not contain ASCII keys"
    );

    let mut saw_ascii_delete = false;
    for token in DELETE_TEST_DATA
        .trim()
        .lines()
        .filter(|l| !l.starts_with('#'))
    {
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
        for line in data.trim().lines().filter(|l| !l.starts_with('#')) {
            let mut split = line.split('\t');
            let k = split.next().expect("Missing normalize key");
            let v = split.next().expect("Missing normalize value");
            assert!(
                !k.is_ascii() || v.is_ascii(),
                "ASCII normalize key must map to ASCII output: {k:?} -> {v:?}"
            );
        }
    }

    for line in ROMANIZE_TEST_DATA
        .trim()
        .lines()
        .filter(|l| !l.starts_with('#'))
    {
        let mut split = line.split('\t');
        let k = split.next().expect("Missing romanize key");
        let v = split.next().expect("Missing romanize value");
        assert!(!k.is_ascii(), "ROMANIZE.txt should not contain ASCII keys");
        assert!(v.is_ascii(), "ROMANIZE.txt values should stay ASCII");
    }
}

#[test]
fn test_process_map_variant_norm_exhaustive() {
    for line in VARIANT_NORM_TEST_DATA
        .trim()
        .lines()
        .filter(|l| !l.starts_with('#'))
    {
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
    for token in DELETE_TEST_DATA
        .trim()
        .lines()
        .filter(|l| !l.starts_with('#'))
    {
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
        for line in data.trim().lines().filter(|l| !l.starts_with('#')) {
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
    for line in ROMANIZE_TEST_DATA
        .trim()
        .lines()
        .filter(|l| !l.starts_with('#'))
    {
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
    for line in ROMANIZE_TEST_DATA
        .trim()
        .lines()
        .filter(|l| !l.starts_with('#'))
    {
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
fn test_normalize_combining_characters() {
    // Normalize uses a per-codepoint lookup table, NOT full NFC composition.
    // Precomposed chars (é = U+00E9) have table entries that strip diacritics.
    // Decomposed combining marks (U+0301) are standalone codepoints without table
    // entries, so they pass through unchanged.
    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::Normalize, 1, "cafe")
        .build()
        .unwrap();

    // Precomposed: "café" = U+0063 U+0061 U+0066 U+00E9 → "cafe"
    assert!(
        matcher.is_match("caf\u{00E9}"),
        "precomposed é should normalize to cafe via table lookup"
    );

    // Decomposed: "cafe\u{0301}" → "cafe" + combining mark passes through
    // The combining mark is a separate codepoint not in the normalize table,
    // so the output is "cafe\u{0301}" (the "cafe" prefix still matches though).
    assert!(
        matcher.is_match("cafe\u{0301}"),
        "cafe prefix matches even with trailing combining mark"
    );

    // Verify the precomposed form actually strips the diacritic
    let precomposed = text_process(ProcessType::Normalize, "caf\u{00E9}");
    assert_eq!(precomposed.as_ref(), "cafe", "precomposed é → e via table");

    // Verify the decomposed combining mark passes through (not composed then stripped)
    let decomposed = text_process(ProcessType::Normalize, "\u{0301}");
    assert_eq!(
        decomposed.as_ref(),
        "\u{0301}",
        "standalone combining mark has no table entry, passes through"
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
fn test_romanize_skips_ascii_digits() {
    // Romanize/RomanizeChar should skip ASCII digits (no ASCII keys in generated table)
    for pt in [ProcessType::Romanize, ProcessType::RomanizeChar] {
        let matcher = SimpleMatcherBuilder::new()
            .add_word(pt, 1, "yi")
            .build()
            .unwrap();
        assert!(!matcher.is_match("1"), "{pt:?} should skip ASCII digits");
    }
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
// CJK script expansion: table-driven text_process validation
// ===========================================================================

#[test]
fn test_text_process_cjk_scripts() {
    let cases: &[(ProcessType, &str, &str)] = &[
        // VariantNorm: halfwidth katakana → fullwidth
        (ProcessType::VariantNorm, "ｱｲｳ", "アイウ"),
        // VariantNorm: mixed halfwidth katakana with ASCII/CJK
        (
            ProcessType::VariantNorm,
            "hello ｶﾀｶﾅ world",
            "hello カタカナ world",
        ),
        // VariantNorm: fullwidth katakana unchanged
        (ProcessType::VariantNorm, "アイウ", "アイウ"),
        // VariantNorm: Chinese T→S preserved
        (ProcessType::VariantNorm, "國語", "国语"),
        // Romanize: Japanese hiragana
        (ProcessType::Romanize, "あいう", " a i u"),
        // Romanize: Japanese katakana
        (ProcessType::Romanize, "カタカナ", " ka ta ka na"),
        // Romanize: Korean hangul
        (ProcessType::Romanize, "한글", " han geul"),
        // Romanize: Korean Seoul
        (ProcessType::Romanize, "서울", " seo ul"),
        // Romanize: Chinese pinyin preserved
        (ProcessType::Romanize, "中国", " zhong guo"),
        // Romanize: mixed Chinese + Japanese + Korean
        (ProcessType::Romanize, "中あ한", " zhong a han"),
        // RomanizeChar: strips inter-syllable spaces
        (ProcessType::RomanizeChar, "한글", "hangeul"),
    ];

    for &(pt, input, expected) in cases {
        let result = text_process(pt, input);
        assert_eq!(
            result.as_ref(),
            expected,
            "{pt:?} on {input:?}: expected {expected:?}, got {:?}",
            result.as_ref()
        );
    }
}

// ===========================================================================
// CJK script expansion: table-driven matcher integration
// ===========================================================================

#[test]
fn test_matcher_cjk_scripts() {
    let cases: &[(ProcessType, &str, &str)] = &[
        // Korean romanization matching
        (ProcessType::Romanize, "han", "한국"),
        // Japanese kana romanization matching
        (ProcessType::Romanize, "shi", "しんぶん"),
        // Half-width katakana matches full-width pattern
        (ProcessType::VariantNorm, "カタカナ", "ｶﾀｶﾅ"),
    ];

    for &(pt, pattern, input) in cases {
        let matcher = SimpleMatcherBuilder::new()
            .add_word(pt, 1, pattern)
            .build()
            .unwrap();
        assert!(
            matcher.is_match(input),
            "{pt:?} pattern={pattern:?} should match input={input:?}"
        );
    }
}

// ===========================================================================
// Mixed CJK scripts through matcher
// ===========================================================================

#[test]
fn test_romanize_mixed_cjk_matcher() {
    // Verify matching a romanized pattern against mixed-CJK text through SimpleMatcher.
    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::Romanize, 1, "zhong")
        .build()
        .unwrap();

    assert!(
        matcher.is_match("中あ한"),
        "romanized 'zhong' should match '中' in mixed CJK text"
    );
    let results = matcher.process("中あ한");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].word_id, 1);
}

// ===========================================================================
// EmojiNorm transform tests
// ===========================================================================

#[test]
fn test_emoji_norm_basic() {
    let result = text_process(ProcessType::EmojiNorm, "👍");
    assert_eq!(result, " thumbs_up");
}

#[test]
fn test_emoji_norm_fire() {
    let result = text_process(ProcessType::EmojiNorm, "🔥");
    assert_eq!(result, " fire");
}

#[test]
fn test_emoji_norm_mixed_text() {
    // "Hello " + " fire" + " World" → "Hello  fire World"
    let result = text_process(ProcessType::EmojiNorm, "Hello 🔥 World");
    assert_eq!(result, "Hello  fire World");
}

#[test]
fn test_emoji_norm_skin_tone_stripped() {
    // 👍🏽 = U+1F44D + U+1F3FD (skin tone modifier)
    // Skin tone stripped, base emoji normalized
    let result = text_process(ProcessType::EmojiNorm, "👍🏽");
    assert_eq!(result, " thumbs_up");
}

#[test]
fn test_emoji_norm_zwj_sequence() {
    // ZWJ (U+200D) stripped, each component normalized independently
    let result = text_process(ProcessType::EmojiNorm, "👨\u{200D}👩\u{200D}👧");
    assert_eq!(result, " man woman girl");
}

#[test]
fn test_emoji_norm_vs16_stripped() {
    // VS16 (U+FE0F) should be stripped
    let result = text_process(ProcessType::EmojiNorm, "❤\u{FE0F}");
    assert_eq!(result, " red_heart");
}

#[test]
fn test_emoji_norm_ascii_passthrough() {
    let result = text_process(ProcessType::EmojiNorm, "hello world");
    assert_eq!(result, "hello world");
}

#[test]
fn test_emoji_norm_multiple_emoji() {
    let result = text_process(ProcessType::EmojiNorm, "🔥❤🎉");
    assert_eq!(result, " fire red_heart party_popper");
}

#[test]
fn test_emoji_norm_matcher_integration() {
    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::EmojiNorm, 1, "fire")
        .add_word(ProcessType::EmojiNorm, 2, "thumbs_up")
        .build()
        .unwrap();

    assert!(matcher.is_match("🔥"));
    assert!(matcher.is_match("👍🏽"));
    assert!(!matcher.is_match("hello"));

    let results = matcher.process("I love 🔥 and 👍");
    assert_eq!(results.len(), 2);
}

#[test]
fn test_emoji_norm_with_normalize() {
    // EmojiNorm | Normalize: emoji→words + casefold
    let pt = ProcessType::EmojiNorm | ProcessType::Normalize;
    let matcher = SimpleMatcherBuilder::new()
        .add_word(pt, 1, "fire")
        .build()
        .unwrap();

    assert!(matcher.is_match("🔥"));
}

// ===========================================================================
// ProcessType composition edge cases
// ===========================================================================

#[test]
fn test_delete_emoji_norm_composition_gotcha() {
    // Delete strips emoji codepoints BEFORE EmojiNorm can convert them.
    // This is a documented pitfall: Delete | EmojiNorm won't match emoji→word patterns.
    let pt = ProcessType::Delete | ProcessType::EmojiNorm;
    let matcher = SimpleMatcherBuilder::new()
        .add_word(pt, 1, "fire")
        .build()
        .unwrap();

    // "fire" as emoji is deleted before EmojiNorm runs, so no match
    assert!(
        !matcher.is_match("🔥"),
        "Delete|EmojiNorm should NOT match emoji (Delete strips it first)"
    );
    // But literal "fire" in text still matches (Delete doesn't remove letters)
    assert!(matcher.is_match("fire"));
}

#[test]
fn test_none_in_composite_preserves_raw_path() {
    // Including None in a composite type matches against both raw and transformed text.
    let pt = ProcessType::None | ProcessType::Delete;
    let matcher = SimpleMatcherBuilder::new()
        .add_word(pt, 1, "helloworld")
        .build()
        .unwrap();

    // "helloworld" matches raw text
    assert!(matcher.is_match("helloworld"));
    // "hello world" doesn't match raw (space), but Delete strips space → "helloworld"
    assert!(matcher.is_match("hello world"));
    // "hello-world" → Delete strips hyphen → "helloworld"
    assert!(matcher.is_match("hello-world"));
}

#[test]
fn test_romanize_vs_romanize_char() {
    // Romanize adds boundary spaces: 西安 → " xi  an " (two separate syllables)
    // RomanizeChar omits them:        西安 → "xian" (no boundaries)
    let matcher_r = SimpleMatcherBuilder::new()
        .add_word(ProcessType::Romanize, 1, "xian")
        .build()
        .unwrap();

    let matcher_rc = SimpleMatcherBuilder::new()
        .add_word(ProcessType::RomanizeChar, 1, "xian")
        .build()
        .unwrap();

    // 先 → Romanize: " xian " (single syllable, contains "xian")
    // 西安 → Romanize: " xi  an " (two syllables, does NOT contain "xian")
    assert!(
        matcher_r.is_match("先"),
        "Romanize: 先 → ' xian ' should contain 'xian'"
    );
    assert!(
        !matcher_r.is_match("西安"),
        "Romanize: 西安 → ' xi  an ' should NOT contain 'xian' (boundary-separated)"
    );

    // RomanizeChar: both 先 and 西安 → "xian" (no spaces)
    assert!(matcher_rc.is_match("先"));
    assert!(matcher_rc.is_match("西安"));
}

#[test]
fn test_variant_norm_delete_normalize_composition() {
    // VariantNormDeleteNormalize is the kitchen-sink transform.
    let pt = ProcessType::VariantNormDeleteNormalize;
    let matcher = SimpleMatcherBuilder::new()
        .add_word(pt, 1, "测试")
        .build()
        .unwrap();

    // Traditional + punctuation + width variants all normalize
    assert!(matcher.is_match("測試"));
    assert!(matcher.is_match("測，試"));
    assert!(matcher.is_match("測 試"));
}
