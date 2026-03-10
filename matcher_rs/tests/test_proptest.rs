use matcher_rs::{ProcessType, SimpleMatcher};
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

            let matcher = SimpleMatcher::new(&map);
            let _ = matcher.is_match(&text);
            let results = matcher.process(&text);

            for res in results {
                let _ = res.word_id;
            }
        }
    }
}
