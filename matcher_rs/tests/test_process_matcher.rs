use id_set::IdSet;
use matcher_rs::{
    build_process_type_tree, reduce_text_process, reduce_text_process_emit,
    reduce_text_process_with_set, reduce_text_process_with_tree, text_process, ProcessType,
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
    let process_type_set = IdSet::from_iter([
        ProcessType::Fanjian.bits() as usize,
        ProcessType::DeleteNormalize.bits() as usize,
        ProcessType::FanjianDeleteNormalize.bits() as usize,
        ProcessType::Delete.bits() as usize,
        ProcessType::Normalize.bits() as usize,
    ]);
    let process_type_tree = build_process_type_tree(&process_type_set);
    println!("{:?}", process_type_tree);
}

#[test]
fn test_reduce_text_process_with_tree() {
    let process_type_set = IdSet::from_iter([
        ProcessType::Fanjian.bits() as usize,
        ProcessType::DeleteNormalize.bits() as usize,
        ProcessType::FanjianDeleteNormalize.bits() as usize,
        ProcessType::Delete.bits() as usize,
        ProcessType::Normalize.bits() as usize,
    ]);
    let process_type_tree = build_process_type_tree(&process_type_set);
    let text = "test爽-︻";

    let processed_text_process_type_set = reduce_text_process_with_tree(&process_type_tree, text);
    println!("{processed_text_process_type_set:?}");
}

#[test]
fn test_reduce_text_process_with_set() {
    let process_type_set = IdSet::from_iter([
        ProcessType::Fanjian.bits() as usize,
        ProcessType::DeleteNormalize.bits() as usize,
        ProcessType::FanjianDeleteNormalize.bits() as usize,
        ProcessType::Delete.bits() as usize,
        ProcessType::Normalize.bits() as usize,
    ]);
    let text = "test爽-︻";

    let processed_text_process_type_set = reduce_text_process_with_set(&process_type_set, text);
    println!("{processed_text_process_type_set:?}");
}
