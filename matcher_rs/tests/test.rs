mod test_simple {
    use std::collections::HashMap;

    use matcher_rs::{SimpleMatchType, SimpleMatcher, TextMatcherTrait};

    #[test]
    fn simple_match_init() {
        let _ = SimpleMatcher::new(&HashMap::from([(
            SimpleMatchType::None,
            HashMap::from([(1, "")]),
        )]));
        let _ = SimpleMatcher::new(&HashMap::from([(
            SimpleMatchType::None,
            HashMap::from([(1, "hello"), (2, "world")]),
        )]));
    }

    #[test]
    fn simple_match_fanjian() {
        let simple_matcher = SimpleMatcher::new(&HashMap::from([(
            SimpleMatchType::Fanjian,
            HashMap::from([(1, "你好")]),
        )]));
        assert!(simple_matcher.is_match("妳好"));

        let simple_matcher = SimpleMatcher::new(&HashMap::from([(
            SimpleMatchType::Fanjian,
            HashMap::from([(1, "妳好")]),
        )]));
        assert!(simple_matcher.is_match("你好"));
    }

    #[test]
    fn simple_match_delete() {
        let simple_matcher = SimpleMatcher::new(&HashMap::from([(
            SimpleMatchType::Delete,
            HashMap::from([(1, "你好")]),
        )]));
        assert!(simple_matcher.is_match("你！好"));
    }

    #[test]
    fn simple_match_normalize() {
        let simple_matcher = SimpleMatcher::new(&HashMap::from([(
            SimpleMatchType::Normalize,
            HashMap::from([(1, "he11o")]),
        )]));
        assert!(simple_matcher.is_match("ℋЀ⒈㈠ϕ"));
    }

    #[test]
    fn simple_match_pinyin() {
        let simple_matcher = SimpleMatcher::new(&HashMap::from([(
            SimpleMatchType::PinYin,
            HashMap::from([(1, "西安")]),
        )]));
        assert!(simple_matcher.is_match("洗按"));
        assert!(!simple_matcher.is_match("现"));
    }

    #[test]
    fn simple_match_pinyinchar() {
        let simple_matcher = SimpleMatcher::new(&HashMap::from([(
            SimpleMatchType::PinYinChar,
            HashMap::from([(1, "西安")]),
        )]));
        assert!(simple_matcher.is_match("洗按"));
        assert!(simple_matcher.is_match("现"));
        assert!(simple_matcher.is_match("xian"));
    }
}

mod test_regex {
    use matcher_rs::{RegexMatchType, RegexMatcher, RegexTable, TextMatcherTrait};

    #[test]
    fn regex_match_regex() {
        let regex_matcher = RegexMatcher::new(&[RegexTable {
            table_id: 1,
            match_id: 1,
            regex_match_type: RegexMatchType::Regex,
            word_list: &vec!["h[aeiou]llo", "w[aeiou]rd"],
        }]);

        assert!(regex_matcher.is_match("hallo"));
        assert!(regex_matcher.is_match("ward"));
    }

    #[test]
    fn regex_match_acrostic() {
        let regex_matcher = RegexMatcher::new(&[RegexTable {
            table_id: 1,
            match_id: 1,
            regex_match_type: RegexMatchType::Acrostic,
            word_list: &vec!["h,e,l,l,o", "你,好"],
        }]);

        assert!(regex_matcher.is_match("hope, endures, love, lasts, onward."));
        assert!(regex_matcher.is_match("Happy moments shared, Every smile and laugh, Love in every word, Lighting up our paths, Open hearts we show."));
        assert!(regex_matcher.is_match("你的笑容温暖, 好心情常伴。"));
    }

    #[test]
    fn rege_match_similar_char() {
        let regex_matcher = RegexMatcher::new(&[RegexTable {
            table_id: 1,
            match_id: 1,
            regex_match_type: RegexMatchType::SimilarChar,
            word_list: &vec!["hello,hi,H,你好", "world,word,🌍,世界"],
        }]);

        assert!(regex_matcher.is_match("helloworld"));
        assert!(regex_matcher.is_match("hi世界"));
    }
}

mod test_sim {
    use matcher_rs::{SimMatchType, SimMatcher, SimTable, TextMatcherTrait};

    #[test]
    fn sim_match() {
        let sim_matcher = SimMatcher::new(&[SimTable {
            table_id: 1,
            match_id: 1,
            sim_match_type: SimMatchType::Levenshtein,
            word_list: &vec!["helloworld"],
            threshold: 0.8,
        }]);

        assert!(sim_matcher.is_match("helloworl"));
        assert!(sim_matcher.is_match("halloworld"));
        assert!(sim_matcher.is_match("ha1loworld"));
        assert!(!sim_matcher.is_match("ha1loworld1"));
    }
}

mod test_matcher {
    use std::collections::HashMap;

    use matcher_rs::{MatchTable, MatchTableType, Matcher, SimpleMatchType, TextMatcherTrait};

    #[test]
    fn matcher_init() {
        let _ = Matcher::new(&HashMap::from([(1, vec![])]));
        let _ = Matcher::new(&HashMap::from([(
            1,
            vec![MatchTable {
                table_id: 1,
                match_table_type: MatchTableType::Simple {
                    simple_match_type: SimpleMatchType::None,
                },
                word_list: vec![],
                exemption_simple_match_type: SimpleMatchType::None,
                exemption_word_list: vec![],
            }],
        )]));
    }

    #[test]
    fn matcher_exemption() {
        let matcher = Matcher::new(&HashMap::from([(
            1,
            vec![MatchTable {
                table_id: 1,
                match_table_type: MatchTableType::Simple {
                    simple_match_type: SimpleMatchType::None,
                },
                word_list: vec!["hello"],
                exemption_simple_match_type: SimpleMatchType::None,
                exemption_word_list: vec!["world"],
            }],
        )]));
        assert!(matcher.is_match("hello"));
        assert!(!matcher.is_match("hello,world"))
    }
}
