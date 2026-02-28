use matcher_rs::{ProcessType, SimMatchType, SimMatcher, SimTable, TextMatcherTrait};

#[test]
fn sim_match() {
    let sim_matcher = SimMatcher::new(
        &[SimTable {
            table_id: 1,
            match_id: 1,
            process_type: ProcessType::None,
            sim_match_type: SimMatchType::Levenshtein,
            word_list: vec!["helloworld"],
            threshold: 0.8,
        }]
        .as_slice(),
    );

    assert!(sim_matcher.is_match("helloworl"));
    assert!(sim_matcher.is_match("halloworld"));
    assert!(sim_matcher.is_match("ha1loworld"));
    assert!(!sim_matcher.is_match("ha1loworld1"));
}

#[test]
fn sim_process_iter_matches_process() {
    let matcher = SimMatcher::new(
        &[SimTable {
            table_id: 1,
            match_id: 1,
            process_type: ProcessType::None,
            sim_match_type: SimMatchType::Levenshtein,
            word_list: vec!["helloworld", "rustlang"],
            threshold: 0.8,
        }]
        .as_slice(),
    );

    let text = "helloworl"; // close to "helloworld"

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
fn sim_process_iter_similarity_values_match() {
    let matcher = SimMatcher::new(
        &[SimTable {
            table_id: 1,
            match_id: 1,
            process_type: ProcessType::None,
            sim_match_type: SimMatchType::Levenshtein,
            word_list: vec!["helloworld"],
            threshold: 0.8,
        }]
        .as_slice(),
    );

    let text = "halloworld";
    let via_process: Vec<f64> = matcher
        .process(text)
        .into_iter()
        .map(|r| r.similarity)
        .collect();
    let via_iter: Vec<f64> = matcher.process_iter(text).map(|r| r.similarity).collect();
    assert_eq!(via_process, via_iter);
}

#[test]
fn sim_process_iter_empty() {
    let matcher = SimMatcher::new(
        &[SimTable {
            table_id: 1,
            match_id: 1,
            process_type: ProcessType::None,
            sim_match_type: SimMatchType::Levenshtein,
            word_list: vec!["hello"],
            threshold: 0.8,
        }]
        .as_slice(),
    );

    assert_eq!(matcher.process_iter("").count(), 0);
}

#[test]
fn sim_matcher_threshold_edge_cases() {
    let sim_table_list = vec![
        SimTable {
            table_id: 1,
            match_id: 1,
            process_type: ProcessType::None,
            sim_match_type: SimMatchType::Levenshtein,
            word_list: vec!["hello"],
            threshold: 0.9,
        },
        SimTable {
            table_id: 2,
            match_id: 2,
            process_type: ProcessType::None,
            sim_match_type: SimMatchType::Levenshtein,
            word_list: vec!["world"],
            threshold: 0.1,
        },
    ];
    let matcher = SimMatcher::new(&sim_table_list);

    // "hello" matches "hello" @ 1.0 AND "world" @ 0.2
    assert_eq!(matcher.process("hello").len(), 2);
    // "hellp" matches "world" @ ~0.2 (if len remains 5) and potentially nothing else
    let results = matcher.process("hellp");
    assert!(results.iter().any(|r| r.match_id == 2));
}

#[test]
fn sim_match_multibyte_and_unicode() {
    let sim_matcher = SimMatcher::new(
        &[SimTable {
            table_id: 1,
            match_id: 1,
            process_type: ProcessType::None,
            sim_match_type: SimMatchType::Levenshtein,
            word_list: vec!["你好世界", "👋😀🌍"],
            threshold: 0.75, // Allow 1 character off for 4-character string
        }]
        .as_slice(),
    );

    assert!(sim_matcher.is_match("你好世果")); // one char off, 3/4 = 0.75 >= 0.75 -> matches
    assert!(!sim_matcher.is_match("你")); // too far

    // For length 3, a missing char is 2/3 = 0.66 which is < 0.75. So we need exact.
    assert!(!sim_matcher.is_match("👋😀"));
    assert!(sim_matcher.is_match("👋😀🌍")); // exact
}

#[test]
fn sim_match_exact_threshold() {
    let sim_matcher = SimMatcher::new(
        &[SimTable {
            table_id: 1,
            match_id: 1,
            process_type: ProcessType::None,
            sim_match_type: SimMatchType::Levenshtein,
            word_list: vec!["perfection"],
            threshold: 1.0,
        }]
        .as_slice(),
    );

    assert!(sim_matcher.is_match("perfection"));
    assert!(!sim_matcher.is_match("perfectio")); // 1 char off must fail
}

#[test]
fn sim_match_zero_threshold() {
    let sim_matcher = SimMatcher::new(
        &[SimTable {
            table_id: 1,
            match_id: 1,
            process_type: ProcessType::None,
            sim_match_type: SimMatchType::Levenshtein,
            word_list: vec!["apple"],
            threshold: 0.0,
        }]
        .as_slice(),
    );

    // With threshold 0.0, anything should match unless similarity logic drops completely disparate things.
    // Given the formula 1.0 - (distance / max_len), it might only be exact 0.0 if there are no common chars.
    assert!(sim_matcher.is_match("banana"));
}
