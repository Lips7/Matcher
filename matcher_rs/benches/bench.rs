use criterion::{black_box, criterion_group, criterion_main, Criterion};
use gxhash::HashMap as GxHashMap;
use nohash_hasher::IntMap;

use matcher_rs::*;

fn bench(c: &mut Criterion) {
    let match_table_map = GxHashMap::from_iter([(
        "test",
        vec![MatchTable {
            table_id: 1,
            match_table_type: MatchTableType::Simple,
            simple_match_type: SimpleMatchType::FanjianDeleteNormalize,
            word_list: vec!["你好,123"],
            exemption_simple_match_type: SimpleMatchType::FanjianDeleteNormalize,
            exemption_word_list: vec![],
        }],
    )]);
    let matcher = Matcher::new(&match_table_map);

    c.bench_function("matcher_build", |b| {
        b.iter(|| Matcher::new(&match_table_map))
    });
    c.bench_function("word_match_super_long_text", |b| {
        b.iter(|| matcher.word_match(black_box("Dolore anim aliquip magna adipisicing excepteur adipisicing minim dolor non dolore labore veniam. Ex cillum dolore quis nulla. Laboris officia qui consectetur laboris nulla et Lorem qui anim in eu. Laboris ea tempor qui ullamco irure culpa. Elit duis laborum dolor voluptate duis. Enim exercitation adipisicing esse. Cupidatat do occaecat ullamco adipisicing deserunt sunt Lorem ad veniam ullamco aute anim id. Dolore anim aliquip magna adipisicing excepteur adipisicing minim dolor non dolore labore veniam. Ex cillum dolore quis nulla. Laboris officia qui consectetur laboris nulla et Lorem qui anim in eu. Laboris ea tempor qui ullamco irure culpa. Elit duis laborum dolor voluptate duis. Enim exercitation adipisicing esse. Cupidatat do occaecat ullamco adipisicing deserunt sunt Lorem ad veniam ullamco aute anim id. Dolore anim aliquip magna adipisicing excepteur adipisicing minim dolor non dolore labore veniam. Ex cillum dolore quis nulla. Laboris officia qui consectetur laboris nulla et Lorem qui anim in eu. Laboris ea tempor qui ullamco irure culpa. Elit duis laborum dolor voluptate duis. Enim exercitation adipisicing esse. Cupidatat do occaecat ullamco adipisicing deserunt sunt Lorem ad veniam ullamco aute anim id.")))
    });
    c.bench_function("word_match_long_text", |b| {
        b.iter(|| matcher.word_match(black_box("Dolore anim aliquip magna adipisicing excepteur adipisicing minim dolor non dolore labore veniam. Ex cillum dolore quis nulla. Laboris officia qui consectetur laboris nulla et Lorem qui anim in eu. Laboris ea tempor qui ullamco irure culpa. Elit duis laborum dolor voluptate duis. Enim exercitation adipisicing esse. Cupidatat do occaecat ullamco adipisicing deserunt sunt Lorem ad veniam ullamco aute anim id.")))
    });
    c.bench_function("word_match_hit_text", |b| {
        b.iter(|| matcher.word_match(black_box("1dsa你好,12312das")))
    });
    c.bench_function("word_match_short_text", |b| {
        b.iter(|| matcher.word_match(black_box("你好")))
    });
    c.bench_function("word_match_empty_text", |b| {
        b.iter(|| matcher.word_match(black_box("")))
    });

    let simple_word_list_dict = GxHashMap::from_iter([(
        SimpleMatchType::FanjianDeleteNormalize,
        IntMap::from_iter([(1, "你好，123")]),
    )]);
    let simple_matcher = SimpleMatcher::new(&simple_word_list_dict);

    c.bench_function("simple_matcher_build", |b| {
        b.iter(|| SimpleMatcher::new(&simple_word_list_dict))
    });
    c.bench_function("simple_process_super_long_text", |b| {
        b.iter(|| simple_matcher.process(black_box("Dolore anim aliquip magna adipisicing excepteur adipisicing minim dolor non dolore labore veniam. Ex cillum dolore quis nulla. Laboris officia qui consectetur laboris nulla et Lorem qui anim in eu. Laboris ea tempor qui ullamco irure culpa. Elit duis laborum dolor voluptate duis. Enim exercitation adipisicing esse. Cupidatat do occaecat ullamco adipisicing deserunt sunt Lorem ad veniam ullamco aute anim id. Dolore anim aliquip magna adipisicing excepteur adipisicing minim dolor non dolore labore veniam. Ex cillum dolore quis nulla. Laboris officia qui consectetur laboris nulla et Lorem qui anim in eu. Laboris ea tempor qui ullamco irure culpa. Elit duis laborum dolor voluptate duis. Enim exercitation adipisicing esse. Cupidatat do occaecat ullamco adipisicing deserunt sunt Lorem ad veniam ullamco aute anim id. Dolore anim aliquip magna adipisicing excepteur adipisicing minim dolor non dolore labore veniam. Ex cillum dolore quis nulla. Laboris officia qui consectetur laboris nulla et Lorem qui anim in eu. Laboris ea tempor qui ullamco irure culpa. Elit duis laborum dolor voluptate duis. Enim exercitation adipisicing esse. Cupidatat do occaecat ullamco adipisicing deserunt sunt Lorem ad veniam ullamco aute anim id.")))
    });
    c.bench_function("simple_process_long_text", |b| {
        b.iter(|| simple_matcher.process(black_box("Dolore anim aliquip magna adipisicing excepteur adipisicing minim dolor non dolore labore veniam. Ex cillum dolore quis nulla. Laboris officia qui consectetur laboris nulla et Lorem qui anim in eu. Laboris ea tempor qui ullamco irure culpa. Elit duis laborum dolor voluptate duis. Enim exercitation adipisicing esse. Cupidatat do occaecat ullamco adipisicing deserunt sunt Lorem ad veniam ullamco aute anim id.")))
    });
    c.bench_function("simple_process_hit_text", |b| {
        b.iter(|| simple_matcher.process(black_box("1dsa你好,12312das")))
    });
    c.bench_function("simple_process_short_text", |b| {
        b.iter(|| simple_matcher.process(black_box("你好")))
    });
    c.bench_function("simple_process_empty_text", |b| {
        b.iter(|| simple_matcher.process(black_box("")))
    });
}

criterion_group! {
    name = benches;
    config = Criterion::default().significance_level(0.05).sample_size(1000);
    targets = bench
}
criterion_main!(benches);
