use super::*;

fn make_patterns(words: &[&str]) -> Vec<(String, u32)> {
    words
        .iter()
        .enumerate()
        .map(|(i, &word)| (word.to_owned(), i as u32))
        .collect()
}

fn refs(patterns: &[(String, u32)]) -> Vec<(&str, u32)> {
    patterns
        .iter()
        .map(|(pattern, value)| (pattern.as_str(), *value))
        .collect()
}

fn big_set() -> Vec<(String, u32)> {
    (0u32..64).map(|i| (format!("token{i:02}"), i)).collect()
}

fn collect_unique_hits(matcher: &HarryMatcher, haystack: &str) -> Vec<u32> {
    let mut hits = Vec::new();
    matcher.for_each_match_value(haystack, |value| {
        hits.push(value);
        false
    });
    hits.sort_unstable();
    hits.dedup();
    hits
}

fn collect_naive_hits(patterns: &[(String, u32)], haystack: &str) -> Vec<u32> {
    let mut hits: Vec<u32> = patterns
        .iter()
        .filter(|(pattern, _)| haystack.contains(pattern.as_str()))
        .map(|(_, value)| *value)
        .collect();
    hits.sort_unstable();
    hits.dedup();
    hits
}

#[test]
fn build_rejects_small_sets() {
    let patterns = make_patterns(&["hello", "world"]);
    assert!(HarryMatcher::build(&refs(&patterns)).is_none());
}

#[test]
fn build_rejects_all_single_byte_sets() {
    let patterns: Vec<(String, u32)> = (0u8..64)
        .map(|i| ((char::from(b'!' + i)).to_string(), i as u32))
        .collect();
    assert!(HarryMatcher::build(&refs(&patterns)).is_none());
}

#[test]
fn build_accepts_large_ascii_set() {
    let patterns = big_set();
    assert!(HarryMatcher::build(&refs(&patterns)).is_some());
}

#[test]
fn build_accepts_large_cjk_set() {
    let patterns: Vec<(String, u32)> = (0u32..64).map(|i| (format!("测试词{i:02}"), i)).collect();
    assert!(HarryMatcher::build(&refs(&patterns)).is_some());
}

#[test]
fn build_accepts_mixed_ascii_cjk_set() {
    let mut patterns = big_set(); // 64 ASCII patterns
    patterns.extend((0u32..32).map(|i| (format!("词语{i:02}"), i + 100)));
    assert!(HarryMatcher::build(&refs(&patterns)).is_some());
}

#[test]
fn build_accepts_mixed_single_and_multi_byte_set() {
    let mut patterns: Vec<(String, u32)> = (0u8..40)
        .map(|i| ((char::from(b'!' + i)).to_string(), i as u32))
        .collect();
    patterns.extend((0u32..32).map(|i| (format!("word{i:02}"), i + 100)));
    assert!(HarryMatcher::build(&refs(&patterns)).is_some());
}

#[test]
fn is_match_basic() {
    let patterns = big_set();
    let matcher = HarryMatcher::build(&refs(&patterns)).unwrap();
    assert!(matcher.is_match("prefix token42 suffix"));
    assert!(!matcher.is_match("nothing here at all!!"));
}

#[test]
fn for_each_match_value_collects_all_hits() {
    let patterns = big_set();
    let matcher = HarryMatcher::build(&refs(&patterns)).unwrap();
    let mut hits = Vec::new();

    let stopped = matcher.for_each_match_value("token01 token42 token63", |value| {
        hits.push(value);
        false
    });

    assert!(!stopped);
    hits.sort_unstable();
    assert_eq!(hits, vec![1, 42, 63]);
}

#[test]
fn early_exit_returns_true() {
    let patterns = big_set();
    let matcher = HarryMatcher::build(&refs(&patterns)).unwrap();
    let mut count = 0usize;

    let stopped = matcher.for_each_match_value("token00 token01 token02", |_| {
        count += 1;
        count >= 1
    });

    assert!(stopped);
    assert_eq!(count, 1);
}

#[test]
fn matches_long_pattern_via_prefix_filter() {
    let mut patterns = big_set();
    patterns.push(("averyverylongliteral".to_owned(), 999));
    let matcher = HarryMatcher::build(&refs(&patterns)).unwrap();

    let hits = collect_unique_hits(&matcher, "xx averyverylongliteral yy");
    assert!(hits.contains(&999));
}

#[test]
fn single_byte_literals_still_match() {
    let mut patterns = big_set();
    patterns.push(("x".to_owned(), 999));
    patterns.push(("z".to_owned(), 1000));
    let matcher = HarryMatcher::build(&refs(&patterns)).unwrap();

    let hits = collect_unique_hits(&matcher, "x token00 yz");
    assert!(hits.contains(&999));
    assert!(hits.contains(&1000));
    assert!(hits.contains(&0));
}

#[test]
fn encoding_collision_is_filtered_by_exact_match() {
    let mut patterns = big_set();
    patterns.push(("pq".to_owned(), 999));
    let matcher = HarryMatcher::build(&refs(&patterns)).unwrap();

    let hits = collect_unique_hits(&matcher, "0q");
    assert!(!hits.contains(&999));
}

#[test]
fn grouped_bucket_false_positive_is_filtered_by_exact_match() {
    let mut patterns = big_set();
    patterns.push(("ab".to_owned(), 999));
    patterns.push(("ij".to_owned(), 1000));
    let matcher = HarryMatcher::build(&refs(&patterns)).unwrap();

    let hits = collect_unique_hits(&matcher, "aj");
    assert!(!hits.contains(&999));
    assert!(!hits.contains(&1000));
}

#[test]
fn handles_simd_chunk_boundaries() {
    let mut patterns = big_set();
    patterns.push(("boundaryxx".to_owned(), 999));
    let matcher = HarryMatcher::build(&refs(&patterns)).unwrap();

    let haystack = format!("{}boundaryxx{}", "a".repeat(17), "b".repeat(23));
    let hits = collect_unique_hits(&matcher, haystack.as_str());
    assert!(hits.contains(&999));
}

#[test]
fn no_false_negatives_vs_naive_for_mixed_lengths() {
    let mut patterns: Vec<(String, u32)> = (0u32..64).map(|i| (format!("pat{i:03}"), i)).collect();
    patterns.push(("x".to_owned(), 900));
    patterns.push(("averyverylongliteral".to_owned(), 901));
    patterns.push(("pq".to_owned(), 902));
    patterns.push(("ij".to_owned(), 903));
    let matcher = HarryMatcher::build(&refs(&patterns)).unwrap();

    let haystack = "xxpat000yyxzzpat031aa averyverylongliteral 0q aj pat063 end";

    let harry = collect_unique_hits(&matcher, haystack);
    let naive = collect_naive_hits(&patterns, haystack);

    assert_eq!(harry, naive);
}

#[test]
fn randomized_parity_against_naive() {
    fn next_u32(state: &mut u64) -> u32 {
        *state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
        (*state >> 32) as u32
    }

    let alphabet = *b"0pqrijabXYZtokenLM";
    let mut seen = std::collections::HashSet::new();
    let mut patterns = Vec::new();
    let mut seed = 1u64;
    let mut next_value = 0u32;

    while patterns.len() < 96 {
        let len_roll = (next_u32(&mut seed) % 10) as usize;
        let len = match len_roll {
            0 => 1,
            1..=7 => len_roll + 1,
            _ => 8 + (next_u32(&mut seed) % 5) as usize,
        };
        let mut bytes = Vec::with_capacity(len);
        for _ in 0..len {
            let idx = (next_u32(&mut seed) as usize) % alphabet.len();
            bytes.push(alphabet[idx]);
        }
        let pattern = String::from_utf8(bytes).unwrap();
        if seen.insert(pattern.clone()) {
            patterns.push((pattern, next_value));
            next_value += 1;
        }
    }

    let matcher = HarryMatcher::build(&refs(&patterns)).unwrap();

    let mut haystack = String::with_capacity(1024);
    for _ in 0..1024 {
        let idx = (next_u32(&mut seed) as usize) % alphabet.len();
        haystack.push(alphabet[idx] as char);
    }
    haystack.push_str("averyverylongliteral");
    haystack.push_str("0q");
    haystack.push_str("aj");

    let harry = collect_unique_hits(&matcher, haystack.as_str());
    let naive = collect_naive_hits(&patterns, haystack.as_str());
    assert_eq!(harry, naive, "Harry missed a match vs naive scan");
}

#[test]
fn ascii_patterns_do_not_match_cjk_haystack() {
    // ASCII patterns have no bytes in common with CJK UTF-8 sequences,
    // so is_match must return false even without a haystack guard.
    let patterns = big_set();
    let matcher = HarryMatcher::build(&refs(&patterns)).unwrap();
    assert!(!matcher.is_match("日本語テキスト"));
}

#[test]
fn cjk_patterns_match_cjk_haystack() {
    let mut patterns = big_set(); // filler to reach ≥64
    patterns.push(("你好世界".to_owned(), 900));
    let matcher = HarryMatcher::build(&refs(&patterns)).unwrap();
    assert!(matcher.is_match("这是一段测试文本你好世界结尾"));
    assert!(!matcher.is_match("this is ascii only text"));
}

#[test]
fn cjk_patterns_no_false_negatives_vs_naive() {
    let mut patterns: Vec<(String, u32)> = (0u32..64).map(|i| (format!("模式{i:02}"), i)).collect();
    patterns.push(("关键词".to_owned(), 900));
    let matcher = HarryMatcher::build(&refs(&patterns)).unwrap();

    let haystack = "这段文本包含关键词还有模式00以及模式31等等";
    let harry = collect_unique_hits(&matcher, haystack);
    let naive = collect_naive_hits(&patterns, haystack);
    assert_eq!(harry, naive, "Harry CJK missed a match vs naive scan");
}

/// Harry fires on_value once per matching *position*, not once per unique pattern.
/// For overlapping occurrences (e.g. "aa" in "aaa"), the callback is called
/// once per start position that produces a hit.
#[test]
fn overlapping_matches_reported_per_position() {
    // Build a set large enough for HarryMatcher::build to succeed.
    // We include the two patterns we actually care about in the overlap test.
    let mut patterns = big_set(); // 64 filler patterns
    patterns.push(("aa".to_owned(), 900)); // 2-char overlap candidate
    patterns.push(("aab".to_owned(), 901)); // longer pattern starting same way
    let refs: Vec<(&str, u32)> = patterns.iter().map(|(p, v)| (p.as_str(), *v)).collect();
    let matcher = HarryMatcher::build(&refs).unwrap();

    // "aaa" contains "aa" at position 0 and position 1 — both overlapping.
    let mut calls_900 = 0usize;
    let mut calls_901 = 0usize;
    matcher.for_each_match_value("aaa", |v| {
        if v == 900 {
            calls_900 += 1;
        }
        if v == 901 {
            calls_901 += 1;
        }
        false
    });
    assert_eq!(
        calls_900, 2,
        "\"aa\" should match at both position 0 and 1 in \"aaa\""
    );
    assert_eq!(calls_901, 0, "\"aab\" should not match in \"aaa\"");

    // "aab" contains "aa" at position 0 and "aab" at position 0.
    let mut calls_900 = 0usize;
    let mut calls_901 = 0usize;
    matcher.for_each_match_value("aab", |v| {
        if v == 900 {
            calls_900 += 1;
        }
        if v == 901 {
            calls_901 += 1;
        }
        false
    });
    assert_eq!(
        calls_900, 1,
        "\"aa\" should match once in \"aab\" (position 0)"
    );
    assert_eq!(
        calls_901, 1,
        "\"aab\" should match once in \"aab\" (position 0)"
    );

    // "aabaab" — "aa" appears at positions 0 and 3; "aab" at positions 0 and 3.
    let mut calls_900 = 0usize;
    let mut calls_901 = 0usize;
    matcher.for_each_match_value("aabaab", |v| {
        if v == 900 {
            calls_900 += 1;
        }
        if v == 901 {
            calls_901 += 1;
        }
        false
    });
    assert_eq!(
        calls_900, 2,
        "\"aa\" should match at positions 0 and 3 in \"aabaab\""
    );
    assert_eq!(
        calls_901, 2,
        "\"aab\" should match at positions 0 and 3 in \"aabaab\""
    );
}

#[test]
fn ascii_patterns_skip_pure_cjk_haystack() {
    let patterns = big_set(); // all ASCII
    let matcher = HarryMatcher::build(&refs(&patterns)).unwrap();
    assert!(matcher.all_patterns_ascii);
    assert!(!matcher.is_match("日本語テキスト中文测试文本韩国语"));
}

#[test]
fn ascii_patterns_find_in_mixed_haystack() {
    let patterns = big_set(); // token00..token63
    let matcher = HarryMatcher::build(&refs(&patterns)).unwrap();
    assert!(matcher.all_patterns_ascii);

    // Embed ASCII match targets inside CJK text.
    assert!(matcher.is_match("中文token42中文"));
    assert!(matcher.is_match("token00"));
    assert!(matcher.is_match("中文中文中文token63中文"));
    assert!(!matcher.is_match("中文中文中文中文"));
}

#[test]
fn ascii_patterns_boundary_alignment() {
    let mut patterns = big_set();
    patterns.push(("needle".to_owned(), 999));
    let matcher = HarryMatcher::build(&refs(&patterns)).unwrap();

    // Test match at various offsets mod 16 in a CJK-padded haystack.
    // Each CJK char is 3 UTF-8 bytes; "needle" is 6 bytes.
    for pad_chars in 0..20 {
        let prefix: String = std::iter::repeat('中').take(pad_chars).collect();
        let haystack = format!("{prefix}needle");
        let hits = collect_unique_hits(&matcher, &haystack);
        assert!(
            hits.contains(&999),
            "missed 'needle' at byte offset {} (pad_chars={pad_chars})",
            prefix.len()
        );
    }
}

#[test]
fn ascii_flag_not_set_for_mixed_patterns() {
    let mut patterns = big_set();
    patterns.push(("中文".to_owned(), 999));
    let matcher = HarryMatcher::build(&refs(&patterns)).unwrap();
    assert!(!matcher.all_patterns_ascii);

    // Should still find both ASCII and CJK matches.
    assert!(matcher.is_match("token00"));
    assert!(matcher.is_match("中文测试"));
}

#[test]
fn ascii_parity_against_naive_on_mixed_haystack() {
    let patterns: Vec<(String, u32)> = (0u32..96).map(|i| (format!("word{i:03}"), i)).collect();
    let matcher = HarryMatcher::build(&refs(&patterns)).unwrap();
    assert!(matcher.all_patterns_ascii);

    let haystack = "中文word000中文word031中文word095日本語テキストword050end";
    let harry = collect_unique_hits(&matcher, haystack);
    let naive = collect_naive_hits(&patterns, haystack);
    assert_eq!(harry, naive, "ASCII fast path missed a match vs naive");
}

#[test]
fn short_patterns_use_fewer_columns() {
    // All patterns are 2-3 bytes → max_prefix_len should be 3.
    let patterns: Vec<(String, u32)> = (0u32..64)
        .map(|i| {
            if i < 32 {
                (format!("a{}", (b'A' + (i as u8)) as char), i)
            } else {
                (format!("z{}{}", (b'A' + (i as u8 - 32)) as char, 'x'), i)
            }
        })
        .collect();
    let matcher = HarryMatcher::build(&refs(&patterns)).unwrap();
    assert_eq!(matcher.max_prefix_len, 3);

    // Verify correctness still holds.
    let harry = collect_unique_hits(&matcher, "aA zAx aBnothere");
    let naive = collect_naive_hits(&patterns, "aA zAx aBnothere");
    assert_eq!(harry, naive, "Short-pattern scan missed match vs naive");
}
