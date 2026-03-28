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

    // Fanjian: Traditional Chinese → Simplified Chinese
    println!("  [Fanjian] Traditional → Simplified");
    println!(
        "    text_process(\"妳好世界\") => \"{}\"",
        text_process(ProcessType::Fanjian, "妳好世界")
    );
    let m = SimpleMatcherBuilder::new()
        .add_word(ProcessType::Fanjian, 1, "你好")
        .build();
    println!(
        "    matcher(\"你好\").is_match(\"妳好世界\") => {}",
        m.is_match("妳好世界")
    );

    // Delete: remove configured codepoints (whitespace, punctuation, emoji, etc.)
    println!("\n  [Delete] Strip noise characters");
    let delete_input = "h.e\u{201C}l.l\u{201D}o";
    println!(
        "    text_process(\"{delete_input}\") => \"{}\"",
        text_process(ProcessType::Delete, delete_input)
    );
    let m = SimpleMatcherBuilder::new()
        .add_word(ProcessType::Delete, 1, "hello")
        .build();
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
        .build();
    println!(
        "    matcher(\"abc\").is_match(\"Ａｂｃ\") => {}",
        m.is_match("Ａｂｃ")
    );

    // PinYin: Chinese character → space-separated pinyin syllables
    println!("\n  [PinYin] Character → Pinyin syllables (space-separated)");
    let py = text_process(ProcessType::PinYin, "中国");
    println!("    text_process(\"中国\") => \"{py}\"");
    let m = SimpleMatcherBuilder::new()
        .add_word(ProcessType::PinYin, 1, "zhong")
        .build();
    println!(
        "    matcher(\"zhong\").is_match(\"中国\") => {}",
        m.is_match("中国")
    );

    // PinYinChar: Chinese character → concatenated pinyin (no spaces)
    println!("\n  [PinYinChar] Character → Pinyin (concatenated)");
    let pyc = text_process(ProcessType::PinYinChar, "中国");
    println!("    text_process(\"中国\") => \"{pyc}\"");
    let m = SimpleMatcherBuilder::new()
        .add_word(ProcessType::PinYinChar, 1, "zhongguo")
        .build();
    println!(
        "    matcher(\"zhongguo\").is_match(\"中国\") => {}",
        m.is_match("中国")
    );

    // ── 2. Composite ProcessTypes ───────────────────────────────────────────────
    println!("\n=== 2. Composite ProcessTypes ===\n");

    // FanjianDeleteNormalize: full pipeline
    println!("  [FanjianDeleteNormalize] Full pipeline");
    let input = "妳！好Ａ";
    println!(
        "    text_process(\"{input}\") => \"{}\"",
        text_process(ProcessType::FanjianDeleteNormalize, input)
    );

    // Custom composite via | operator: match both raw and Fanjian-converted text
    println!("\n  [None | Fanjian] Match raw OR converted");
    let m = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None | ProcessType::Fanjian, 1, "你好")
        .build();
    println!(
        "    is_match(\"你好\")   => {} (raw match)",
        m.is_match("你好")
    );
    println!(
        "    is_match(\"妳好\")   => {} (Fanjian match)",
        m.is_match("妳好")
    );
    println!("    is_match(\"hello\") => {}", m.is_match("hello"));

    // ── 3. reduce_text_process — inspect every intermediate step ────────────────
    println!("\n=== 3. reduce_text_process ===\n");

    let input = "~躶~Ａ~";
    println!("  Input: \"{input}\"");
    println!("  Pipeline: FanjianDeleteNormalize (Fanjian → Delete → Normalize)\n");

    let variants = reduce_text_process(ProcessType::FanjianDeleteNormalize, input);
    for (i, v) in variants.iter().enumerate() {
        let label = match i {
            0 => "original",
            1 => "after Fanjian",
            2 => "after Delete",
            3 => "after Normalize",
            _ => "unknown",
        };
        println!("    [{i}] {label}: \"{}\"", v);
    }

    // ── 4. reduce_text_process_emit — emitted variant semantics ─────────────────
    println!("\n=== 4. reduce_text_process_emit ===\n");

    println!("  Input: \"{input}\"");
    println!("  Pipeline: FanjianDeleteNormalize\n");
    println!("  Emit semantics: replace-steps (Fanjian, Normalize) overwrite in-place;");
    println!("  Delete appends a new entry (creates a new scan boundary).\n");

    let emitted = reduce_text_process_emit(ProcessType::FanjianDeleteNormalize, input);
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
    // another matches via the Fanjian-converted variant.
    let m = SimpleMatcherBuilder::new()
        .add_word(ProcessType::None | ProcessType::Fanjian, 1, "hello&你好")
        .build();

    println!("  Rule: \"hello&你好\" under None | Fanjian");
    println!(
        "    is_match(\"hello 妳好\") => {}",
        m.is_match("hello 妳好")
    );
    println!(
        "    is_match(\"hello 你好\") => {}",
        m.is_match("hello 你好")
    );
    println!(
        "    is_match(\"hello world\") => {}",
        m.is_match("hello world")
    );

    // Zero-allocation when no change occurs
    println!("\n  [Cow optimization]");
    let result = text_process(ProcessType::Fanjian, "pure ascii");
    println!(
        "    text_process(Fanjian, \"pure ascii\") is borrowed: {}",
        matches!(result, std::borrow::Cow::Borrowed(_))
    );
}
