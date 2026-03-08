use std::collections::HashSet;

use matcher_rs::{
    ProcessType, build_process_type_tree, reduce_text_process, reduce_text_process_emit,
    reduce_text_process_with_set, reduce_text_process_with_tree, text_process,
};

#[test]
fn test_text_process() {
    let text = text_process(ProcessType::Fanjian, "~ᗩ~躶~𝚩~軆~Ⲉ~");
    println!("{:?}", text);
}

#[test]
fn test_reduce_text_process() {
    let text = reduce_text_process(ProcessType::FanjianDeleteNormalize, "~ᗩ~躶~𝚩~軆~Ⲉ~");
    println!("{:?}", text);
}

#[test]
fn test_reduce_text_process_emit() {
    let text = reduce_text_process_emit(ProcessType::FanjianDeleteNormalize, "~ᗩ~躶~𝚩~軆~Ⲉ~");
    println!("{:?}", text);
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
    println!("{:?}", process_type_tree);
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

    let processed_text_process_type_masks = reduce_text_process_with_tree(&process_type_tree, text);
    println!("{processed_text_process_type_masks:?}");
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

    let processed_text_process_type_masks = reduce_text_process_with_set(&process_type_set, text);
    println!("{processed_text_process_type_masks:?}");
}

#[test]
fn test_reduce_text_process_all_combined() {
    // ProcessType operations applied progressively
    let text = reduce_text_process(
        ProcessType::Fanjian
            | ProcessType::Delete
            | ProcessType::Normalize
            | ProcessType::PinYin
            | ProcessType::PinYinChar,
        "~ᗩ~躶~𝚩~軆~Ⲉ~ 漢語西安",
    );
    println!("{:?}", text);
    assert!(!text.is_empty());
}

#[test]
fn test_reduce_text_process_empty_text() {
    let process_type_set = HashSet::from_iter([
        ProcessType::Fanjian.bits(),
        ProcessType::Delete.bits(),
        ProcessType::Normalize.bits(),
    ]);

    let processed_text = reduce_text_process_with_set(&process_type_set, "");
    // Should be basically a single entry of `("", ProcessTypes...)` or purely empty.
    assert!(processed_text.iter().all(|(text, _)| text.is_empty()));
}
