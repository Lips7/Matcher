//! Core API tutorial for `matcher_rs`.
//!
//! Run: `cargo run --example basic -p matcher_rs`
//!
//! Covers: builder API, is_match, process, process_into, for_each_match,
//! find_match, process_iter, logical operators, HashMap construction.

use std::collections::HashMap;

use matcher_rs::{ProcessType, SimpleMatcher, SimpleMatcherBuilder, SimpleResult};

fn main() {
    // ── 1. Builder API ──────────────────────────────────────────────────────────
    println!("=== 1. Builder API ===\n");

    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, "hello")
        .add_word(ProcessType::None, 2, "world")
        .add_word(ProcessType::None, 3, "rust")
        .build()
        .unwrap();

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

    // ── 5. for_each_match — zero-allocation callback ─────────────────────────
    println!("\n=== 5. for_each_match ===\n");

    let mut ids = Vec::new();
    matcher.for_each_match("hello world, welcome to rust", |r| {
        ids.push(r.word_id);
        false // continue scanning
    });
    println!("  Matched word_ids: {ids:?}");

    // Early exit: stop after first match
    let mut first_only = Vec::new();
    let stopped = matcher.for_each_match("hello world", |r| {
        first_only.push(r.word_id);
        true // stop after first
    });
    println!("  Early exit (stopped={stopped}): {first_only:?}");

    // ── 6. find_match — first matching rule ────────────────────────────────────
    println!("\n=== 6. find_match ===\n");

    if let Some(r) = matcher.find_match("hello world") {
        println!("  First match: word_id={}, word=\"{}\"", r.word_id, r.word);
    }
    println!("  No match: {:?}", matcher.find_match("no keywords here"));

    // ── 7. process_iter — composable iterator ──────────────────────────────────
    println!("\n=== 7. process_iter ===\n");

    let iter = matcher.process_iter("hello world, welcome to rust");
    println!("  Iterator length: {}", iter.len());
    for r in iter {
        println!("    word_id={}, word=\"{}\"", r.word_id, r.word);
    }

    // Iterator combinators
    let first_two: Vec<_> = matcher
        .process_iter("hello world, welcome to rust")
        .take(2)
        .collect();
    println!("  First 2 via .take(2): {:?}", first_two.len());

    // ── 8. Logical operators ────────────────────────────────────────────────────
    println!("\n=== 8. Logical Operators ===\n");

    // AND: both sub-patterns must appear (order-independent)
    let and_matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, "apple&pie")
        .build()
        .unwrap();

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
        .build()
        .unwrap();

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
        .build()
        .unwrap();

    println!("\n  Rule: \"fox&jump~lazy\" (AND + NOT)");
    println!(
        "    \"the fox can jump\"      => {}",
        combined.is_match("the fox can jump")
    );
    println!(
        "    \"the lazy fox can jump\" => {}",
        combined.is_match("the lazy fox can jump")
    );

    // OR: any alternative matches the segment
    let or_matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, "color|colour")
        .build()
        .unwrap();

    println!("\n  Rule: \"color|colour\" (OR)");
    println!(
        "    \"nice color\"  => {}",
        or_matcher.is_match("nice color")
    );
    println!(
        "    \"nice colour\" => {}",
        or_matcher.is_match("nice colour")
    );
    println!("    \"nice hue\"    => {}", or_matcher.is_match("nice hue"));

    // Word boundary: \b restricts to whole-word matches
    let boundary_matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, r"\bcat\b")
        .build()
        .unwrap();

    println!("\n  Rule: \"\\bcat\\b\" (word boundary)");
    println!(
        "    \"the cat sat\"  => {}",
        boundary_matcher.is_match("the cat sat")
    );
    println!(
        "    \"concatenate\"  => {}",
        boundary_matcher.is_match("concatenate")
    );
    println!(
        "    \"cats and dogs\" => {}",
        boundary_matcher.is_match("cats and dogs")
    );

    // Count semantics: "a&a" requires at least 2 occurrences
    let count = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, "ha&ha")
        .build()
        .unwrap();

    println!("\n  Rule: \"ha&ha\" (requires 2 occurrences)");
    println!("    \"ha\"    => {}", count.is_match("ha"));
    println!("    \"ha ha\" => {}", count.is_match("ha ha"));

    // Combined: AND + OR + NOT + word boundary
    // "bright&color|colour~\bdark\b" means:
    //   - must contain "bright" AND ("color" OR "colour")
    //   - must NOT contain "dark" as a whole word
    let full = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, r"bright&color|colour~\bdark\b")
        .build()
        .unwrap();

    println!("\n  Rule: \"bright&color|colour~\\bdark\\b\" (AND + OR + NOT + boundary)");
    println!(
        "    \"bright colour\"      => {}",
        full.is_match("bright colour")
    );
    println!(
        "    \"bright color\"       => {}",
        full.is_match("bright color")
    );
    println!(
        "    \"bright dark color\"  => {}",
        full.is_match("bright dark color")
    );
    println!(
        "    \"bright darken color\" => {}",
        full.is_match("bright darken color")
    );

    // ── 9. HashMap construction (for serde / dynamic scenarios) ─────────────────
    println!("\n=== 9. HashMap Construction ===\n");

    let table: HashMap<ProcessType, HashMap<u32, &str>> = HashMap::from([(
        ProcessType::None,
        HashMap::from([(1, "hello"), (2, "world")]),
    )]);
    let matcher = SimpleMatcher::new(&table).unwrap();

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
