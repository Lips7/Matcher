#![feature(test)]

extern crate test;
use test::Bencher;

use matcher_rs::{build_smt_tree, reduce_text_process_with_tree, SimpleMatchType};

#[bench]
fn bench_test(b: &mut Bencher) {
    let smt_list = [
        SimpleMatchType::Fanjian,
        SimpleMatchType::DeleteNormalize - SimpleMatchType::WordDelete,
        SimpleMatchType::FanjianDeleteNormalize - SimpleMatchType::WordDelete,
        SimpleMatchType::Delete - SimpleMatchType::WordDelete,
        SimpleMatchType::Normalize,
    ];
    let smt_tree = build_smt_tree(&smt_list);

    reduce_text_process_with_tree(&smt_tree, "你好，我是中国人");

    b.iter(|| {
        for _ in 0..1000 {
            reduce_text_process_with_tree(&smt_tree, "你好，我是中国人");
        }
    });
}

fn main() {}
