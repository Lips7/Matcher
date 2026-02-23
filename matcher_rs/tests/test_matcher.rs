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

#[test]
fn matcher_process_iter_matches_process() {
    let matcher = Matcher::new(&HashMap::from([(
        1u32,
        vec![MatchTable {
            table_id: 1,
            match_table_type: MatchTableType::Simple {
                process_type: ProcessType::None,
            },
            word_list: vec!["hello", "world"],
            exemption_process_type: ProcessType::None,
            exemption_word_list: vec![],
        }],
    )]));

    let text = "hello world";

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
        "Matcher process_iter must yield same results as process"
    );
}

#[test]
fn matcher_process_iter_empty() {
    let matcher = Matcher::new(&HashMap::from([(
        1u32,
        vec![MatchTable {
            table_id: 1,
            match_table_type: MatchTableType::Simple {
                process_type: ProcessType::None,
            },
            word_list: vec!["hello"],
            exemption_process_type: ProcessType::None,
            exemption_word_list: vec![],
        }],
    )]));

    assert_eq!(matcher.process_iter("").count(), 0);
}

#[test]
fn matcher_process_iter_exemption_respected() {
    // Verify that exemption logic still works correctly through process_iter.
    let matcher = Matcher::new(&HashMap::from([(
        1u32,
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

    // "hello" alone — should match
    assert!(matcher.process_iter("hello").count() > 0);
    // "hello world" — exemption fires, no results
    assert_eq!(matcher.process_iter("hello world").count(), 0);
}

#[test]
fn process_type_tree_behavior() {
    let matcher = Matcher::new(&HashMap::from([(
        1u32,
        vec![
            MatchTable {
                table_id: 1,
                match_table_type: MatchTableType::Simple {
                    process_type: ProcessType::Fanjian | ProcessType::Delete,
                },
                word_list: vec!["hello"],
                exemption_process_type: ProcessType::None,
                exemption_word_list: vec![],
            },
            MatchTable {
                table_id: 2,
                match_table_type: MatchTableType::Simple {
                    process_type: ProcessType::None,
                },
                word_list: vec!["world"],
                exemption_process_type: ProcessType::None,
                exemption_word_list: vec![],
            },
        ],
    )]));

    assert_eq!(matcher.process("hello").len(), 1);
    assert_eq!(matcher.process("world").len(), 1);
}

#[test]
fn matcher_complex_combinations() {
    use matcher_rs::{RegexMatchType, SimMatchType};
    let matcher = Matcher::new(&HashMap::from([(
        1u32, // MatchID 1
        vec![
            MatchTable {
                table_id: 1,
                match_table_type: MatchTableType::Simple {
                    process_type: ProcessType::None,
                },
                word_list: vec!["simpleword"],
                exemption_process_type: ProcessType::None,
                exemption_word_list: vec![],
            },
            MatchTable {
                table_id: 2,
                match_table_type: MatchTableType::Regex {
                    process_type: ProcessType::None,
                    regex_match_type: RegexMatchType::Regex,
                },
                word_list: vec!["^regex.*"],
                exemption_process_type: ProcessType::None,
                exemption_word_list: vec![],
            },
            MatchTable {
                table_id: 3,
                match_table_type: MatchTableType::Similar {
                    process_type: ProcessType::None,
                    sim_match_type: SimMatchType::Levenshtein,
                    threshold: 0.8,
                },
                word_list: vec!["similarity"],
                exemption_process_type: ProcessType::None,
                exemption_word_list: vec![],
            },
        ],
    )]));

    assert!(matcher.is_match("simpleword is here"));
    assert!(matcher.is_match("regex_is_here"));
    assert!(matcher.is_match("similariti")); // matches sim node
    assert!(!matcher.is_match("completely different string"));
}

#[test]
fn matcher_cross_table_exemptions() {
    use matcher_rs::RegexMatchType;
    let matcher = Matcher::new(&HashMap::from([(
        1u32,
        vec![MatchTable {
            table_id: 1,
            match_table_type: MatchTableType::Regex {
                process_type: ProcessType::None,
                regex_match_type: RegexMatchType::Regex,
            },
            word_list: vec!["badword"], // Will flag this
            exemption_process_type: ProcessType::None,
            exemption_word_list: vec!["good badword"], // Exemption for this table
        }],
    )]));

    assert!(matcher.is_match("This is a badword"));
    assert!(!matcher.is_match("This is a good badword")); // Exception fires
}
