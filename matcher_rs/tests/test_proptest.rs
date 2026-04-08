use std::{borrow::Cow, collections::HashMap};

use matcher_rs::{MatcherError, ProcessType, SimpleMatcher, SimpleMatcherBuilder, SimpleResult};
use proptest::prelude::*;

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
            ProcessType::VariantNorm,
            ProcessType::Delete,
            ProcessType::Normalize,
            ProcessType::Romanize,
            ProcessType::RomanizeChar,
            ProcessType::DeleteNormalize,
            ProcessType::VariantNormDeleteNormalize,
        ] {
            let mut map = HashMap::new();
            map.insert(ptype, inner_map.clone());

            let matcher = match SimpleMatcher::new(&map) {
                Ok(m) => m,
                Err(MatcherError::EmptyPatterns) => continue,
                Err(e) => panic!("unexpected error: {e}"),
            };
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
        let matcher = match builder.build() {
            Ok(m) => m,
            Err(MatcherError::EmptyPatterns) => return Ok(()),
            Err(e) => panic!("unexpected error: {e}"),
        };

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
        let matcher = match SimpleMatcherBuilder::new()
            .add_word(ProcessType::None, 1, &word)
            .build()
        {
            Ok(m) => m,
            Err(MatcherError::EmptyPatterns) => return Ok(()),
            Err(e) => panic!("unexpected error: {e}"),
        };

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
        let matcher = match SimpleMatcherBuilder::new()
            .add_word(ProcessType::None, 1, &word)
            .build()
        {
            Ok(m) => m,
            Err(MatcherError::EmptyPatterns) => return Ok(()),
            Err(e) => panic!("unexpected error: {e}"),
        };

        let _ = matcher.is_match(&text);
        let _ = matcher.process(&text);
    }

    #[test]
    fn prop_is_match_process_consistent_all_ptypes(
        word in "[a-z]{1,30}",
        text in "[a-z ]{0,100}"
    ) {
        for ptype in [
            ProcessType::None,
            ProcessType::VariantNorm,
            ProcessType::Delete,
            ProcessType::Normalize,
            ProcessType::Romanize,
            ProcessType::RomanizeChar,
            ProcessType::DeleteNormalize,
            ProcessType::VariantNormDeleteNormalize,
        ] {
            let matcher = SimpleMatcherBuilder::new()
                .add_word(ptype, 1, &word)
                .build()
                .unwrap();

            let is_match = matcher.is_match(&text);
            let results = matcher.process(&text);
            prop_assert_eq!(
                is_match,
                !results.is_empty(),
                "is_match/process mismatch for {:?}", ptype
            );
        }
    }

    #[test]
    fn prop_search_mode_equivalence(
        word in "[a-z]{1,30}",
        text in "[a-z ]{0,100}"
    ) {
        // AllSimple path: single ProcessType::None, pure literal
        let simple = SimpleMatcherBuilder::new()
            .add_word(ProcessType::None, 1, &word)
            .build()
            .unwrap();

        // General path: add a second ProcessType to force General mode
        let general = SimpleMatcherBuilder::new()
            .add_word(ProcessType::None, 1, &word)
            .add_word(ProcessType::VariantNorm, 2, &word)
            .build()
            .unwrap();

        prop_assert_eq!(
            simple.is_match(&text),
            general.is_match(&text),
            "AllSimple vs General is_match disagree"
        );

        // Both should find word_id=1 if the word appears
        let simple_ids: Vec<u32> = simple.process(&text).iter().map(|r| r.word_id).collect();
        let general_ids: Vec<u32> = general.process(&text).iter().map(|r| r.word_id).collect();
        if simple_ids.contains(&1) {
            prop_assert!(
                general_ids.contains(&1),
                "General path missed word_id=1 that AllSimple found"
            );
        }
    }

    #[test]
    fn prop_deterministic(
        word in "[a-z]{1,30}",
        text in "[a-z ]{0,100}"
    ) {
        let matcher = SimpleMatcherBuilder::new()
            .add_word(ProcessType::None, 1, &word)
            .build()
            .unwrap();

        let r1 = matcher.process(&text);
        let r2 = matcher.process(&text);
        prop_assert_eq!(r1, r2, "process() must be deterministic");
    }

    #[test]
    fn prop_not_veto_consistent(
        positive in "[a-z]{1,20}",
        negative in "[a-z]{1,20}",
        prefix in "[a-z ]{0,30}",
        suffix in "[a-z ]{0,30}"
    ) {
        let pattern = format!("{}~{}", positive, negative);
        let matcher = SimpleMatcherBuilder::new()
            .add_word(ProcessType::None, 1, &pattern)
            .build()
            .unwrap();

        // Text that contains the negative substring must not fire
        let text_with_neg = format!("{}{}{}{}", prefix, positive, negative, suffix);
        if text_with_neg.contains(&negative) && text_with_neg.contains(&positive) {
            prop_assert!(
                !matcher.is_match(&text_with_neg),
                "NOT veto should prevent match when negative present"
            );
        }
    }

    #[test]
    fn prop_builder_vs_hashmap_equivalent(
        word in "[a-z]{1,30}",
        text in "[a-z ]{0,100}"
    ) {
        let from_builder = SimpleMatcherBuilder::new()
            .add_word(ProcessType::None, 1, &word)
            .build()
            .unwrap();

        let from_map = SimpleMatcher::new(&HashMap::from([(
            ProcessType::None,
            HashMap::from([(1u32, word.as_str())]),
        )]))
        .unwrap();

        prop_assert_eq!(
            from_builder.is_match(&text),
            from_map.is_match(&text),
            "builder vs hashmap is_match disagree"
        );
        prop_assert_eq!(
            from_builder.process(&text),
            from_map.process(&text),
            "builder vs hashmap process disagree"
        );
    }

    #[test]
    fn prop_process_into_appends(
        word in "[a-z]{1,30}",
        text in "[a-z ]{0,100}"
    ) {
        let matcher = SimpleMatcherBuilder::new()
            .add_word(ProcessType::None, 1, &word)
            .build()
            .unwrap();

        // Pre-seed with a sentinel result
        let sentinel = SimpleResult {
            word_id: 9999,
            word: Cow::Borrowed("sentinel"),
        };
        let mut results: Vec<SimpleResult<'_>> = vec![sentinel];
        matcher.process_into(&text, &mut results);

        let expected = matcher.process(&text);
        // First element should be the sentinel
        prop_assert_eq!(results[0].word_id, 9999);
        prop_assert_eq!(&*results[0].word, "sentinel");
        // Remaining elements should match process() output
        prop_assert_eq!(&results[1..], expected.as_slice());
    }

    #[test]
    fn prop_all_process_types_no_panic(
        bits in 0u8..64,
        text in "\\PC{0,200}"
    ) {
        let pt = ProcessType::from_bits_retain(bits);
        let _ = matcher_rs::text_process(pt, &text);
    }

    #[test]
    fn prop_ascii_parent_child_transforms_match_materialized_output(
        input in prop::collection::vec(
            (32u8..=125u8).prop_filter("exclude matcher operators and escape prefix", |b| *b != b'&' && *b != b'~' && *b != b'|' && *b != b'\\'),
            1..80
        ).prop_map(|bytes| String::from_utf8(bytes).expect("ASCII strategy should stay valid UTF-8"))
    ) {
        for bits in 1u8..64 {
            let pt = ProcessType::from_bits_retain(bits);
            let expected = matcher_rs::text_process(pt, &input);
            if expected.is_empty() {
                continue;
            }

            let matcher = SimpleMatcherBuilder::new()
                .add_word(pt, 1, expected.as_ref())
                .build()
                .unwrap();

            prop_assert!(
                matcher.is_match(&input),
                "ASCII parent transform path missed match for {:?} on input {:?} -> {:?}",
                pt,
                input,
                expected
            );

            let results = matcher.process(&input);
            prop_assert!(
                results.iter().any(|result| result.word_id == 1),
                "process() missed word_id=1 for {:?} on input {:?} -> {:?}",
                pt,
                input,
                expected
            );
        }
    }
}
