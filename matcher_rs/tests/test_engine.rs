use matcher_rs::{ProcessType, SimpleMatcherBuilder};

// ---------------------------------------------------------------------------
// DIRECT_RULE_BIT and PatternDispatch
// ---------------------------------------------------------------------------

#[test]
fn test_direct_vs_indirect_dispatch() {
    // Shared sub-pattern "hello" in both simple and compound rule forces
    // Entries dispatch instead of DirectRule for the shared pattern.
    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, "hello")
        .add_word(ProcessType::None, 2, "hello&world")
        .add_word(ProcessType::None, 3, "hello&earth")
        .build()
        .unwrap();

    let r1 = matcher.process("hello");
    assert_eq!(r1.len(), 1);
    assert_eq!(r1[0].word_id, 1);

    let r2 = matcher.process("hello world");
    let mut ids: Vec<u32> = r2.iter().map(|r| r.word_id).collect();
    ids.sort();
    assert_eq!(ids, vec![1, 2]);

    let r3 = matcher.process("hello world earth");
    assert_eq!(r3.len(), 3);
}

// ---------------------------------------------------------------------------
// Bitmask vs matrix dispatch threshold
// ---------------------------------------------------------------------------

#[test]
fn test_bitmask_to_matrix_threshold() {
    // 64 unique AND segments → bitmask path (exactly at capacity)
    let parts_64: Vec<String> = (0..64).map(|i| format!("w{i}")).collect();
    let matcher_64 = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, parts_64.join("&"))
        .build()
        .unwrap();

    let text_64 = parts_64.join(" ");
    assert!(matcher_64.is_match(&text_64));
    assert!(!matcher_64.is_match(&parts_64[..63].join(" ")));

    // 65 unique AND segments → matrix fallback
    let parts_65: Vec<String> = (0..65).map(|i| format!("w{i}")).collect();
    let matcher_65 = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, parts_65.join("&"))
        .build()
        .unwrap();

    let text_65 = parts_65.join(" ");
    assert!(matcher_65.is_match(&text_65));
    assert!(!matcher_65.is_match(&parts_65[..64].join(" ")));

    // 63 AND + 1 NOT = 64 total → bitmask path (at capacity)
    let and_63: Vec<String> = (0..63).map(|i| format!("w{i}")).collect();
    let m_63_not = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, format!("{}~veto", and_63.join("&")))
        .build()
        .unwrap();

    assert!(m_63_not.is_match(&and_63.join(" ")));
    assert!(!m_63_not.is_match(&format!("{} veto", and_63.join(" "))));

    // 63 AND + 2 NOT = 65 total > capacity → matrix
    let m_63_2not = SimpleMatcherBuilder::new()
        .add_word(
            ProcessType::None,
            1,
            format!("{}~notX~notY", and_63.join("&")),
        )
        .build()
        .unwrap();

    assert!(m_63_2not.is_match(&and_63.join(" ")));
    assert!(!m_63_2not.is_match(&format!("{} notX", and_63.join(" "))));
}

#[test]
fn test_matrix_repeated_segments() {
    // "a&a&b&b&b" → and_splits: {a:2, b:3} → matrix (counts != 1)
    let m1 = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, "a&a&b&b&b")
        .build()
        .unwrap();

    assert!(m1.is_match("a a b b b"));
    assert!(!m1.is_match("a b b b"), "only 1a");
    assert!(!m1.is_match("a a b b"), "only 2b");

    // "a~b~b" → threshold-based NOT veto: fires only when "b" appears 2×
    let m2 = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, "a~b~b")
        .build()
        .unwrap();

    assert!(m2.is_match("a"));
    assert!(m2.is_match("a b"), "1b below threshold");
    assert!(!m2.is_match("a b b"), "2b triggers veto");
    assert!(!m2.is_match("a b b b"), "3b triggers veto");
}

// ---------------------------------------------------------------------------
// Pattern indexing under Delete
// ---------------------------------------------------------------------------

#[test]
fn test_delete_adjusted_pattern_indexing() {
    // Pattern stored verbatim in AC; text is delete-stripped before scan.
    let m1 = SimpleMatcherBuilder::new()
        .add_word(ProcessType::Delete, 1, "ab")
        .build()
        .unwrap();

    assert!(m1.is_match("ab"));
    assert!(m1.is_match("a*b"));
    assert!(m1.is_match("a b"));
    assert!(!m1.is_match("ac"));

    // VariantNorm|Delete composite indexing
    let m2 = SimpleMatcherBuilder::new()
        .add_word(ProcessType::VariantNorm | ProcessType::Delete, 1, "测试")
        .build()
        .unwrap();

    assert!(m2.is_match("测试"), "simplified direct");
    assert!(m2.is_match("測試"), "traditional → VariantNorm");
    assert!(m2.is_match("测！试"), "simplified + noise → Delete");
    assert!(m2.is_match("測！試"), "traditional + noise → both");
}

#[test]
fn test_delete_scans_original_text() {
    // Delete is non-bijective: patterns are stored verbatim, so the original
    // (pre-Delete) text must also be scanned. Patterns containing deletable
    // characters can only match via the original-text scan.

    // Pattern "a*b" contains deletable '*'. It can only match inputs that
    // literally contain "a*b" (via original-text scan).
    let m = SimpleMatcherBuilder::new()
        .add_word(ProcessType::Delete, 1, "a*b")
        .build()
        .unwrap();

    assert!(m.is_match("a*b"), "literal match via original-text scan");
    assert!(m.is_match("xa*by"), "substring match in original text");
    assert!(!m.is_match("ab"), "delete-stripped text has no '*'");
    assert!(!m.is_match("a b"), "pattern '*' ≠ space");

    // Pattern "hello world" contains deletable space. Only matchable when
    // the input literally contains "hello world".
    let m2 = SimpleMatcherBuilder::new()
        .add_word(ProcessType::Delete, 1, "hello world")
        .build()
        .unwrap();

    assert!(m2.is_match("hello world"), "original text matches directly");
    assert!(
        !m2.is_match("helloworld"),
        "pattern has space, text doesn't"
    );

    // Pattern without deletable chars works via both scans.
    let m3 = SimpleMatcherBuilder::new()
        .add_word(ProcessType::Delete, 1, "ab")
        .build()
        .unwrap();

    assert!(m3.is_match("ab"), "exact match in original");
    assert!(m3.is_match("a*b"), "delete-transformed 'a*b' → 'ab'");
    assert!(m3.is_match("a b"), "delete-transformed 'a b' → 'ab'");
}

#[test]
fn test_none_redundant_in_composites() {
    // None combined with any transform is redundant and silently stripped.
    // None|Delete should behave identically to Delete alone.
    let m_delete = SimpleMatcherBuilder::new()
        .add_word(ProcessType::Delete, 1, "helloworld")
        .build()
        .unwrap();

    let m_none_delete = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None | ProcessType::Delete, 1, "helloworld")
        .build()
        .unwrap();

    for input in ["helloworld", "hello world", "hello-world", "hello  world"] {
        assert_eq!(
            m_delete.is_match(input),
            m_none_delete.is_match(input),
            "Delete and None|Delete must agree on {input:?}"
        );
    }
    assert!(!m_delete.is_match("hallo"));
    assert!(!m_none_delete.is_match("hallo"));

    // None|VariantNorm should behave identically to VariantNorm alone.
    let m_vn = SimpleMatcherBuilder::new()
        .add_word(ProcessType::VariantNorm, 1, "测试")
        .build()
        .unwrap();

    let m_none_vn = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None | ProcessType::VariantNorm, 1, "测试")
        .build()
        .unwrap();

    for input in ["测试", "測試"] {
        assert_eq!(
            m_vn.is_match(input),
            m_none_vn.is_match(input),
            "VariantNorm and None|VariantNorm must agree on {input:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// Engine routing: density-based dispatch
// ---------------------------------------------------------------------------

#[test]
fn test_density_dispatch() {
    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, "needle")
        .add_word(ProcessType::None, 2, "针")
        .build()
        .unwrap();

    // Low density (mostly ASCII → bytewise path)
    let low = format!("needle 针 {}", "a".repeat(50));
    let ids_low: Vec<u32> = matcher.process(&low).iter().map(|r| r.word_id).collect();
    assert!(ids_low.contains(&1), "low density: needle found");
    assert!(ids_low.contains(&2), "low density: 针 found");

    // High density (mostly CJK → charwise path)
    let high = format!("{}needle{}", "你好世界测试国语中文", "你好世界测试国语中文");
    assert!(matcher.is_match(&high));
    let ids_high: Vec<u32> = matcher.process(&high).iter().map(|r| r.word_id).collect();
    assert!(ids_high.contains(&1), "high density: needle found");

    // All-ASCII patterns: ASCII pattern still found in mixed text
    let ascii_only = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, "hello")
        .add_word(ProcessType::None, 2, "world")
        .build()
        .unwrap();

    assert!(ascii_only.is_match("hello 世界"));
    assert!(
        !ascii_only.is_match("你好世界"),
        "no ASCII pattern in CJK text"
    );

    let r = ascii_only.process("hello 世界 world");
    let mut ids: Vec<u32> = r.into_iter().map(|r| r.word_id).collect();
    ids.sort();
    assert_eq!(ids, vec![1, 2]);
}

// ---------------------------------------------------------------------------
// AC automaton: overlapping patterns
// ---------------------------------------------------------------------------

#[test]
fn test_overlapping_patterns() {
    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, "hello")
        .add_word(ProcessType::None, 2, "hello world")
        .add_word(ProcessType::None, 3, "world")
        .build()
        .unwrap();

    let mut ids: Vec<u32> = matcher
        .process("hello world")
        .into_iter()
        .map(|r| r.word_id)
        .collect();
    ids.sort();
    assert_eq!(ids, vec![1, 2, 3]);
}

// ---------------------------------------------------------------------------
// Parallel engine compilation (large pattern sets)
// ---------------------------------------------------------------------------

#[test]
fn test_large_pattern_set_compilation() {
    // 150 ASCII + 150 CJK patterns → parallel engine construction
    let ascii: Vec<String> = (0..150u32).map(|i| format!("ascii{i:03}")).collect();
    let cjk: Vec<String> = (0..150u32).map(|i| format!("测试{i:03}")).collect();
    let mut builder = SimpleMatcherBuilder::new();
    for (i, w) in ascii.iter().enumerate() {
        builder = builder.add_word(ProcessType::None, i as u32, w);
    }
    for (i, w) in cjk.iter().enumerate() {
        builder = builder.add_word(ProcessType::None, i as u32 + 1000, w);
    }
    let matcher = builder.build().unwrap();

    assert!(matcher.is_match("ascii042"));
    assert!(matcher.is_match("测试099"));
    assert!(!matcher.is_match("missing"));

    let results = matcher.process("ascii000 测试000 some text");
    let mut ids: Vec<u32> = results.iter().map(|r| r.word_id).collect();
    ids.sort();
    assert!(ids.contains(&0));
    assert!(ids.contains(&1000));
}

// ---------------------------------------------------------------------------
// DFA/charwise streaming iteration paths
// ---------------------------------------------------------------------------

#[test]
fn test_dfa_and_charwise_streaming() {
    // ASCII patterns under VariantNorm → DFA streaming path
    let words: Vec<String> = (0..100u32).map(|i| format!("word{i:03}")).collect();
    let mut builder = SimpleMatcherBuilder::new();
    for (i, w) in words.iter().enumerate() {
        builder = builder.add_word(ProcessType::VariantNorm, i as u32, w);
    }
    let m1 = builder.build().unwrap();
    assert!(m1.is_match("word042"));
    assert_eq!(m1.process("word000 word099").len(), 2);

    // Non-ASCII patterns under VariantNorm|Delete → charwise streaming path
    let m2 = SimpleMatcherBuilder::new()
        .add_word(ProcessType::VariantNorm | ProcessType::Delete, 1, "测试")
        .add_word(ProcessType::VariantNorm | ProcessType::Delete, 2, "你好")
        .build()
        .unwrap();

    assert!(m2.is_match("測！試"));
    assert!(m2.is_match("你！好"));
    assert_eq!(m2.process("測！試 你！好").len(), 2);
}

// ---------------------------------------------------------------------------
// Sequential matcher reuse (thread-local state isolation)
// ---------------------------------------------------------------------------

#[test]
fn test_sequential_matcher_reuse() {
    let matcher_a = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, "alpha")
        .add_word(ProcessType::None, 2, "beta")
        .build()
        .unwrap();

    let matcher_b = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 10, "gamma")
        .add_word(ProcessType::None, 20, "delta")
        .build()
        .unwrap();

    // Use matcher_a
    assert!(matcher_a.is_match("alpha beta"));
    assert_eq!(matcher_a.process("alpha beta").len(), 2);

    // Switch to matcher_b — no state leakage
    assert!(!matcher_b.is_match("alpha"));
    assert!(matcher_b.is_match("gamma delta"));
    assert_eq!(matcher_b.process("gamma delta").len(), 2);

    // Back to matcher_a — still works
    assert!(matcher_a.is_match("alpha"));
    assert!(!matcher_a.is_match("gamma"));
}

// ---------------------------------------------------------------------------
// Mixed ASCII/CJK patterns
// ---------------------------------------------------------------------------

#[test]
fn test_mixed_ascii_cjk_rules() {
    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, "abc")
        .add_word(ProcessType::None, 2, "你好")
        .build()
        .unwrap();

    let mut ids: Vec<u32> = matcher
        .process("你好 abc")
        .into_iter()
        .map(|r| r.word_id)
        .collect();
    ids.sort_unstable();
    assert_eq!(ids, vec![1, 2]);
}
