use ahash::AHashMap;
use zerovec::VarZeroVec;

use matcher_rs::*;

#[test]
fn simple_match() {
    let simple_wordlist_dict = AHashMap::from([
        (
            SimpleMatchType::FanjianDeleteNormalize,
            vec![
                SimpleWord {
                    word_id: 1,
                    word: "你真好,123",
                },
                SimpleWord {
                    word_id: 2,
                    word: r"It's /\/\y duty",
                },
                SimpleWord {
                    word_id: 3,
                    word: "学生",
                },
                SimpleWord {
                    word_id: 6,
                    word: "无,法,无,天",
                },
                SimpleWord {
                    word_id: 7,
                    word: "+V,退保",
                },
                SimpleWord {
                    word_id: 10,
                    word: r"NMN",
                },
            ],
        ),
        (
            SimpleMatchType::FanjianDeleteNormalize | SimpleMatchType::PinYin,
            vec![SimpleWord {
                word_id: 4,
                word: "你好",
            }],
        ),
        (
            SimpleMatchType::FanjianDeleteNormalize | SimpleMatchType::PinYinChar,
            vec![SimpleWord {
                word_id: 5,
                word: "西安",
            }],
        ),
        (
            SimpleMatchType::DeleteNormalize,
            vec![SimpleWord {
                word_id: 9,
                word: "八一",
            }],
        ),
    ]);
    let simple_matcher = SimpleMatcher::new(&simple_wordlist_dict);

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
}

#[test]
fn regex_match() {
    let similar_wordlist = VarZeroVec::from(&["你,ni,N", r"好,hao,H,Hao,号", r"吗,ma,M"]);
    let acrostic_wordlist = VarZeroVec::from(&["你,真,棒"]);
    let regex_wordlist = VarZeroVec::from(&[r"(?<!\d)1[3-9]\d{9}(?!\d)"]);

    let regex_table_list = vec![
        RegexTable {
            table_id: 1,
            match_id: "1",
            match_table_type: &MatchTableType::SimilarChar,
            wordlist: &similar_wordlist,
        },
        RegexTable {
            table_id: 2,
            match_id: "2",
            match_table_type: &MatchTableType::Acrostic,
            wordlist: &acrostic_wordlist,
        },
        RegexTable {
            table_id: 3,
            match_id: "3",
            match_table_type: &MatchTableType::Regex,
            wordlist: &regex_wordlist,
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
    let wordlist = VarZeroVec::from(&["你真是太棒了真的太棒了", "你真棒"]);

    let sim_table_list = vec![SimTable {
        table_id: 1,
        match_id: "1",
        wordlist: &wordlist,
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
    let match_table_dict = AHashMap::from([(
        "test",
        vec![
            MatchTable {
                table_id: 1,
                match_table_type: MatchTableType::Simple,
                wordlist: VarZeroVec::from(&["无,法,无,天"]),
                exemption_wordlist: VarZeroVec::new(),
                simple_match_type: SimpleMatchType::FanjianDeleteNormalize,
            },
            MatchTable {
                table_id: 2,
                match_table_type: MatchTableType::Simple,
                wordlist: VarZeroVec::from(&["你好"]),
                exemption_wordlist: VarZeroVec::new(),
                simple_match_type: SimpleMatchType::FanjianDeleteNormalize
                    | SimpleMatchType::PinYin,
            },
        ],
    )]);

    let matcher = Matcher::new(&match_table_dict);

    assert_eq!(
        r#"[{"table_id":1,"word":"无,法,无,天"}]"#,
        matcher.word_match("无法无天").get("test").unwrap()
    );
    assert!(matcher.word_match("无法天").is_empty());
    assert!(!matcher.word_match("你豪").is_empty());
}
