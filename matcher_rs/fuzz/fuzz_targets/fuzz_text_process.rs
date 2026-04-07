#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use matcher_rs::{text_process, reduce_text_process, ProcessType};

#[derive(Arbitrary, Debug)]
struct FuzzInput {
    process_type_bits: u8,
    text: String,
}

fuzz_target!(|input: FuzzInput| {
    let pt = ProcessType::from_bits_truncate(input.process_type_bits);

    // Exercise all public text-processing paths — they must not panic.
    let _ = text_process(pt, &input.text);
    let _ = reduce_text_process(pt, &input.text);
});
