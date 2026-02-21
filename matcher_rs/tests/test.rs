mod test_simple {
    use std::collections::HashMap;

    use matcher_rs::{ProcessType, SimpleMatcher, SimpleWord, TextMatcherTrait};

    #[test]
    fn simple_match_init() {
        let _ = SimpleMatcher::new(&HashMap::from([(
            ProcessType::None,
            HashMap::from([(1, "")]),
        )]));
        let _ = SimpleMatcher::new(&HashMap::from([(
            ProcessType::None,
            HashMap::from([(1, "hello"), (2, "world")]),
        )]));
        // Boundary conditions
        let empty_map: HashMap<ProcessType, HashMap<u32, &str>> = HashMap::new();
        let empty_matcher = SimpleMatcher::new(&empty_map);
        assert!(!empty_matcher.is_match("test"));
        assert!(!empty_matcher.is_match(""));
    }

    #[test]
    fn simple_match_fanjian() {
        let simple_matcher = SimpleMatcher::new(&HashMap::from([(
            ProcessType::Fanjian,
            HashMap::from([(1, "你好")]),
        )]));
        assert!(simple_matcher.is_match("妳好"));

        let simple_matcher = SimpleMatcher::new(&HashMap::from([(
            ProcessType::Fanjian,
            HashMap::from([(1, "妳好")]),
        )]));
        assert!(simple_matcher.is_match("你好"));
    }

    #[test]
    fn simple_match_delete() {
        let simple_matcher = SimpleMatcher::new(&HashMap::from([(
            ProcessType::Delete,
            HashMap::from([(1, "你好")]),
        )]));
        assert!(simple_matcher.is_match("你！好"));
    }

    #[test]
    fn simple_match_normalize() {
        let simple_matcher = SimpleMatcher::new(&HashMap::from([(
            ProcessType::Normalize,
            HashMap::from([(1, "he11o")]),
        )]));
        assert!(simple_matcher.is_match("ℋЀ⒈㈠Õ"));
    }

    #[test]
    fn simple_match_pinyin() {
        let simple_matcher = SimpleMatcher::new(&HashMap::from([(
            ProcessType::PinYin,
            HashMap::from([(1, "西安")]),
        )]));
        assert!(simple_matcher.is_match("洗按"));
        assert!(!simple_matcher.is_match("现"));
    }

    #[test]
    fn simple_match_pinyinchar() {
        let simple_matcher = SimpleMatcher::new(&HashMap::from([(
            ProcessType::PinYinChar,
            HashMap::from([(1, "西安")]),
        )]));
        assert!(simple_matcher.is_match("洗按"));
        assert!(simple_matcher.is_match("现"));
        assert!(simple_matcher.is_match("xian"));
    }

    #[test]
    fn simple_match_combination() {
        let simple_matcher = SimpleMatcher::new(&HashMap::from([(
            ProcessType::None,
            HashMap::from([
                (1, SimpleWord::from("hello").and("world")),
                (2, SimpleWord::from("hello").and("world").and("hello")),
                (3, SimpleWord::from("hello").not("world")),
                (4, SimpleWord::from("hello").not("world").not("world")),
                (5, SimpleWord::from("hello").and("world").not("word")),
                (
                    6,
                    SimpleWord::from("hello")
                        .and("world")
                        .not("word")
                        .not("word"),
                ),
            ]),
        )]));
        assert!(simple_matcher.is_match("hello world"));
        assert!(simple_matcher.is_match("hello hello world"));
        assert!(simple_matcher.is_match("hello word"));
    }
}

mod test_regex {
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
}

mod test_sim {
    use matcher_rs::{ProcessType, SimMatchType, SimMatcher, SimTable, TextMatcherTrait};

    #[test]
    fn sim_match() {
        let sim_matcher = SimMatcher::new(&[SimTable {
            table_id: 1,
            match_id: 1,
            process_type: ProcessType::None,
            sim_match_type: SimMatchType::Levenshtein,
            word_list: vec!["helloworld"],
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

    use matcher_rs::{MatchTable, MatchTableType, Matcher, ProcessType, TextMatcherTrait};

    #[test]
    fn matcher_init() {
        let _ = Matcher::new(&HashMap::from([(
            1,
            vec![MatchTable {
                table_id: 1,
                match_table_type: MatchTableType::Simple {
                    process_type: ProcessType::None,
                },
                word_list: vec![],
                exemption_process_type: ProcessType::None,
                exemption_word_list: vec![],
            }],
        )]));

        let empty_map: HashMap<u32, Vec<MatchTable<'_>>> = HashMap::new();
        let empty_matcher = Matcher::new(&empty_map);
        assert!(!empty_matcher.is_match("anything"));
        assert!(!empty_matcher.is_match(""));
    }

    #[test]
    fn matcher_exemption() {
        let matcher = Matcher::new(&HashMap::from([(
            1,
            vec![MatchTable {
                table_id: 1,
                match_table_type: MatchTableType::Simple {
                    process_type: ProcessType::None,
                },
                word_list: vec!["hello"],
                exemption_process_type: ProcessType::None,
                exemption_word_list: vec!["world"],
            }],
        )]));
        assert!(matcher.is_match("hello"));
        assert!(!matcher.is_match("hello,world"))
    }
}

mod test_process {
    use id_set::IdSet;
    use matcher_rs::{
        build_process_type_tree, reduce_text_process, reduce_text_process_emit,
        reduce_text_process_with_set, reduce_text_process_with_tree, text_process, ProcessType,
    };

    #[test]
    fn test_text_process() {
        let text = text_process(ProcessType::Fanjian, "~ᗩ~躶~𝚩~軆~Ⲉ~");
        println!("{:?}", text);
    }

    #[test]
    fn test_reduce_text_process() {
        let text = reduce_text_process(ProcessType::FanjianDeleteNormalize, "~ᗩ~躶~𝚩~軆~Ⲉ~");
        println!("{:?}", text);
    }

    #[test]
    fn test_reduce_text_process_emit() {
        let text = reduce_text_process_emit(ProcessType::FanjianDeleteNormalize, "~ᗩ~躶~𝚩~軆~Ⲉ~");
        println!("{:?}", text);
    }

    #[test]
    fn test_build_process_type_tree() {
        let process_type_set = IdSet::from_iter([
            ProcessType::Fanjian.bits() as usize,
            ProcessType::DeleteNormalize.bits() as usize,
            ProcessType::FanjianDeleteNormalize.bits() as usize,
            ProcessType::Delete.bits() as usize,
            ProcessType::Normalize.bits() as usize,
        ]);
        let process_type_tree = build_process_type_tree(&process_type_set);
        println!("{:?}", process_type_tree);
    }

    #[test]
    fn test_reduce_text_process_with_tree() {
        let process_type_set = IdSet::from_iter([
            ProcessType::Fanjian.bits() as usize,
            ProcessType::DeleteNormalize.bits() as usize,
            ProcessType::FanjianDeleteNormalize.bits() as usize,
            ProcessType::Delete.bits() as usize,
            ProcessType::Normalize.bits() as usize,
        ]);
        let process_type_tree = build_process_type_tree(&process_type_set);
        let text = "test爽-︻";

        let processed_text_process_type_set =
            reduce_text_process_with_tree(&process_type_tree, text);
        println!("{processed_text_process_type_set:?}");
    }

    #[test]
    fn test_reduce_text_process_with_set() {
        let process_type_set = IdSet::from_iter([
            ProcessType::Fanjian.bits() as usize,
            ProcessType::DeleteNormalize.bits() as usize,
            ProcessType::FanjianDeleteNormalize.bits() as usize,
            ProcessType::Delete.bits() as usize,
            ProcessType::Normalize.bits() as usize,
        ]);
        let text = "test爽-︻";

        let processed_text_process_type_set = reduce_text_process_with_set(&process_type_set, text);
        println!("{processed_text_process_type_set:?}");
    }
}
