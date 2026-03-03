use matcher_rs::{
    MatchResultTrait, ProcessType, RegexMatchType, RegexMatcher, RegexTable, TextMatcherTrait,
};

#[test]
fn regex_match_regex() {
    let regex_matcher = RegexMatcher::new(&[RegexTable {
        table_id: 1,
        match_id: 1,
        process_type: ProcessType::None,
        regex_match_type: RegexMatchType::Regex,
        word_list: vec!["h[aeiou]llo", "w[aeiou]rd"],
    }]);

    assert!(regex_matcher.is_match("hallo"));
    assert!(regex_matcher.is_match("ward"));
}

#[test]
fn regex_match_acrostic() {
    let regex_matcher = RegexMatcher::new(&[RegexTable {
        table_id: 1,
        match_id: 1,
        process_type: ProcessType::None,
        regex_match_type: RegexMatchType::Acrostic,
        word_list: vec!["h,e,l,l,o", "你,好"],
    }]);

    assert!(regex_matcher.is_match("hope, endures, love, lasts, onward."));
    assert!(regex_matcher.is_match("Happy moments shared, Every smile and laugh, Love in every word, Lighting up our paths, Open hearts we show."));
    assert!(regex_matcher.is_match("你的笑容温暖, 好心情常伴。"));
}

#[test]
fn regex_match_similar_char() {
    let regex_matcher = RegexMatcher::new(&[RegexTable {
        table_id: 1,
        match_id: 1,
        process_type: ProcessType::None,
        regex_match_type: RegexMatchType::SimilarChar,
        word_list: vec!["hello,hi,H,你好", "world,word,🌍,世界"],
    }]);

    assert!(regex_matcher.is_match("helloworld"));
    assert!(regex_matcher.is_match("hi世界"));
}

#[test]
fn regex_process_iter_matches_process() {
    let matcher = RegexMatcher::new(&[RegexTable {
        table_id: 1,
        match_id: 1,
        process_type: ProcessType::None,
        regex_match_type: RegexMatchType::Regex,
        word_list: vec!["h[aeiou]llo", "w[aeiou]rld"],
    }]);

    let text = "hello world hallo";

    let mut via_process: Vec<String> = matcher
        .process(text)
        .into_iter()
        .map(|r| r.word.to_string())
        .collect();
    let mut via_iter: Vec<String> = matcher
        .process_iter(text)
        .map(|r| r.word.to_string())
        .collect();

    via_process.sort();
    via_iter.sort();

    assert_eq!(
        via_process, via_iter,
        "process_iter must yield same word_ids as process"
    );
}

#[test]
fn regex_process_iter_acrostic() {
    let matcher = RegexMatcher::new(&[RegexTable {
        table_id: 1,
        match_id: 1,
        process_type: ProcessType::None,
        regex_match_type: RegexMatchType::Acrostic,
        word_list: vec!["h,e,l,l,o", "你,好"],
    }]);

    let text = "hope, endures, love, lasts, onward.";
    // process_iter should find the same results as process
    let via_process: Vec<String> = matcher
        .process(text)
        .into_iter()
        .map(|r| r.word.to_string())
        .collect();
    let via_iter: Vec<String> = matcher
        .process_iter(text)
        .map(|r| r.word.to_string())
        .collect();
    assert_eq!(via_process, via_iter);
}

#[test]
fn regex_process_iter_empty() {
    let matcher = RegexMatcher::new(&[RegexTable {
        table_id: 1,
        match_id: 1,
        process_type: ProcessType::None,
        regex_match_type: RegexMatchType::Regex,
        word_list: vec!["hello"],
    }]);

    assert_eq!(matcher.process_iter("").count(), 0);
}

#[test]
fn regex_match_invalid_regex_graceful_ignore() {
    // an invalid regex like "[unclosed" should be skipped and not panic.
    let regex_matcher = RegexMatcher::new(&[RegexTable {
        table_id: 1,
        match_id: 1,
        process_type: ProcessType::None,
        regex_match_type: RegexMatchType::Regex,
        word_list: vec!["valid", "[unclosed"],
    }]);

    assert!(regex_matcher.is_match("this is valid"));
    assert!(!regex_matcher.is_match("[unclosed"));
}

#[test]
fn regex_match_long_pattern_skip() {
    // Tests that a pattern over 1024 chars in length is skipped
    // to avoid potential ReDoS.
    let very_long_word = "a".repeat(1050);

    let regex_matcher = RegexMatcher::new(&[RegexTable {
        table_id: 1,
        match_id: 1,
        process_type: ProcessType::None,
        regex_match_type: RegexMatchType::Regex,
        word_list: vec![&very_long_word],
    }]);

    assert!(!regex_matcher.is_match(&very_long_word));
}

#[test]
fn regex_match_regex_set() {
    // Test behavior with multiple regex patterns confirming valid conversion and matching.
    let regex_matcher = RegexMatcher::new(&[RegexTable {
        table_id: 1,
        match_id: 1,
        process_type: ProcessType::None,
        regex_match_type: RegexMatchType::Regex,
        word_list: vec!["alpha", "beta", "gamma"],
    }]);

    let results = regex_matcher.process("beta and gamma");
    let mut words: Vec<String> = results.into_iter().map(|r| r.word().to_string()).collect();
    words.sort();

    assert_eq!(words, vec!["beta", "gamma"]);
}

#[test]
fn regex_match_duplicated_pattern() {
    let regex_matcher = RegexMatcher::new(&[RegexTable {
        table_id: 1,
        match_id: 1,
        process_type: ProcessType::None,
        regex_match_type: RegexMatchType::Regex,
        word_list: vec!["duplicate", "duplicate", "different"],
    }]);

    let results = regex_matcher.process("this is a duplicate pattern");
    let mut words: Vec<String> = results.into_iter().map(|r| r.word.to_string()).collect();
    words.sort();

    // We expect both word_id 0 and 1 to be returned because both have the same pattern "duplicate"
    assert_eq!(words, vec!["duplicate", "duplicate"]);
}
