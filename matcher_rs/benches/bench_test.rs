use std::collections::HashMap;

use aho_corasick_unsafe::{AhoCorasickBuilder, AhoCorasickKind, MatchKind};
use divan::{black_box, Bencher};

#[divan::bench]
fn bench_test_1(bencher: Bencher) {
    let ac = AhoCorasickBuilder::new()
        .kind(Some(AhoCorasickKind::DFA))
        .match_kind(MatchKind::Standard)
        .ascii_case_insensitive(true)
        .build(["我", "我", "我", "我"])
        .unwrap();

    let m1 = HashMap::from([(0, 1), (1, 2), (2, 3), (3, 4)]);

    bencher.bench(|| {
        let mut m2 = HashMap::from([(1, 0), (2, 0), (3, 0), (4, 0)]);
        for x in ac.find_overlapping_iter(black_box("12321421asddaw你我")) {
            let s = m2.get_mut(m1.get(&x.pattern().as_i32()).unwrap()).unwrap();
            *s += 1;
        }
    })
}

#[divan::bench]
fn bench_test_2(bencher: Bencher) {
    let ac = AhoCorasickBuilder::new()
        .kind(Some(AhoCorasickKind::DFA))
        .match_kind(MatchKind::Standard)
        .ascii_case_insensitive(true)
        .build(["我"])
        .unwrap();

    let m1 = [vec![1, 2, 3, 4]];

    bencher.bench(|| {
        let mut m2 = HashMap::from([(1, 0), (2, 0), (3, 0), (4, 0)]);
        for x in ac.find_overlapping_iter(black_box("12321421asddaw你我")) {
            for index in m1.get(x.pattern().as_usize()).unwrap() {
                let s = m2.get_mut(index).unwrap();
                *s += 1;
            }
        }
    })
}

fn main() {
    divan::main()
}
