use std::collections::HashSet;

use matcher_rs::{
    ProcessType, build_process_type_tree, reduce_text_process, reduce_text_process_emit,
    reduce_text_process_with_tree, text_process,
};

#[test]
fn test_text_process() {
    let text = text_process(ProcessType::Fanjian, "~ᗩ~躶~𝚩~軆~Ⲉ~");
    // "躶" (U+8EB6) -> "裸" (U+88F8)
    assert_eq!(text, "~ᗩ~裸~𝚩~軆~Ⲉ~");
}

#[test]
fn test_delete_simd_skip_ascii_before_non_ascii() {
    // Regression: SIMD fast-skip in DeleteFindIter incorrectly advanced to the first
    // non-ASCII byte without checking for deletable ASCII bytes before it. Spaces
    // between non-deletable ASCII letters and Chinese characters were not deleted.
    let variants = reduce_text_process(ProcessType::FanjianDeleteNormalize, "A B 測試 Ａ １");
    assert_eq!(variants[0], "A B 測試 Ａ １");
    assert_eq!(variants[1], "A B 测试 Ａ １");
    assert_eq!(variants[2], "AB测试Ａ１");
    assert_eq!(variants[3], "ab测试a1");
}

#[test]
fn test_reduce_text_process() {
    let variants = reduce_text_process(ProcessType::FanjianDeleteNormalize, "~ᗩ~躶~𝚩~軆~Ⲉ~");

    // Step-by-step:
    // 0. Original: "~ᗩ~躶~𝚩~軆~Ⲉ~"
    // 1. Fanjian:  "~ᗩ~裸~𝚩~軆~Ⲉ~"
    // 2. Delete:   "ᗩ裸𝚩軆Ⲉ"
    // 3. Normalize:"a裸b軆c"

    assert_eq!(variants.len(), 4);
    assert_eq!(variants[0], "~ᗩ~躶~𝚩~軆~Ⲉ~");
    assert_eq!(variants[1], "~ᗩ~裸~𝚩~軆~Ⲉ~");
    assert_eq!(variants[2], "ᗩ裸𝚩軆Ⲉ");
    assert_eq!(variants[3], "a裸b軆c");
}

#[test]
fn test_reduce_text_process_emit() {
    let variants = reduce_text_process_emit(ProcessType::FanjianDeleteNormalize, "~ᗩ~躶~𝚩~軆~Ⲉ~");

    // reduce_text_process_emit behavior:
    // - replace_all (Fanjian) overwrites the last element if changed.
    // - delete_all pushes a new element.
    // - replace_all (Normalize) overwrites the last element.

    // 1. Start with ["~ᗩ~躶~𝚩~軆~Ⲉ~"]
    // 2. Fanjian: ["~ᗩ~裸~𝚩~軆~Ⲉ~"] (overwritten)
    // 3. Delete: ["~ᗩ~裸~𝚩~軆~Ⲉ~", "ᗩ裸𝚩軆Ⲉ"] (pushed)
    // 4. Normalize: ["~ᗩ~裸~𝚩~軆~Ⲉ~", "a裸b軆c"] (overwritten last)

    assert_eq!(variants.len(), 2);
    assert_eq!(variants[0], "~ᗩ~裸~𝚩~軆~Ⲉ~");
    assert_eq!(variants[1], "a裸b軆c");
}

#[test]
fn test_build_process_type_tree() {
    let process_type_set = HashSet::from_iter([
        ProcessType::Fanjian,
        ProcessType::DeleteNormalize,
        ProcessType::FanjianDeleteNormalize,
        ProcessType::Delete,
        ProcessType::Normalize,
    ]);
    let process_type_tree = build_process_type_tree(&process_type_set);

    // Root node (None) + 6 nodes for the various transitions
    assert_eq!(process_type_tree.len(), 7);
}

#[test]
fn test_reduce_text_process_with_tree() {
    let process_type_set = HashSet::from_iter([
        ProcessType::Fanjian,
        ProcessType::DeleteNormalize,
        ProcessType::FanjianDeleteNormalize,
        ProcessType::Delete,
        ProcessType::Normalize,
    ]);
    let process_type_tree = build_process_type_tree(&process_type_set);
    let text = "~ᗩ~躶~𝚩~軆~Ⲉ~";

    let results = reduce_text_process_with_tree(&process_type_tree, text);

    // Verify specific expected variants and their masks
    let find_variant = |target: &str| results.iter().find(|(s, _)| s == target);

    // mask 2 = 1 << None.bits() (1 << 1)
    assert!(find_variant("~ᗩ~躶~𝚩~軆~Ⲉ~").is_some());
    assert_eq!(find_variant("~ᗩ~躶~𝚩~軆~Ⲉ~").unwrap().1, 2);

    // mask 16388 = (1 << (Fanjian | Delete | Normalize).bits()) | (1 << Fanjian.bits())
    // bits: (1 << 14) | (1 << 2) = 16384 | 4 = 16388
    assert!(find_variant("~ᗩ~裸~𝚩~軆~Ⲉ~").is_some());
    assert_eq!(find_variant("~ᗩ~裸~𝚩~軆~Ⲉ~").unwrap().1, 16388);
}

#[test]
fn test_reduce_text_process_with_set() {
    let process_type_set = HashSet::from_iter([
        ProcessType::Fanjian,
        ProcessType::DeleteNormalize,
        ProcessType::FanjianDeleteNormalize,
        ProcessType::Delete,
        ProcessType::Normalize,
    ]);
    let text = "~ᗩ~躶~𝚩~軆~Ⲉ~";

    let results = reduce_text_process_with_tree(&build_process_type_tree(&process_type_set), text);

    assert!(results.iter().any(|(s, _)| s == "a裸b軆c"));
    assert!(results.iter().any(|(s, _)| s == "ᗩ裸𝚩軆Ⲉ"));
}

#[test]
fn test_reduce_text_process_all_combined() {
    let text = reduce_text_process(
        ProcessType::Fanjian
            | ProcessType::Delete
            | ProcessType::Normalize
            | ProcessType::PinYin
            | ProcessType::PinYinChar,
        "~ᗩ~躶~𝚩~軆~Ⲉ~ 漢語西安",
    );

    // Final result should be fully normalized pinyin
    // "~ᗩ~躶~𝚩~軆~Ⲉ~ 漢語西安" -> ... -> "a luob tic han yu xi an"
    assert_eq!(text.last().unwrap(), "a luob tic han yu xi an");
}

#[test]
fn test_dag_specific_outputs() {
    let processed = text_process(ProcessType::Fanjian | ProcessType::Delete, "妳！好");
    assert_eq!(processed, "你好");

    let processed = text_process(ProcessType::Normalize, "ℋЀ⒈㈠Õ");
    assert_eq!(processed, "he11o");
}

#[test]
fn test_reduce_text_process_with_tree_correctness() {
    let process_type_set = HashSet::from_iter([
        ProcessType::None,
        ProcessType::Fanjian,
        ProcessType::Delete,
        ProcessType::Fanjian | ProcessType::Delete,
    ]);
    let process_type_tree = build_process_type_tree(&process_type_set);
    let text = "妳！好";

    let results = reduce_text_process_with_tree(&process_type_tree, text);

    let mut found_variants = results.iter().map(|(s, _)| s.as_ref()).collect::<Vec<_>>();
    found_variants.sort();

    assert!(found_variants.contains(&"妳！好"));
    assert!(found_variants.contains(&"你！好"));
    assert!(found_variants.contains(&"妳好"));
    assert!(found_variants.contains(&"你好"));
}

#[test]
fn test_reduce_text_process_empty_text() {
    let process_type_set = HashSet::from_iter([
        ProcessType::Fanjian,
        ProcessType::Delete,
        ProcessType::Normalize,
    ]);

    let processed_text =
        reduce_text_process_with_tree(&build_process_type_tree(&process_type_set), "");
    assert!(processed_text.iter().all(|(text, _)| text.is_empty()));
}

const FANJIAN_TEST_DATA: &str = include_str!("../process_map/FANJIAN.txt");
const DELETE_TEST_DATA: &str = include_str!("../process_map/TEXT-DELETE.txt");
const NORM_TEST_DATA: &str = include_str!("../process_map/NORM.txt");
const NUM_NORM_TEST_DATA: &str = include_str!("../process_map/NUM-NORM.txt");
const PINYIN_TEST_DATA: &str = include_str!("../process_map/PINYIN.txt");

#[test]
fn test_process_map_fanjian_exhaustive() {
    for line in FANJIAN_TEST_DATA.trim().lines() {
        let mut split = line.split('\t');
        let k = split.next().expect("Missing key in FANJIAN.txt");
        let v = split.next().expect("Missing value in FANJIAN.txt");

        // Current implementation is 1-to-1 for Fanjian, truncating v to first char
        let expected_v = v.chars().next().unwrap().to_string();
        assert_eq!(
            text_process(ProcessType::Fanjian, k),
            expected_v,
            "Fanjian failed for {}",
            k
        );
    }
}

#[test]
fn test_process_map_delete_exhaustive() {
    // Test characters from TEXT-DELETE.txt
    for line in DELETE_TEST_DATA.trim().lines() {
        for c in line.chars() {
            let s = c.to_string();
            assert_eq!(
                text_process(ProcessType::Delete, &s),
                "",
                "Delete failed for char '{}' (U+{:04X})",
                c,
                c as u32
            );
        }
    }

    // Test whitespace from WHITE_SPACE constant
    let white_spaces = [
        "\u{0009}", "\u{000A}", "\u{000B}", "\u{000C}", "\u{000D}", "\u{0020}", "\u{0085}",
        "\u{00A0}", "\u{1680}", "\u{2000}", "\u{2001}", "\u{2002}", "\u{2003}", "\u{2004}",
        "\u{2005}", "\u{2006}", "\u{2007}", "\u{2008}", "\u{2009}", "\u{200A}", "\u{200D}",
        "\u{200F}", "\u{2028}", "\u{2029}", "\u{202F}", "\u{205F}", "\u{3000}",
    ];
    for ws in white_spaces {
        assert_eq!(
            text_process(ProcessType::Delete, ws),
            "",
            "Delete failed for whitespace U+{:04X}",
            ws.chars().next().unwrap() as u32
        );
    }
}

#[test]
fn test_process_map_normalize_exhaustive() {
    use std::collections::HashMap;
    let mut merged_map = HashMap::new();

    // Merging logic matches process_matcher.rs: NORM then NUM_NORM overwrites
    for data in [NORM_TEST_DATA, NUM_NORM_TEST_DATA] {
        for line in data.trim().lines() {
            let mut split = line.split('\t');
            let k = split.next().expect("Missing key");
            let v = split.next().expect("Missing value");
            if k != v {
                merged_map.insert(k, v);
            }
        }
    }

    for (k, v) in merged_map {
        assert_eq!(
            text_process(ProcessType::Normalize, k),
            v,
            "Normalize failed for {}",
            k
        );
    }
}

#[test]
fn test_process_map_pinyin_exhaustive() {
    for line in PINYIN_TEST_DATA.trim().lines() {
        let mut split = line.split('\t');
        let k = split.next().expect("Missing key in PINYIN.txt");
        let v = split.next().expect("Missing value in PINYIN.txt");

        assert_eq!(
            text_process(ProcessType::PinYin, k),
            v,
            "PinYin failed for {}",
            k
        );
    }
}

#[test]
fn test_process_map_pinyin_char_exhaustive() {
    for line in PINYIN_TEST_DATA.trim().lines() {
        let mut split = line.split('\t');
        let k = split.next().expect("Missing key in PINYIN.txt");
        let v = split.next().expect("Missing value in PINYIN.txt");

        assert_eq!(
            text_process(ProcessType::PinYinChar, k),
            v.trim(),
            "PinYinChar failed for {}",
            k
        );
    }
}
