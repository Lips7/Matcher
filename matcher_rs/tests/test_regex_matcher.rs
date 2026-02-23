use matcher_rs::{ProcessType, RegexMatchType, RegexMatcher, RegexTable, TextMatcherTrait};

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

    let mut via_process: Vec<u32> = matcher
        .process(text)
        .into_iter()
        .map(|r| r.word_id)
        .collect();
    let mut via_iter: Vec<u32> = matcher.process_iter(text).map(|r| r.word_id).collect();

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
    let via_process: Vec<u32> = matcher
        .process(text)
        .into_iter()
        .map(|r| r.word_id)
        .collect();
    let via_iter: Vec<u32> = matcher.process_iter(text).map(|r| r.word_id).collect();
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
