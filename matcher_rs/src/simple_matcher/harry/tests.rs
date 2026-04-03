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

// These tests access private fields (all_patterns_ascii, max_prefix_len)
// and must remain as unit tests.

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

    assert!(matcher.is_match("中文token42中文"));
    assert!(matcher.is_match("token00"));
    assert!(matcher.is_match("中文中文中文token63中文"));
    assert!(!matcher.is_match("中文中文中文中文"));
}

#[test]
fn ascii_flag_not_set_for_mixed_patterns() {
    let mut patterns = big_set();
    patterns.push(("中文".to_owned(), 999));
    let matcher = HarryMatcher::build(&refs(&patterns)).unwrap();
    assert!(!matcher.all_patterns_ascii);

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

    let harry = collect_unique_hits(&matcher, "aA zAx aBnothere");
    let naive = collect_naive_hits(&patterns, "aA zAx aBnothere");
    assert_eq!(harry, naive, "Short-pattern scan missed match vs naive");
}
