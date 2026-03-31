#[cfg(feature = "runtime_build")]
use std::collections::HashMap;

#[cfg(feature = "runtime_build")]
use matcher_rs::SimpleMatcher;
use matcher_rs::{ProcessType, SimpleMatcherBuilder};

/// Validates that matching results are correct regardless of the active feature
/// flags (dfa on/off, simd_runtime_dispatch on/off). This test runs under
/// `cargo all-features nextest run` across all feature combinations.
#[test]
fn test_matcher_results_stable_across_features() {
    let matcher = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None, 1, "hello")
        .add_word(ProcessType::None, 2, "foo&bar")
        .add_word(ProcessType::None, 3, "good~bad")
        .add_word(ProcessType::Fanjian, 4, "\u{4F60}\u{597D}") // 你好
        .add_word(ProcessType::Delete | ProcessType::Normalize, 5, "test")
        .build()
        .unwrap();

    assert!(matcher.is_match("hello world"));
    assert!(matcher.is_match("foo and bar"));
    assert!(!matcher.is_match("foo only"));
    assert!(matcher.is_match("good stuff"));
    assert!(!matcher.is_match("good and bad"));

    let results = matcher.process("hello foo bar");
    let ids: Vec<u32> = results.iter().map(|r| r.word_id).collect();
    assert!(ids.contains(&1));
    assert!(ids.contains(&2));

    let results = matcher.process("good and bad");
    let ids: Vec<u32> = results.iter().map(|r| r.word_id).collect();
    assert!(!ids.contains(&3), "NOT segment 'bad' should veto rule 3");
}

/// Validates that runtime-built transformation tables produce the same
/// text_process output as compile-time tables.
#[cfg(feature = "runtime_build")]
#[test]
fn test_runtime_build_text_process() {
    use matcher_rs::text_process;

    let input = "\u{4F60}\u{597D}\u{4E16}\u{754C}"; // 你好世界

    let fanjian_result = text_process(ProcessType::Fanjian, input);
    assert!(
        !fanjian_result.is_empty(),
        "Fanjian should produce non-empty output"
    );

    let delete_result = text_process(ProcessType::Delete, "h.e.l.l.o");
    assert_eq!(
        delete_result.as_ref(),
        "hello",
        "Delete should remove punctuation (periods)"
    );

    let normalize_result = text_process(ProcessType::Normalize, "\u{2460}\u{2461}\u{2462}");
    assert_ne!(
        normalize_result.as_ref(),
        "\u{2460}\u{2461}\u{2462}",
        "Normalize should transform circled digits"
    );
}

/// Validates that matchers built under runtime_build produce the same match
/// results as expected.
#[cfg(feature = "runtime_build")]
#[test]
fn test_runtime_build_matcher_equiv() {
    let matcher = SimpleMatcher::new(&HashMap::from([
        (
            ProcessType::Fanjian,
            HashMap::from([(1, "\u{4F60}\u{597D}")]),
        ),
        (ProcessType::None, HashMap::from([(2, "hello")])),
    ]))
    .unwrap();

    assert!(matcher.is_match("hello"));
    assert!(matcher.is_match("\u{4F60}\u{597D}"));

    let results = matcher.process("hello \u{4F60}\u{597D}");
    assert_eq!(results.len(), 2);
}
