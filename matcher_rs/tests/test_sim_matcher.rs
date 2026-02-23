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

#[test]
fn sim_process_iter_matches_process() {
    let matcher = SimMatcher::new(&[SimTable {
        table_id: 1,
        match_id: 1,
        process_type: ProcessType::None,
        sim_match_type: SimMatchType::Levenshtein,
        word_list: vec!["helloworld", "rustlang"],
        threshold: 0.8,
    }]);

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
    let matcher = SimMatcher::new(&[SimTable {
        table_id: 1,
        match_id: 1,
        process_type: ProcessType::None,
        sim_match_type: SimMatchType::Levenshtein,
        word_list: vec!["helloworld"],
        threshold: 0.8,
    }]);

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
    let matcher = SimMatcher::new(&[SimTable {
        table_id: 1,
        match_id: 1,
        process_type: ProcessType::None,
        sim_match_type: SimMatchType::Levenshtein,
        word_list: vec!["hello"],
        threshold: 0.8,
    }]);

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
