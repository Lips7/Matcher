# Matcher

A high performance matcher for massive amounts of sensitive words.

## Features

- Supports simple word matching, regex-based matching, and similarity-based matching.
- Supports different text normalization methods for matching.
  - `Fanjian`: Simplify traditional Chinese characters to simplified ones. eg. `蟲艸` -> `虫艹`.
  - `DeleteNormalize`: Delete white spaces, punctuation, and other non-alphanumeric characters. eg. `𝜢𝕰𝕃𝙻Ϙ 𝙒ⓞƦℒ𝒟!` -> `helloworld`.
  - `PinYin`: Convert Chinese characters to Pinyin, which can be used for fuzzy matching. eg. `西安` -> `/xi//an/`, will match `洗按` -> `/xi//an/`, but won't match `先` -> `/xian/`.
  - `PinYinChar`: Convert Chinese characters to Pinyin, which can be used for fuzzy matching. eg. `西安` -> `xian`, will match `洗按` -> `xian` and `先` -> `xian`.
- Supports combination word matching and repeated word matching. eg. `hello,world` will match `hello world` and `world,hello`, `无,法,无,天` will match `无无法天` but won't match `无法天` because `无` repeated two times int the word.
- Supports customizable exemption lists to exclude certain words from matching.
- Can handle large amounts of sensitive words efficiently.

## Limitations

- Matchers can only handle words containing no more than 32 combined words and no more than 8 repeated words.
- It's user's resposibility to ensure the correctness of the input data and ensure `match_id`, `table_id`, `word_id` are glabally unique.

## Usage

- For none rust user, you have to use **msgpack** to serialze matcher config to bytes.
- Why msgpack? Why not json? Because json can't handle back slash well, eg. `It's /\/\y duty`, it will be processed incorrectly if using json, and msgpack is faster than json.

### For Rust User

See [Rust Readme](./matcher_rs/README.md)

### For Python User

See [Python Readme](./matcher_py/README.md)

### For Java User

Install rust, git clone this repo, run `cargo build --release`, and copy the `target/release/libmatcher.so` or `target/release/libmatcher.dylib` if you are using mac, to `matcher_java/src/resources/matcher_c.so`.

See [Java Readme](./matcher_java/README.md)

### For C User

Install rust, git clone this repo, run `cargo build --release`, and copy the `target/release/libmatcher.so` or `target/release/libmatcher.dylib` if you are using mac, to `matcher_c/matcher_c.so`.

See [C Readme](./matcher_c/README.md)

## Design

Currently most features are besed on [aho_corasick](https://github.com/BurntSushi/aho-corasick), which provides ability to find occurrences of many patterns at once with SIMD acceleration in some cases.

For more implement details, see [Design](./DESIGN.md).