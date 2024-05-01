use ahash::AHashMap;
use matcher_rs::*;

// use vectorscan_rs::{Database, Flag, Pattern, Scan, ScanMode, Scanner};

fn main() {
    // let database = Database::new(
    //     vec![Pattern::new(
    //         "123".as_bytes(),
    //         Flag::CASELESS | Flag::SOM_LEFTMOST,
    //         1,
    //     )],
    //     ScanMode::BLOCK,
    //     true,
    // ).unwrap();
    // let scanner = Scanner::new(&database).unwrap();
    // let _ = scanner.scan("123".as_bytes(), |rule_id, from, to, _| {
    //     println!("{} {} {}", rule_id, from, to);
    //     Scan::Continue
    // });
    let vector_wordlist_dict = AHashMap::from([
        (
            VectorMatchType::FanjianDeleteNormalize,
            vec![
                VectorWord {
                    word_id: 1,
                    word: "你真好,123",
                },
                VectorWord {
                    word_id: 2,
                    word: r"It's /\/\y duty",
                },
                VectorWord {
                    word_id: 3,
                    word: "学生",
                },
                VectorWord {
                    word_id: 6,
                    word: "无,法,无,天",
                },
                VectorWord {
                    word_id: 7,
                    word: "+V,退保",
                },
                VectorWord {
                    word_id: 10,
                    word: r"NMN",
                },
            ],
        ),
        (
            VectorMatchType::FanjianDeleteNormalize | VectorMatchType::PinYin,
            vec![VectorWord {
                word_id: 4,
                word: "你好",
            }],
        ),
        (
            VectorMatchType::FanjianDeleteNormalize | VectorMatchType::PinYinChar,
            vec![VectorWord {
                word_id: 5,
                word: "西安",
            }],
        ),
        (
            VectorMatchType::DeleteNormalize,
            vec![VectorWord {
                word_id: 9,
                word: "八一",
            }],
        ),
    ]);
    let vector_matcher = VectorMatcher::new(&vector_wordlist_dict);

    assert_eq!(
        "你真好,123".to_owned(),
        vector_matcher.process("你真好,123")[0].word
    );
    assert_eq!(
        "你真好,123".to_owned(),
        vector_matcher.process(
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
    assert!(!vector_matcher.process(r"It's /\/\y duty").is_empty());
    assert!(!vector_matcher.process("零基础不会给孩子扎头发的，感觉看过来，这里有最详细的教程。手把手教学1分钟学会一款发型。#零基础教学 #简单易学 #生女儿就是用来打扮的").is_empty());

    assert!(vector_matcher.is_match("你好,123"));
    assert!(vector_matcher.is_match("你号"));
    assert!(vector_matcher.is_match("xian"));
    assert!(vector_matcher.is_match("Mac+vlan 退，保"));
    assert!(vector_matcher.is_match("八○一社区"));
    assert!(vector_matcher.is_match("ЛmЛmXoXo"));

    assert!(vector_matcher.is_match("无无法天"));
    assert_eq!(vector_matcher.is_match("无法天"), false);
}
