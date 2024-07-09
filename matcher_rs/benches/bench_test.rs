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

    bencher.bench(|| {
        let mut _s = 0;
        for x in ac.find_overlapping_iter(black_box("12321421asddaw你我")) {
            _s += x.pattern().as_usize();
        }
    })
}

#[divan::bench]
fn bench_test_2(bencher: Bencher) {
    let ac = AhoCorasickBuilder::new()
        .kind(Some(AhoCorasickKind::DFA))
        .match_kind(MatchKind::Standard)
        .ascii_case_insensitive(true)
        .build(["我", "我"])
        .unwrap();

    bencher.bench(|| {
        let mut _s = 0;
        for x in ac.find_overlapping_iter(black_box(
            "12321421asddaw你我",
        )) {
            _s += x.pattern().as_usize();
        }
        for x in ac.find_overlapping_iter(black_box(
            "12321421asddaw你我",
        )) {
            _s += x.pattern().as_usize();
        }
    })
}

fn main() {
    divan::main()
}
