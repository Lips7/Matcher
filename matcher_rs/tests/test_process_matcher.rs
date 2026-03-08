use std::collections::HashSet;

use matcher_rs::{
    ProcessType, build_process_type_tree, reduce_text_process, reduce_text_process_emit,
    reduce_text_process_with_set, reduce_text_process_with_tree, text_process,
};

#[test]
fn test_text_process() {
    let text = text_process(ProcessType::Fanjian, "~ᗩ~躶~𝚩~軆~Ⲉ~");
    // "躶" (U+8EB6) -> "裸" (U+88F8)
    assert_eq!(text, "~ᗩ~裸~𝚩~軆~Ⲉ~");
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
        ProcessType::Fanjian.bits(),
        ProcessType::DeleteNormalize.bits(),
        ProcessType::FanjianDeleteNormalize.bits(),
        ProcessType::Delete.bits(),
        ProcessType::Normalize.bits(),
    ]);
    let process_type_tree = build_process_type_tree(&process_type_set);

    // Root node (None) + 6 nodes for the various transitions
    assert_eq!(process_type_tree.len(), 7);
}

#[test]
fn test_reduce_text_process_with_tree() {
    let process_type_set = HashSet::from_iter([
        ProcessType::Fanjian.bits(),
        ProcessType::DeleteNormalize.bits(),
        ProcessType::FanjianDeleteNormalize.bits(),
        ProcessType::Delete.bits(),
        ProcessType::Normalize.bits(),
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
        ProcessType::Fanjian.bits(),
        ProcessType::DeleteNormalize.bits(),
        ProcessType::FanjianDeleteNormalize.bits(),
        ProcessType::Delete.bits(),
        ProcessType::Normalize.bits(),
    ]);
    let text = "~ᗩ~躶~𝚩~軆~Ⲉ~";

    let results = reduce_text_process_with_set(&process_type_set, text);

    // reduce_text_process_with_set should produce same results as tree (though variant order/masking might differ slightly in implementation details, the set of variants should match)
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
        ProcessType::None.bits(),
        ProcessType::Fanjian.bits(),
        ProcessType::Delete.bits(),
        (ProcessType::Fanjian | ProcessType::Delete).bits(),
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
        ProcessType::Fanjian.bits(),
        ProcessType::Delete.bits(),
        ProcessType::Normalize.bits(),
    ]);

    let processed_text = reduce_text_process_with_set(&process_type_set, "");
    assert!(processed_text.iter().all(|(text, _)| text.is_empty()));
}
