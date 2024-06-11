use gxhash::HashMap as GxHashMap;
use nohash_hasher::IntMap;

use matcher_rs::*;

#[test]
fn simple_match() {
    let simple_word_list_dict = GxHashMap::from_iter([
        (
            SimpleMatchType::FanjianDeleteNormalize,
            IntMap::from_iter([
                (1, "你真好,123"),
                (2, r"It's /\/\y duty"),
                (3, "学生"),
                (6, "无,法,无,天"),
                (7, "+V,退保"),
                (10, r"NMN"),
            ]),
        ),
        (
            SimpleMatchType::Fanjian,
            IntMap::from_iter([(11, r"xxx,yyy")]),
        ),
        (
            SimpleMatchType::FanjianDeleteNormalize | SimpleMatchType::PinYin,
            IntMap::from_iter([(4, r"你好")]),
        ),
        (
            SimpleMatchType::FanjianDeleteNormalize | SimpleMatchType::PinYinChar,
            IntMap::from_iter([(5, r"西安")]),
        ),
        (
            SimpleMatchType::DeleteNormalize,
            IntMap::from_iter([(9, r"八一")]),
        ),
    ]);
    let simple_matcher = SimpleMatcher::new(&simple_word_list_dict);

    assert_eq!(
        "你真好,123".to_owned(),
        simple_matcher.process("你真好,123")[0].word
    );
    assert_eq!(
        "你真好,123".to_owned(),
        simple_matcher.process(
            "
                    你
                    真
                    好
                    1
                    2
                    3
                "
        )[0]
        .word
    );
    assert!(!simple_matcher.process(r"It's /\/\y duty").is_empty());
    assert!(!simple_matcher.process("零基础不会给孩子扎头发的，感觉看过来，这里有最详细的教程。手把手教学1分钟学会一款发型。#零基础教学 #简单易学 #生女儿就是用来打扮的").is_empty());

    assert!(simple_matcher.is_match("你好,123"));
    assert!(simple_matcher.is_match("你号"));
    assert!(simple_matcher.is_match("xian"));
    assert!(simple_matcher.is_match("Mac+vlan 退，保"));
    assert!(simple_matcher.is_match("八○一社区"));
    assert!(simple_matcher.is_match("ЛmЛmXoXo"));

    assert!(simple_matcher.is_match("无无法天"));
    assert_eq!(simple_matcher.is_match("无法天"), false);
    assert_eq!(simple_matcher.is_match("xꓫ,yyy"), false);
}

#[test]
fn regex_match() {
    let similar_word_list = vec!["你,ni,N", r"好,hao,H,Hao,号", r"吗,ma,M"];
    let acrostic_word_list = vec!["你,真,棒"];
    let regex_word_list = vec![r"(?<!\d)1[3-9]\d{9}(?!\d)"];

    let regex_table_list = vec![
        RegexTable {
            table_id: 1,
            match_id: "1",
            match_table_type: &MatchTableType::SimilarChar,
            word_list: &similar_word_list,
        },
        RegexTable {
            table_id: 2,
            match_id: "2",
            match_table_type: &MatchTableType::Acrostic,
            word_list: &acrostic_word_list,
        },
        RegexTable {
            table_id: 3,
            match_id: "3",
            match_table_type: &MatchTableType::Regex,
            word_list: &regex_word_list,
        },
    ];
    let regex_matcher = RegexMatcher::new(&regex_table_list);

    assert_eq!("你号吗", regex_matcher.process("你，号？吗")[0].word);
    assert_eq!(
        "你,真,棒",
        regex_matcher.process("你先休息，真的很棒，棒到家了")[0].word
    );
    assert!(regex_matcher.is_match("15651781111"));
}

#[test]
fn sim_match() {
    let word_list = vec!["你真是太棒了真的太棒了", "你真棒"];

    let sim_table_list = vec![SimTable {
        table_id: 1,
        match_id: "1",
        word_list: &word_list,
    }];
    let sim_matcher = SimMatcher::new(&sim_table_list);

    assert_eq!(
        "你真是太棒了真的太棒了",
        sim_matcher.process("你真是太棒了真的太")[0].word
    );

    assert!(sim_matcher.is_match("你真棒"));
}

#[test]
fn word_match() {
    let match_table_map = GxHashMap::from_iter([(
        "test",
        vec![
            MatchTable {
                table_id: 1,
                match_table_type: MatchTableType::Simple,
                simple_match_type: SimpleMatchType::FanjianDeleteNormalize,
                word_list: vec!["无,法,无,天"],
                exemption_simple_match_type: SimpleMatchType::FanjianDeleteNormalize,
                exemption_word_list: vec![],
            },
            MatchTable {
                table_id: 2,
                match_table_type: MatchTableType::Simple,
                simple_match_type: SimpleMatchType::FanjianDeleteNormalize
                    | SimpleMatchType::PinYin,
                word_list: vec!["你好"],
                exemption_simple_match_type: SimpleMatchType::FanjianDeleteNormalize,
                exemption_word_list: vec![],
            },
        ],
    )]);

    let matcher = Matcher::new(&match_table_map);

    assert_eq!(
        r#"[{"table_id":1,"word":"无,法,无,天"}]"#,
        matcher.word_match("无法无天").get("test").unwrap()
    );
    assert!(matcher.word_match("无法天").is_empty());
    assert!(!matcher.word_match("你豪").is_empty());
}
