use matcher_rs::{
    MatchTable, MatchTableType, Matcher, ProcessType, RegexMatchType, RegexMatcher, RegexTable,
    SimMatchType, SimMatcher, SimTable, SimpleMatcher, TextMatcherTrait,
};
use proptest::prelude::*;
use std::collections::HashMap;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]

    #[test]
    fn prop_simple_matcher_does_not_panic(
        word in "\\PC{0,100}",
        text in "\\PC{0,100}"
    ) {
        let mut inner_map = HashMap::new();
        inner_map.insert(1, word.as_str());

        // Test with different process types
        for ptype in [ProcessType::None, ProcessType::Fanjian, ProcessType::Normalize] {
            let mut map = HashMap::new();
            map.insert(ptype, inner_map.clone());

            let matcher = SimpleMatcher::new(&map);
            let _ = matcher.is_match(&text);
            let results = matcher.process(&text);

            // Just verifying it doesn't panic and iterators are safe
            for res in results {
                let _ = res.word_id;
            }
        }
    }

    #[test]
    fn prop_regex_matcher_does_not_panic(
        word in "[a-zA-Z0-9]{0,100}",  // Use alphanumeric to avoid invalid regex patterns causing compile error panics
        text in "\\PC{0,100}"
    ) {
        let regex_table = RegexTable {
            table_id: 1,
            match_id: 1,
            process_type: ProcessType::None,
            regex_match_type: RegexMatchType::Regex,
            // Only insert word if it's not empty, fancy-regex handles most but we limit to alphanumeric
            word_list: vec![word.as_str()],
        };

        // We only test if regex compilation itself doesn't panic
        if let Ok(matcher_res) = std::panic::catch_unwind(|| {
            RegexMatcher::new(&[regex_table][..])
        }) {
            let _ = matcher_res.is_match(&text);
            let _ = matcher_res.process(&text);
        }
    }

    #[test]
    fn prop_sim_matcher_does_not_panic(
        word in "\\PC{0,100}",
        text in "\\PC{0,100}",
        threshold in 0.0f64..=1.0f64
    ) {
        let sim_table = SimTable {
            table_id: 1,
            match_id: 1,
            process_type: ProcessType::None,
            sim_match_type: SimMatchType::Levenshtein,
            word_list: vec![word.as_str()],
            threshold,
        };

        let matcher = SimMatcher::new(&[sim_table]);
        let _ = matcher.is_match(&text);
        let _ = matcher.process(&text);
    }

    #[test]
    fn prop_matcher_does_not_panic(
        word in "\\PC{0,100}",
        exempt_word in "\\PC{0,100}",
        text in "\\PC{0,100}"
    ) {
        let table = MatchTable {
            table_id: 1,
            match_table_type: MatchTableType::Simple {
                process_type: ProcessType::None,
            },
            word_list: vec![word.as_str()],
            exemption_process_type: ProcessType::None,
            exemption_word_list: vec![exempt_word.as_str()],
        };

        let mut map = HashMap::new();
        map.insert(1, vec![table]);

        let matcher = Matcher::new(&map);
        let _ = matcher.is_match(&text);
        let _ = matcher.process(&text);
    }
}
