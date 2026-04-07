#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use matcher_rs::{ProcessType, SimpleMatcherBuilder};

#[derive(Arbitrary, Debug)]
struct FuzzInput {
    process_type_bits: u8,
    word_id: u32,
    pattern: String,
    haystack: String,
}

fuzz_target!(|input: FuzzInput| {
    let pt = ProcessType::from_bits_truncate(input.process_type_bits);
    let Ok(matcher) = SimpleMatcherBuilder::new()
        .add_word(pt, input.word_id, &input.pattern)
        .build()
    else {
        return;
    };

    // Exercise both is_match and process — they must not panic.
    let _ = matcher.is_match(&input.haystack);
    let _ = matcher.process(&input.haystack);
});
