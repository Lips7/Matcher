# Benchmark Data

Test data used by benchmarks (`matcher_rs/benches/bench.rs`) and profiling examples.

## Haystacks (`text/`)

| File | Lines | Description |
|------|-------|-------------|
| `text/cn/三体.txt` | ~12K | Chinese novel excerpt (dense CJK text) |
| `text/en/sherlock.txt` | ~13K | English novel excerpt (ASCII-heavy text) |

## Word Lists (`word/`)

| File | Lines | Description |
|------|-------|-------------|
| `word/cn/jieba.txt` | ~349K | Chinese word list (jieba segmentation dictionary) |
| `word/en/dictionary.txt` | ~123K | English word list |

Benchmarks sample subsets of these lists (e.g., 100K patterns) to construct matchers, then scan the corresponding haystack.
