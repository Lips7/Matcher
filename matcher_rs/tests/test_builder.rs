use matcher_rs::{
    MatchTable, MatchTableBuilder, MatchTableType, MatcherBuilder, ProcessType, TextMatcherTrait,
};

#[test]
fn matcher_builder() {
    let matcher = MatcherBuilder::new()
        .add_table(
            1,
            MatchTable {
                table_id: 1,
                match_table_type: MatchTableType::Simple {
                    process_type: ProcessType::None,
                },
                word_list: vec!["hello"],
                exemption_process_type: ProcessType::None,
                exemption_word_list: vec![],
            },
        )
        .build();

    assert!(matcher.is_match("hello world"));
    assert!(!matcher.is_match("goodbye"));
}

#[test]
fn match_table_builder_simple() {
    let table = MatchTableBuilder::new(
        1,
        MatchTableType::Simple {
            process_type: ProcessType::None,
        },
    )
    .add_word("hello")
    .add_word("world")
    .build();

    let matcher = MatcherBuilder::new().add_table(1, table).build();
    assert!(matcher.is_match("hello"));
    assert!(matcher.is_match("world"));
    assert!(!matcher.is_match("goodbye"));
}

#[test]
fn match_table_builder_add_words_bulk() {
    let table = MatchTableBuilder::new(
        2,
        MatchTableType::Simple {
            process_type: ProcessType::None,
        },
    )
    .add_words(["foo", "bar", "baz"])
    .build();

    let matcher = MatcherBuilder::new().add_table(1, table).build();
    assert!(matcher.is_match("foo"));
    assert!(matcher.is_match("bar"));
    assert!(matcher.is_match("baz"));
    assert!(!matcher.is_match("qux"));
}

#[test]
fn match_table_builder_exemption() {
    let table = MatchTableBuilder::new(
        3,
        MatchTableType::Simple {
            process_type: ProcessType::None,
        },
    )
    .add_word("hello")
    .add_exemption_word("world")
    .build();

    let matcher = MatcherBuilder::new().add_table(1, table).build();
    assert!(matcher.is_match("hello"));
    assert!(!matcher.is_match("hello world"));
}

#[test]
fn match_table_builder_add_exemption_words_bulk() {
    let table = MatchTableBuilder::new(
        4,
        MatchTableType::Simple {
            process_type: ProcessType::None,
        },
    )
    .add_word("hello")
    .add_exemption_words(["world", "earth"])
    .build();

    let matcher = MatcherBuilder::new().add_table(1, table).build();
    assert!(matcher.is_match("hello"));
    assert!(!matcher.is_match("hello world"));
    assert!(!matcher.is_match("hello earth"));
}

#[test]
fn match_table_builder_regex() {
    use matcher_rs::RegexMatchType;

    let table = MatchTableBuilder::new(
        5,
        MatchTableType::Regex {
            process_type: ProcessType::None,
            regex_match_type: RegexMatchType::Regex,
        },
    )
    .add_word("h[aeiou]llo")
    .add_word("w[aeiou]rld")
    .build();

    let matcher = MatcherBuilder::new().add_table(1, table).build();
    assert!(matcher.is_match("hallo"));
    assert!(matcher.is_match("world"));
    assert!(!matcher.is_match("hxllo"));
}

#[test]
fn match_table_builder_similar() {
    use matcher_rs::SimMatchType;

    let table = MatchTableBuilder::new(
        6,
        MatchTableType::Similar {
            process_type: ProcessType::None,
            sim_match_type: SimMatchType::Levenshtein,
            threshold: 0.8,
        },
    )
    .add_word("helloworld")
    .build();

    let matcher = MatcherBuilder::new().add_table(1, table).build();
    assert!(matcher.is_match("helloworl")); // one char off
    assert!(!matcher.is_match("completely different"));
}

#[test]
fn match_table_builder_empty() {
    let table = MatchTableBuilder::new(
        1,
        MatchTableType::Simple {
            process_type: ProcessType::None,
        },
    )
    .build();

    let matcher = MatcherBuilder::new().add_table(1, table).build();
    assert!(!matcher.is_match("anything"));
}
