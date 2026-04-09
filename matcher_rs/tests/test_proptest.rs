use std::{borrow::Cow, collections::HashMap};

use matcher_rs::{MatcherError, ProcessType, SimpleMatcher, SimpleMatcherBuilder, SimpleResult};
use proptest::prelude::*;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    #[test]
    fn prop_no_panic_random_input(
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

        // Also exercise all ProcessType bits through text_process
        for bits in 0u8..64 {
            let pt = ProcessType::from_bits_retain(bits);
            let _ = matcher_rs::text_process(pt, &text);
        }
    }

    #[test]
    fn prop_api_consistency(
        words in prop::collection::vec("\\PC{1,30}", 5..20),
        text in "\\PC{0,100}"
    ) {
        // Test across all ProcessTypes: is_match, process, process_into,
        // for_each_match must all agree.
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
            let mut builder = SimpleMatcherBuilder::new();
            for (i, word) in words.iter().enumerate() {
                builder = builder.add_word(ptype, i as u32, word);
            }
            let matcher = match builder.build() {
                Ok(m) => m,
                Err(MatcherError::EmptyPatterns) => continue,
                Err(e) => panic!("unexpected error: {e}"),
            };

            let is_match = matcher.is_match(&text);
            let results = matcher.process(&text);

            // is_match ↔ process consistency
            prop_assert_eq!(
                is_match,
                !results.is_empty(),
                "is_match/process mismatch for {:?}", ptype
            );

            // process_into matches process
            let mut into_results = Vec::new();
            matcher.process_into(&text, &mut into_results);
            prop_assert_eq!(results.len(), into_results.len());
            for (a, b) in results.iter().zip(into_results.iter()) {
                prop_assert_eq!(a.word_id, b.word_id);
            }

            // process_into append semantics
            let sentinel = SimpleResult {
                word_id: 9999,
                word: Cow::Borrowed("sentinel"),
            };
            let mut append_buf: Vec<SimpleResult<'_>> = vec![sentinel];
            matcher.process_into(&text, &mut append_buf);
            prop_assert_eq!(append_buf[0].word_id, 9999);
            prop_assert_eq!(&append_buf[1..], results.as_slice());

            // for_each_match consistency
            let mut fem_ids = Vec::new();
            matcher.for_each_match(&text, |r| {
                fem_ids.push(r.word_id);
                false
            });
            let mut proc_ids: Vec<u32> = results.iter().map(|r| r.word_id).collect();
            fem_ids.sort();
            proc_ids.sort();
            prop_assert_eq!(fem_ids, proc_ids);
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

        let text_with_neg = format!("{}{}{}{}", prefix, positive, negative, suffix);
        if text_with_neg.contains(&negative) && text_with_neg.contains(&positive) {
            prop_assert!(
                !matcher.is_match(&text_with_neg),
                "NOT veto should prevent match when negative present"
            );
        }
    }

    #[test]
    fn prop_builder_vs_hashmap(
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
    fn prop_transform_round_trip(
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
                "transform round-trip missed match for {:?} on input {:?} -> {:?}",
                pt, input, expected
            );

            let results = matcher.process(&input);
            prop_assert!(
                results.iter().any(|result| result.word_id == 1),
                "process() missed word_id=1 for {:?} on input {:?} -> {:?}",
                pt, input, expected
            );
        }
    }
}
