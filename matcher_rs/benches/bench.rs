use ahash::AHashMap;
use aho_corasick::{AhoCorasickBuilder, AhoCorasickKind::DFA, MatchKind as AhoCorasickMatchKind};
use criterion::{criterion_group, criterion_main, Criterion};
use daachorse::{
    CharwiseDoubleArrayAhoCorasick, CharwiseDoubleArrayAhoCorasickBuilder,
    MatchKind as DoubleArrayAhoCorasickMatchKind,
};

const FANJIAN: &str = include_str!("../str_conv_map/FANJIAN.txt");
const UNICODE: &str = include_str!("../str_conv_map/UNICODE.txt");
const THREE_BODY: &str = include_str!("../../data/text/cn/三体.txt");

fn bench(c: &mut Criterion) {
    let mut test_map = AHashMap::new();
    for str_conv_map in [FANJIAN, UNICODE] {
        test_map.extend(str_conv_map.trim().lines().map(|pair_str| {
            let mut pair_str_split = pair_str.split('\t');
            (
                pair_str_split.next().unwrap(),
                pair_str_split.next().unwrap(),
            )
        }));
    }

    let matcher = AhoCorasickBuilder::new()
        .kind(Some(DFA))
        .match_kind(AhoCorasickMatchKind::Standard)
        .build(test_map.iter().map(|(&key, _)| key).collect::<Vec<&str>>())
        .unwrap();
    c.bench_function("fanjian_ahocorasick", |b| {
        b.iter(|| {
            for line in THREE_BODY.lines() {
                matcher.find_iter(line).count();
            }
        })
    });

    c.bench_function("fanjian_daachorse_charwise_u64", |b| {
        b.iter(|| {
            for line in THREE_BODY.lines() {
                matcher.find_iter(line).count();
            }
        })
    });

    c.bench_function("fanjian_build_ahocorasick", |b| {
        b.iter(|| {
            let _ = AhoCorasickBuilder::new()
                .kind(Some(DFA))
                .match_kind(AhoCorasickMatchKind::Standard)
                .build(test_map.iter().map(|(&key, _)| key).collect::<Vec<&str>>())
                .unwrap();
        })
    });
    c.bench_function("fanjian_build_daachorse_charwise_u64", |b| {
        b.iter(|| {
            let _: CharwiseDoubleArrayAhoCorasick<u64> =
                CharwiseDoubleArrayAhoCorasickBuilder::new()
                    .match_kind(DoubleArrayAhoCorasickMatchKind::Standard)
                    .build(test_map.iter().map(|(&key, _)| key).collect::<Vec<&str>>())
                    .unwrap();
        })
    });
}

criterion_group! {
    name = benches;
    config = Criterion::default().significance_level(0.05).sample_size(100);
    targets = bench
}
criterion_main!(benches);
