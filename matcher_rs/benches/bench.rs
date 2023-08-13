use ahash::AHashMap;
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use zerovec::VarZeroVec;

use matcher_rs::*;

fn bench(c: &mut Criterion) {
    let match_table_dict = AHashMap::from([(
        "test",
        vec![MatchTable {
            table_id: 1,
            match_table_type: MatchTableType::Simple,
            wordlist: VarZeroVec::from(&["你好,123"]),
            exemption_wordlist: VarZeroVec::new(),
            simple_match_type: SimpleMatchType::FanjianDeleteNormalize,
        }],
    )]);
    let matcher = Matcher::new(&match_table_dict);

    c.bench_function("matcher_build", |b| {
        b.iter(|| Matcher::new(&match_table_dict))
    });
    c.bench_function("word_match_super_long_text", |b| {
        b.iter(|| matcher.word_match(black_box("dsahbdj12pu980-120opo[sad[d]pas;l[;'.,zmc;as'k[aepe所有的沙发博客看后289UI哈哈不可得兼萨马拉州，女把wejlhjp0iidasbwdjksabfadghjaklsekjniwh123powhudbasbasmdsal,d.as,dlasfjsaifjbo39p9eu12p0poaspopofjsapdaksdpsa【】萨达省；c'xzlk.asd，萨。，但马上，队列即可领取王杰饿哦啥屁；但那是没法解开了吗你只需龙祥怎么了华北地区房东啥尽快帮我去IE请问i两节课大赛不好发不出吗你只需把vaf打死就不会发生的旅程啊，sd阿斯顿啥都怕是个大傻大叔的吧到那时  dsabjx· ds····           巴士到家啦vxzmdm")))
    });
    c.bench_function("word_match_long_text", |b| {
        b.iter(|| matcher.word_match(black_box("gasbhkjdbsauhjkv不就代表沙发就卡死，倍去我空间恶化就啊不对劲啊是贵宾卡我了，没了叫你起床加巴西办公室就看到，nhrqjmwjhxb 吃了好几遍五块钱2，恶魔发微博")))
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

    let simple_wordlist_dict = AHashMap::from([(
        SimpleMatchType::FanjianDeleteNormalize,
        vec![SimpleWord {
            word_id: 1,
            word: "你好,123",
        }],
    )]);
    let simple_matcher = SimpleMatcher::new(&simple_wordlist_dict);

    c.bench_function("simple_matcher_build", |b| {
        b.iter(|| SimpleMatcher::new(&simple_wordlist_dict))
    });
    c.bench_function("simple_process_super_long_text", |b| {
        b.iter(|| simple_matcher.process(black_box("dsahbdj12pu980-120opo[sad[d]pas;l[;'.,zmc;as'k[aepe所有的沙发博客看后289UI哈哈不可得兼萨马拉州，女把wejlhjp0iidasbwdjksabfadghjaklsekjniwh123powhudbasbasmdsal,d.as,dlasfjsaifjbo39p9eu12p0poaspopofjsapdaksdpsa【】萨达省；c'xzlk.asd，萨。，但马上，队列即可领取王杰饿哦啥屁；但那是没法解开了吗你只需龙祥怎么了华北地区房东啥尽快帮我去IE请问i两节课大赛不好发不出吗你只需把vaf打死就不会发生的旅程啊，sd阿斯顿啥都怕是个大傻大叔的吧到那时  dsabjx· ds····           巴士到家啦vxzmdm")))
    });
    c.bench_function("simple_process_long_text", |b| {
        b.iter(|| simple_matcher.process(black_box("gasbhkjdbsauhjkv不就代表沙发就卡死，倍去我空间恶化就啊不对劲啊是贵宾卡我了，没了叫你起床加巴西办公室就看到，nhrqjmwjhxb 吃了好几遍五块钱2，恶魔发微博")))
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
