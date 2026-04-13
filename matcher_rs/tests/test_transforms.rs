use std::path::Path;

use matcher_rs::{
    ProcessType, SimpleMatcherBuilder, reduce_text_process, reduce_text_process_emit, text_process,
};

// ===========================================================================
// Individual ProcessType spot checks (table-driven)
// ===========================================================================

#[test]
fn test_each_process_type() {
    let cases: &[(ProcessType, &str, &str)] = &[
        // VariantNorm: Traditional → Simplified Chinese
        (ProcessType::VariantNorm, "測試", "测试"),
        (ProcessType::VariantNorm, "國語", "国语"),
        // VariantNorm: halfwidth katakana → fullwidth
        (ProcessType::VariantNorm, "ｱｲｳ", "アイウ"),
        (
            ProcessType::VariantNorm,
            "hello ｶﾀｶﾅ world",
            "hello カタカナ world",
        ),
        // VariantNorm: fullwidth katakana unchanged
        (ProcessType::VariantNorm, "アイウ", "アイウ"),
        // Delete: strips configured codepoints
        (ProcessType::Delete, "a*b", "ab"),
        (ProcessType::Delete, "a b", "ab"),
        (ProcessType::Delete, "a!!b", "ab"),
        // Normalize: compatibility chars + casefold + diacritic strip
        (ProcessType::Normalize, "ＡＢⅣ①℉", "ab41°f"),
        (ProcessType::Normalize, "Café", "cafe"),
        (ProcessType::Normalize, "CAFÉ", "cafe"),
        (ProcessType::Normalize, "I", "i"),
        // Romanize: CJK → pinyin with inter-char spacing
        (ProcessType::Romanize, "西安", " xi an"),
        (ProcessType::Romanize, "한글", " han geul"),
        (ProcessType::Romanize, "あいう", " a i u"),
        (ProcessType::Romanize, "カタカナ", " ka ta ka na"),
        (ProcessType::Romanize, "中あ한", " zhong a han"),
        // RomanizeChar: no inter-char spaces
        (ProcessType::RomanizeChar, "西安", "xian"),
        (ProcessType::RomanizeChar, "한글", "hangeul"),
        // EmojiNorm: emoji → CLDR short names
        (ProcessType::EmojiNorm, "👍", " thumbs_up"),
        (ProcessType::EmojiNorm, "🔥", " fire"),
        (ProcessType::EmojiNorm, "hello world", "hello world"),
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

    // Romanize/RomanizeChar should skip ASCII digits
    for pt in [ProcessType::Romanize, ProcessType::RomanizeChar] {
        let m = SimpleMatcherBuilder::new()
            .add_word(pt, 1, "yi")
            .build()
            .unwrap();
        assert!(!m.is_match("1"), "{pt:?} should skip ASCII digits");
    }
}

// ===========================================================================
// Normalize: combining characters (subtle behavior)
// ===========================================================================

#[test]
fn test_normalize_combining_characters() {
    // Per-codepoint lookup table, NOT full NFC composition.
    // Precomposed chars have table entries; standalone combining marks pass
    // through.
    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::Normalize, 1, "cafe")
        .build()
        .unwrap();

    // Precomposed é (U+00E9) → "e" via table
    assert!(matcher.is_match("caf\u{00E9}"));
    let precomposed = text_process(ProcessType::Normalize, "caf\u{00E9}");
    assert_eq!(precomposed.as_ref(), "cafe");

    // Decomposed: "cafe" + combining acute (U+0301) — "cafe" prefix still matches
    assert!(matcher.is_match("cafe\u{0301}"));
    // Standalone combining mark has no table entry, passes through
    let decomposed = text_process(ProcessType::Normalize, "\u{0301}");
    assert_eq!(decomposed.as_ref(), "\u{0301}");
}

// ===========================================================================
// EmojiNorm modifiers (table-driven)
// ===========================================================================

#[test]
fn test_emoji_norm_modifiers() {
    let cases: &[(&str, &str)] = &[
        // Skin tone stripped, base emoji normalized
        ("👍🏽", " thumbs_up"),
        // VS16 stripped
        ("❤\u{FE0F}", " red_heart"),
        // ZWJ sequence: each component normalized independently
        ("👨\u{200D}👩\u{200D}👧", " man woman girl"),
        // Multiple emoji
        ("🔥❤🎉", " fire red_heart party_popper"),
        // Mixed text
        ("Hello 🔥 World", "Hello  fire World"),
    ];

    for &(input, expected) in cases {
        let result = text_process(ProcessType::EmojiNorm, input);
        assert_eq!(result.as_ref(), expected, "EmojiNorm on {input:?}");
    }
}

// ===========================================================================
// Standalone transform API: reduce_text_process pipeline
// ===========================================================================

#[test]
fn test_reduce_text_process_pipeline() {
    let variants = reduce_text_process(ProcessType::VariantNormDeleteNormalize, "！Ａ！測試！１！");
    // 0. Original: "！Ａ！測試！１！"
    // 1. VariantNorm: "！Ａ！测试！１！"
    // 2. Delete:      "Ａ测试１"
    // 3. Normalize:   "a测试1"
    assert_eq!(variants.len(), 4);
    assert_eq!(variants[0], "！Ａ！測試！１！");
    assert_eq!(variants[1], "！Ａ！测试！１！");
    assert_eq!(variants[2], "Ａ测试１");
    assert_eq!(variants[3], "a测试1");

    // Emit behavior: replace-type steps overwrite; Delete appends
    let emit =
        reduce_text_process_emit(ProcessType::VariantNormDeleteNormalize, "！Ａ！測試！１！");
    assert_eq!(emit.len(), 2);
    assert_eq!(emit[0], "！Ａ！测试！１！");
    assert_eq!(emit[1], "a测试1");

    // All transforms combined
    let all = reduce_text_process(
        ProcessType::VariantNorm
            | ProcessType::Delete
            | ProcessType::Normalize
            | ProcessType::Romanize
            | ProcessType::RomanizeChar,
        "Ａ！漢語西安１",
    );
    assert_eq!(all.last().unwrap(), "a han yu xi an1");
}

// ===========================================================================
// Regression: SIMD fast-skip in DeleteFindIter
// ===========================================================================

#[test]
fn test_delete_simd_regression() {
    let variants = reduce_text_process(ProcessType::VariantNormDeleteNormalize, "A B 測試 Ａ １");
    assert_eq!(variants[0], "A B 測試 Ａ １");
    assert_eq!(variants[1], "A B 测试 Ａ １");
    assert_eq!(variants[2], "AB测试Ａ１");
    assert_eq!(variants[3], "ab测试a1");
}

// ===========================================================================
// Composite ProcessType through SimpleMatcher
// ===========================================================================

#[test]
fn test_composition_variant_norm_delete_normalize() {
    // Kitchen-sink: VariantNorm + Delete + Normalize
    let m1 = SimpleMatcherBuilder::new()
        .add_word(
            ProcessType::VariantNorm | ProcessType::Delete | ProcessType::Normalize,
            1,
            "测试",
        )
        .build()
        .unwrap();
    assert!(m1.is_match("測！試"));

    // Cross-variant: None + Romanize
    let m2 = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None | ProcessType::Romanize, 1, "apple&西安")
        .build()
        .unwrap();
    assert!(m2.is_match("apple 洗按"));

    // VariantNorm + Romanize
    let m3 = SimpleMatcherBuilder::new()
        .add_word(ProcessType::VariantNorm | ProcessType::Romanize, 1, "测试")
        .build()
        .unwrap();
    assert!(m3.is_match("测试"));
    assert!(m3.is_match("測試"));
    assert!(
        m3.is_match("策士"),
        "different chars with same romanize syllables"
    );
}

#[test]
fn test_delete_emoji_norm_gotcha() {
    // Delete strips emoji codepoints BEFORE EmojiNorm can convert them.
    let pt = ProcessType::Delete | ProcessType::EmojiNorm;
    let matcher = SimpleMatcherBuilder::new()
        .add_word(pt, 1, "fire")
        .build()
        .unwrap();

    assert!(
        !matcher.is_match("🔥"),
        "Delete strips emoji before EmojiNorm"
    );
    assert!(matcher.is_match("fire"), "literal 'fire' still matches");
}

#[test]
fn test_none_in_composite_is_redundant() {
    // None|Delete is silently normalized to Delete. Delete scans both
    // original and deleted text, so the None flag adds nothing.
    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None | ProcessType::Delete, 1, "helloworld")
        .build()
        .unwrap();

    assert!(matcher.is_match("helloworld"), "original text match");
    assert!(matcher.is_match("hello world"), "Delete strips space");
    assert!(matcher.is_match("hello-world"), "Delete strips hyphen");
    assert!(!matcher.is_match("hallo"));
}

#[test]
fn test_romanize_vs_romanize_char_spacing() {
    // Romanize adds boundary spaces; RomanizeChar omits them
    let mr = SimpleMatcherBuilder::new()
        .add_word(ProcessType::Romanize, 1, "xian")
        .build()
        .unwrap();

    let mrc = SimpleMatcherBuilder::new()
        .add_word(ProcessType::RomanizeChar, 1, "xian")
        .build()
        .unwrap();

    // 先 → Romanize: " xian " (single syllable, contains "xian")
    assert!(mr.is_match("先"));
    // 西安 → Romanize: " xi  an " (two syllables, NOT "xian")
    assert!(!mr.is_match("西安"));
    // RomanizeChar: both → "xian" (no spaces)
    assert!(mrc.is_match("先"));
    assert!(mrc.is_match("西安"));
}

#[test]
fn test_or_with_process_type() {
    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::VariantNorm, 1, "测试|世界")
        .build()
        .unwrap();

    assert!(matcher.is_match("测试"));
    assert!(matcher.is_match("世界"));
    assert!(
        matcher.is_match("測試"),
        "traditional variant via VariantNorm"
    );
}

// ===========================================================================
// Streaming scan paths
// ===========================================================================

#[test]
fn test_streaming_scan_paths() {
    // Normalize + VariantNorm forces General mode with streaming iterators
    let m1 = SimpleMatcherBuilder::new()
        .add_word(ProcessType::Normalize, 1, "ab")
        .add_word(ProcessType::VariantNorm, 2, "测试")
        .build()
        .unwrap();

    assert!(m1.is_match("ＡＢ"), "fullwidth → normalized to ab");
    assert!(m1.is_match("測試"), "traditional → simplified");
    assert_eq!(m1.process("ＡＢ 測試").len(), 2);

    // Romanize streaming
    let m2 = SimpleMatcherBuilder::new()
        .add_word(ProcessType::Romanize, 1, "xi an")
        .add_word(ProcessType::VariantNorm, 2, "测试")
        .build()
        .unwrap();

    assert!(m2.is_match("西安"));
    assert!(m2.is_match("測試"));
}

#[test]
fn test_ascii_noop_optimization() {
    // VariantNorm is a no-op on pure ASCII — scan reuses parent text.
    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::VariantNorm, 1, "hello")
        .add_word(ProcessType::None, 2, "world")
        .build()
        .unwrap();

    assert!(matcher.is_match("hello world"));
    assert_eq!(matcher.process("hello world").len(), 2);
}

// ===========================================================================
// Unicode robustness (table-driven)
// ===========================================================================

#[test]
fn test_unicode_robustness() {
    // Emoji + ZWJ passthrough
    let m1 = SimpleMatcherBuilder::new()
        .add_word(ProcessType::VariantNormDeleteNormalize, 1, "test")
        .build()
        .unwrap();
    let text = "test 👨\u{200D}👩\u{200D}👧\u{200D}👦 🎉";
    assert!(m1.is_match(text));
    assert_eq!(m1.process(text).len(), 1);

    // Combining marks: "cafe" matches "cafe" + combining acute as byte prefix
    let m2 = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, "cafe")
        .build()
        .unwrap();
    assert!(m2.is_match("cafe\u{0301}"));

    // Private Use Area chars pass through unchanged
    let pua = "\u{E000}\u{E001}\u{F8FF}";
    let m3 = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, pua)
        .build()
        .unwrap();
    assert!(m3.is_match(pua));
    assert_eq!(text_process(ProcessType::VariantNorm, pua).as_ref(), pua);

    // 4-byte emoji in pattern
    let emoji_pat = "test\u{1F389}";
    let m4 = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, emoji_pat)
        .build()
        .unwrap();
    assert!(m4.is_match("test\u{1F389}"));
    assert!(!m4.is_match("test"));

    // EmojiNorm integration through matcher
    let m5 = SimpleMatcherBuilder::new()
        .add_word(ProcessType::EmojiNorm, 1, "fire")
        .add_word(ProcessType::EmojiNorm, 2, "thumbs_up")
        .build()
        .unwrap();
    assert!(m5.is_match("🔥"));
    assert!(m5.is_match("👍🏽"));
    assert_eq!(m5.process("I love 🔥 and 👍").len(), 2);
}

// ===========================================================================
// Exhaustive process_map validation
// ===========================================================================

fn process_map_dir() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("process_map")
        .leak()
}

fn parse_tab_mappings(path: &Path) -> Vec<(String, String)> {
    std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|line| {
            let mut parts = line.splitn(2, '\t');
            let src = parts.next().unwrap().to_owned();
            let dst = parts.next().unwrap_or("").to_owned();
            (src, dst)
        })
        .collect()
}

fn parse_hex_codepoints(path: &Path) -> Vec<char> {
    std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|line| {
            let hex = line.strip_prefix("U+")?;
            u32::from_str_radix(hex, 16).ok().and_then(char::from_u32)
        })
        .collect()
}

#[test]
fn test_process_map_variant_norm_exhaustive() {
    let mappings = parse_tab_mappings(&process_map_dir().join("VARIANT_NORM.txt"));
    assert!(
        mappings.len() > 1000,
        "expected 1000+ mappings, got {}",
        mappings.len()
    );

    let mut failures = Vec::new();
    for (src, expected) in &mappings {
        let result = text_process(ProcessType::VariantNorm, src);
        if result.as_ref() != expected.as_str() {
            failures.push(format!(
                "{src:?} → expected {expected:?}, got {:?}",
                result.as_ref()
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} VariantNorm failures:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn test_process_map_romanize_exhaustive() {
    let mappings = parse_tab_mappings(&process_map_dir().join("ROMANIZE.txt"));
    assert!(
        mappings.len() > 10000,
        "expected 10000+ mappings, got {}",
        mappings.len()
    );

    let mut failures = Vec::new();
    for (src, raw_dst) in &mappings {
        // build.rs prepends a space for Romanize
        let expected_romanize = format!(" {raw_dst}");
        let result = text_process(ProcessType::Romanize, src);
        if result.as_ref() != expected_romanize.as_str() {
            failures.push(format!(
                "Romanize {src:?} → expected {expected_romanize:?}, got {:?}",
                result.as_ref()
            ));
        }
        // RomanizeChar: same but no leading space
        let result_char = text_process(ProcessType::RomanizeChar, src);
        if result_char.as_ref() != raw_dst.as_str() {
            failures.push(format!(
                "RomanizeChar {src:?} → expected {raw_dst:?}, got {:?}",
                result_char.as_ref()
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} Romanize failures:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn test_process_map_normalize_exhaustive() {
    let norm = parse_tab_mappings(&process_map_dir().join("NORM.txt"));
    let num_norm = parse_tab_mappings(&process_map_dir().join("NUM-NORM.txt"));
    let all_mappings: Vec<_> = norm.iter().chain(num_norm.iter()).collect();
    assert!(
        all_mappings.len() > 5000,
        "expected 5000+ mappings, got {}",
        all_mappings.len()
    );

    // NORM.txt and NUM-NORM.txt are merged at build time into one page table.
    // NUM-NORM entries take priority (loaded second, overwrite NORM). After
    // fixing the generate script to exclude NUM-NORM overlaps from NORM, both
    // files should produce exact matches.
    let mut failures = Vec::new();
    for (src, expected) in &all_mappings {
        let result = text_process(ProcessType::Normalize, src);
        if result.as_ref() != expected.as_str() {
            failures.push(format!(
                "{src:?} → expected {expected:?}, got {:?}",
                result.as_ref()
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} Normalize failures:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn test_process_map_delete_exhaustive() {
    let codepoints = parse_hex_codepoints(&process_map_dir().join("TEXT-DELETE.txt"));
    assert!(
        codepoints.len() > 1000,
        "expected 1000+ codepoints, got {}",
        codepoints.len()
    );

    let mut failures = Vec::new();
    for ch in &codepoints {
        let input = ch.to_string();
        let result = text_process(ProcessType::Delete, &input);
        if !result.is_empty() {
            failures.push(format!(
                "U+{:04X} ({ch:?}) should be deleted, got {:?}",
                *ch as u32,
                result.as_ref()
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} Delete failures:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn test_process_map_emoji_norm_exhaustive() {
    let mappings = parse_tab_mappings(&process_map_dir().join("EMOJI_NORM.txt"));
    assert!(
        mappings.len() > 500,
        "expected 500+ mappings, got {}",
        mappings.len()
    );

    let mut failures = Vec::new();
    for (src, raw_dst) in &mappings {
        let expected = if raw_dst.is_empty() {
            // Empty value = stripped (modifier/ZWJ codepoints)
            String::new()
        } else {
            // build.rs prepends a space for named emoji
            format!(" {raw_dst}")
        };
        let result = text_process(ProcessType::EmojiNorm, src);
        if result.as_ref() != expected.as_str() {
            failures.push(format!(
                "{src:?} → expected {expected:?}, got {:?}",
                result.as_ref()
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} EmojiNorm failures:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
