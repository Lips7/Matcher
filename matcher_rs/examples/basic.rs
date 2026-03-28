//! Core API tutorial for `matcher_rs`.
//!
//! Run: `cargo run --example basic -p matcher_rs`
//!
//! Covers: builder API, is_match, process, process_into, logical operators, HashMap construction.

use std::collections::HashMap;

use matcher_rs::{ProcessType, SimpleMatcher, SimpleMatcherBuilder, SimpleResult};

fn main() {
    // ── 1. Builder API ──────────────────────────────────────────────────────────
    println!("=== 1. Builder API ===\n");

    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, "hello")
        .add_word(ProcessType::None, 2, "world")
        .add_word(ProcessType::None, 3, "rust")
        .build();

    println!("Built matcher with 3 rules: hello(1), world(2), rust(3)");

    // ── 2. is_match ─────────────────────────────────────────────────────────────
    println!("\n=== 2. is_match ===\n");

    println!("  \"hello world\"  => {}", matcher.is_match("hello world"));
    println!("  \"goodbye\"      => {}", matcher.is_match("goodbye"));
    println!("  \"\"             => {}", matcher.is_match(""));

    // ── 3. process — collect all matching rules ─────────────────────────────────
    println!("\n=== 3. process ===\n");

    let results = matcher.process("hello world, welcome to rust");
    println!("  Text: \"hello world, welcome to rust\"");
    println!("  Matches: {}", results.len());
    for r in &results {
        println!("    word_id={}, word=\"{}\"", r.word_id, r.word);
    }

    // ── 4. process_into — reuse allocation across searches ──────────────────────
    println!("\n=== 4. process_into ===\n");

    let texts = ["hello world", "rust is great", "nothing here", "hello rust"];
    let mut results: Vec<SimpleResult<'_>> = Vec::new();

    for text in texts {
        results.clear();
        matcher.process_into(text, &mut results);
        println!("  \"{text}\" => {} match(es)", results.len());
        for r in &results {
            println!("    word_id={}, word=\"{}\"", r.word_id, r.word);
        }
    }

    // ── 5. Logical operators ────────────────────────────────────────────────────
    println!("\n=== 5. Logical Operators ===\n");

    // AND: both sub-patterns must appear (order-independent)
    let and_matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, "apple&pie")
        .build();

    println!("  Rule: \"apple&pie\" (AND)");
    println!(
        "    \"apple pie\"     => {}",
        and_matcher.is_match("apple pie")
    );
    println!(
        "    \"pie and apple\" => {}",
        and_matcher.is_match("pie and apple")
    );
    println!(
        "    \"apple only\"    => {}",
        and_matcher.is_match("apple only")
    );

    // NOT: match first, veto if second appears
    let not_matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, "banana~peel")
        .build();

    println!("\n  Rule: \"banana~peel\" (NOT)");
    println!(
        "    \"banana split\" => {}",
        not_matcher.is_match("banana split")
    );
    println!(
        "    \"banana peel\"  => {}",
        not_matcher.is_match("banana peel")
    );

    // Combined: AND + NOT
    let combined = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, "fox&jump~lazy")
        .build();

    println!("\n  Rule: \"fox&jump~lazy\" (AND + NOT)");
    println!(
        "    \"the fox can jump\"      => {}",
        combined.is_match("the fox can jump")
    );
    println!(
        "    \"the lazy fox can jump\" => {}",
        combined.is_match("the lazy fox can jump")
    );

    // Count semantics: "a&a" requires at least 2 occurrences
    let count = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, "ha&ha")
        .build();

    println!("\n  Rule: \"ha&ha\" (requires 2 occurrences)");
    println!("    \"ha\"    => {}", count.is_match("ha"));
    println!("    \"ha ha\" => {}", count.is_match("ha ha"));

    // ── 6. HashMap construction (for serde / dynamic scenarios) ─────────────────
    println!("\n=== 6. HashMap Construction ===\n");

    let table: HashMap<ProcessType, HashMap<u32, &str>> = HashMap::from([(
        ProcessType::None,
        HashMap::from([(1, "hello"), (2, "world")]),
    )]);
    let matcher = SimpleMatcher::new(&table);

    println!(
        "  Built from HashMap: is_match(\"hello\") => {}",
        matcher.is_match("hello")
    );

    let results = matcher.process("hello world");
    println!("  process(\"hello world\") => {} matches", results.len());
    for r in &results {
        println!("    word_id={}, word=\"{}\"", r.word_id, r.word);
    }
}
