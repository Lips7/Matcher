//! Text transformation pipeline examples for `matcher_rs`.
//!
//! Run: `cargo run --example transforms -p matcher_rs`
//!
//! Covers: individual ProcessTypes, composite types, text_process, reduce_text_process,
//! reduce_text_process_emit, cross-variant matching.

use matcher_rs::{
    ProcessType, SimpleMatcherBuilder, reduce_text_process, reduce_text_process_emit, text_process,
};

fn main() {
    // ── 1. Individual ProcessTypes ──────────────────────────────────────────────
    println!("=== 1. Individual ProcessTypes ===\n");

    // VariantNorm: Traditional Chinese → Simplified Chinese
    println!("  [VariantNorm] Traditional → Simplified");
    println!(
        "    text_process(\"測試臺灣\") => \"{}\"",
        text_process(ProcessType::VariantNorm, "測試臺灣")
    );
    let m = SimpleMatcherBuilder::new()
        .add_word(ProcessType::VariantNorm, 1, "测试")
        .build()
        .unwrap();
    println!(
        "    matcher(\"测试\").is_match(\"測試臺灣\") => {}",
        m.is_match("測試臺灣")
    );

    // Delete: remove configured codepoints (punctuation, separators, controls, emoji, etc.)
    println!("\n  [Delete] Strip noise characters");
    let delete_input = "h.e\u{201C}l.l\u{201D}o";
    println!(
        "    text_process(\"{delete_input}\") => \"{}\"",
        text_process(ProcessType::Delete, delete_input)
    );
    let m = SimpleMatcherBuilder::new()
        .add_word(ProcessType::Delete, 1, "hello")
        .build()
        .unwrap();
    println!(
        "    matcher(\"hello\").is_match(\"h e l l o\") => {}",
        m.is_match("h e l l o")
    );

    // Normalize: replacement-table normalization (diacritics, Unicode variants)
    println!("\n  [Normalize] Unicode normalization");
    println!(
        "    text_process(\"Ａｂｃ\") => \"{}\"",
        text_process(ProcessType::Normalize, "Ａｂｃ")
    );
    let m = SimpleMatcherBuilder::new()
        .add_word(ProcessType::Normalize, 1, "abc")
        .build()
        .unwrap();
    println!(
        "    matcher(\"abc\").is_match(\"Ａｂｃ\") => {}",
        m.is_match("Ａｂｃ")
    );

    // Romanize: Chinese character → space-separated romanize syllables
    println!("\n  [Romanize] Character → Romanize syllables (space-separated)");
    let py = text_process(ProcessType::Romanize, "中国");
    println!("    text_process(\"中国\") => \"{py}\"");
    let m = SimpleMatcherBuilder::new()
        .add_word(ProcessType::Romanize, 1, "zhong")
        .build()
        .unwrap();
    println!(
        "    matcher(\"zhong\").is_match(\"中国\") => {}",
        m.is_match("中国")
    );

    // RomanizeChar: Chinese character → concatenated romanize (no spaces)
    println!("\n  [RomanizeChar] Character → Romanize (concatenated)");
    let pyc = text_process(ProcessType::RomanizeChar, "中国");
    println!("    text_process(\"中国\") => \"{pyc}\"");
    let m = SimpleMatcherBuilder::new()
        .add_word(ProcessType::RomanizeChar, 1, "zhongguo")
        .build()
        .unwrap();
    println!(
        "    matcher(\"zhongguo\").is_match(\"中国\") => {}",
        m.is_match("中国")
    );

    // EmojiNorm: emoji → English words (CLDR short names), strip modifiers
    println!("\n  [EmojiNorm] Emoji → English words");
    let emoji_input = "I love 🔥 and 👍🏽!";
    println!(
        "    text_process(\"{emoji_input}\") => \"{}\"",
        text_process(ProcessType::EmojiNorm, emoji_input)
    );
    let m = SimpleMatcherBuilder::new()
        .add_word(ProcessType::EmojiNorm, 1, "fire")
        .build()
        .unwrap();
    println!(
        "    matcher(\"fire\").is_match(\"🔥\") => {}",
        m.is_match("🔥")
    );

    // ── 2. Composite ProcessTypes ───────────────────────────────────────────────
    println!("\n=== 2. Composite ProcessTypes ===\n");

    // VariantNormDeleteNormalize: full pipeline
    println!("  [VariantNormDeleteNormalize] Full pipeline");
    let input = "測！試Ａ";
    println!(
        "    text_process(\"{input}\") => \"{}\"",
        text_process(ProcessType::VariantNormDeleteNormalize, input)
    );

    // Custom composite via | operator: match both raw and VariantNorm-converted text
    println!("\n  [None | VariantNorm] Match raw OR converted");
    let m = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None | ProcessType::VariantNorm, 1, "测试")
        .build()
        .unwrap();
    println!(
        "    is_match(\"测试\")   => {} (raw match)",
        m.is_match("测试")
    );
    println!(
        "    is_match(\"測試\")   => {} (VariantNorm match)",
        m.is_match("測試")
    );
    println!("    is_match(\"hello\") => {}", m.is_match("hello"));

    // ── 3. reduce_text_process — inspect every intermediate step ────────────────
    println!("\n=== 3. reduce_text_process ===\n");

    let input = "~測~Ａ~";
    println!("  Input: \"{input}\"");
    println!("  Pipeline: VariantNormDeleteNormalize (VariantNorm → Delete → Normalize)\n");

    let variants = reduce_text_process(ProcessType::VariantNormDeleteNormalize, input);
    for (i, v) in variants.iter().enumerate() {
        let label = match i {
            0 => "original",
            1 => "after VariantNorm",
            2 => "after Delete",
            3 => "after Normalize",
            _ => "unknown",
        };
        println!("    [{i}] {label}: \"{}\"", v);
    }

    // ── 4. reduce_text_process_emit — emitted variant semantics ─────────────────
    println!("\n=== 4. reduce_text_process_emit ===\n");

    println!("  Input: \"{input}\"");
    println!("  Pipeline: VariantNormDeleteNormalize\n");
    println!("  Emit semantics: replace-steps (VariantNorm, Normalize) overwrite in-place;");
    println!("  Delete appends a new entry (creates a new scan boundary).\n");

    let emitted = reduce_text_process_emit(ProcessType::VariantNormDeleteNormalize, input);
    for (i, v) in emitted.iter().enumerate() {
        println!("    [{i}] \"{}\"", v);
    }
    println!(
        "\n  ({} entries vs {} from reduce_text_process)",
        emitted.len(),
        variants.len()
    );

    // ── 5. Cross-variant matching ───────────────────────────────────────────────
    println!("\n=== 5. Cross-Variant Matching ===\n");

    // An AND rule where one sub-pattern matches raw text and
    // another matches via the VariantNorm-converted variant.
    let m = SimpleMatcherBuilder::new()
        .add_word(
            ProcessType::None | ProcessType::VariantNorm,
            1,
            "hello&测试",
        )
        .build()
        .unwrap();

    println!("  Rule: \"hello&测试\" under None | VariantNorm");
    println!(
        "    is_match(\"hello 測試\") => {}",
        m.is_match("hello 測試")
    );
    println!(
        "    is_match(\"hello 测试\") => {}",
        m.is_match("hello 测试")
    );
    println!(
        "    is_match(\"hello world\") => {}",
        m.is_match("hello world")
    );

    // Zero-allocation when no change occurs
    println!("\n  [Cow optimization]");
    let result = text_process(ProcessType::VariantNorm, "pure ascii");
    println!(
        "    text_process(VariantNorm, \"pure ascii\") is borrowed: {}",
        matches!(result, std::borrow::Cow::Borrowed(_))
    );
}
