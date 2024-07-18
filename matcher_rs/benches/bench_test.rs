#![feature(test)]

extern crate test;
use test::Bencher;

use matcher_rs::{build_process_type_tree, reduce_text_process_with_tree, ProcessType};

#[bench]
fn bench_test(b: &mut Bencher) {
    let process_type_list = [
        ProcessType::Fanjian,
        ProcessType::DeleteNormalize,
        ProcessType::FanjianDeleteNormalize,
        ProcessType::Delete,
        ProcessType::Normalize,
    ];
    let process_typetree = build_process_type_tree(&process_type_list);

    reduce_text_process_with_tree(&process_typetree, "hello world!");

    b.iter(|| {
        for _ in 0..1000 {
            reduce_text_process_with_tree(&process_typetree, "hello world!");
        }
    });
}

fn main() {}
