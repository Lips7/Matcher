use matcher_rs::{ProcessType, SimpleMatcher, SimpleMatcherBuilder};
use proptest::prelude::*;
use std::collections::HashMap;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    #[test]
    fn prop_simple_matcher_does_not_panic(
        word in "\\PC{0,100}",
        text in "\\PC{0,100}"
    ) {
        let mut inner_map = HashMap::new();
        inner_map.insert(1, word.as_str());

        for ptype in [
            ProcessType::None,
            ProcessType::Fanjian,
            ProcessType::Delete,
            ProcessType::Normalize,
            ProcessType::PinYin,
            ProcessType::PinYinChar,
            ProcessType::DeleteNormalize,
            ProcessType::FanjianDeleteNormalize,
        ] {
            let mut map = HashMap::new();
            map.insert(ptype, inner_map.clone());

            let matcher = SimpleMatcher::new(&map).unwrap();
            let _ = matcher.is_match(&text);
            let results = matcher.process(&text);

            for res in results {
                let _ = res.word_id;
            }
        }
    }

    #[test]
    fn prop_multi_rule_consistent(
        words in prop::collection::vec("\\PC{1,30}", 5..20),
        text in "\\PC{0,100}"
    ) {
        let mut builder = SimpleMatcherBuilder::new();
        for (i, word) in words.iter().enumerate() {
            builder = builder.add_word(ProcessType::None, i as u32, word);
        }
        let matcher = builder.build().unwrap();

        let is_match = matcher.is_match(&text);
        let results = matcher.process(&text);

        prop_assert_eq!(
            is_match,
            !results.is_empty(),
            "is_match and process must be consistent"
        );
    }

    #[test]
    fn prop_process_into_matches_process(
        word in "\\PC{1,50}",
        text in "\\PC{0,100}"
    ) {
        let matcher = SimpleMatcherBuilder::new()
            .add_word(ProcessType::None, 1, &word)
            .build()
            .unwrap();

        let results = matcher.process(&text);
        let mut into_results = Vec::new();
        matcher.process_into(&text, &mut into_results);

        prop_assert_eq!(results.len(), into_results.len());
        for (a, b) in results.iter().zip(into_results.iter()) {
            prop_assert_eq!(a.word_id, b.word_id);
        }
    }

    #[test]
    fn prop_operator_patterns_no_panic(
        // Generate random strings that may contain & and ~ operators
        word in "[a-z&~]{1,50}",
        text in "[a-z ]{0,100}"
    ) {
        let matcher = SimpleMatcherBuilder::new()
            .add_word(ProcessType::None, 1, &word)
            .build()
            .unwrap();

        let _ = matcher.is_match(&text);
        let _ = matcher.process(&text);
    }

}
