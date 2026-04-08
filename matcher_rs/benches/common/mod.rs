// Shared helpers for all bench binaries. Not every binary uses every item.
#![allow(dead_code, unused_imports)]

use std::collections::HashMap;

use matcher_rs::{ProcessType, SimpleMatcher};

// ── Data ────────────────────────────────────────────────────────────────────────

pub const CN_WORD_LIST: &str = include_str!("../../../data/word/cn/jieba.txt");
pub const CN_HAYSTACK: &str = include_str!("../../../data/text/cn/三体.txt");

pub const EN_WORD_LIST: &str = include_str!("../../../data/word/en/dictionary.txt");
pub const EN_HAYSTACK: &str = include_str!("../../../data/text/en/sherlock.txt");

// ── Parameters
// ──────────────────────────────────────────────────────────────────

pub const RULE_COUNTS: &[usize] = &[1_000, 10_000, 50_000, 100_000, 500_000];
pub const DEFAULT_RULE_COUNT: usize = 10_000;

pub const BUILD_PROCESS_TYPES: &[ProcessType] = &[
    ProcessType::None,
    ProcessType::Delete,
    ProcessType::VariantNorm,
    ProcessType::VariantNormDeleteNormalize,
    ProcessType::Romanize,
];

// ── Helpers
// ─────────────────────────────────────────────────────────────────────

/// Returns a filtered, sorted word list.
///
/// - `"en"`:    pure ASCII words from the English dictionary
/// - `"cn"`:    pure non-ASCII words from the Chinese dictionary
/// - `"mixed"`: alternating ASCII and CJK words (guaranteed ~50/50 mix)
pub fn word_list(lang: &str) -> Vec<&str> {
    match lang {
        "cn" => {
            let mut w: Vec<&str> = CN_WORD_LIST
                .lines()
                .filter(|s| !s.is_ascii() && !s.is_empty())
                .collect();
            w.sort_unstable();
            w
        }
        "mixed" => {
            let mut en: Vec<&str> = EN_WORD_LIST
                .lines()
                .filter(|s| s.is_ascii() && !s.is_empty())
                .collect();
            let mut cn: Vec<&str> = CN_WORD_LIST
                .lines()
                .filter(|s| !s.is_ascii() && !s.is_empty())
                .collect();
            en.sort_unstable();
            cn.sort_unstable();
            let cap = en.len() + cn.len();
            let mut words = Vec::with_capacity(cap);
            let (mut ei, mut ci) = (0, 0);
            while ei < en.len() || ci < cn.len() {
                if ei < en.len() {
                    words.push(en[ei]);
                    ei += 1;
                }
                if ci < cn.len() {
                    words.push(cn[ci]);
                    ci += 1;
                }
            }
            words
        }
        _ => {
            let mut w: Vec<&str> = EN_WORD_LIST
                .lines()
                .filter(|s| s.is_ascii() && !s.is_empty())
                .collect();
            w.sort_unstable();
            w
        }
    }
}

pub fn build_literal_map(lang: &str, size: usize, match_scenario: bool) -> HashMap<u32, String> {
    let patterns = word_list(lang);
    let mut map = HashMap::with_capacity(size);
    for i in 0..size {
        let word_idx = (i * 997) % patterns.len();
        let word = if match_scenario {
            patterns[word_idx].to_string()
        } else if lang == "cn" {
            format!("{}\u{E000}{i}", patterns[word_idx])
        } else {
            format!("__impossible_{word_idx}_match_{i}__")
        };
        map.insert((i + 1) as u32, word);
    }
    map
}

pub fn build_shaped_map(lang: &str, size: usize, shape: &str) -> HashMap<u32, String> {
    let patterns = word_list(lang);
    let mut map = HashMap::with_capacity(size);
    for i in 0..size {
        let idx = (i * 997) % patterns.len();
        let shaped = match shape {
            "literal" => patterns[idx].to_string(),
            "and" => {
                let a = patterns[idx];
                let b = patterns[(idx + 101) % patterns.len()];
                let c = patterns[(idx + 211) % patterns.len()];
                format!("{a}&{b}&{c}")
            }
            "not" => format!("{}~__never_block_{i}__", patterns[idx]),
            "or" => {
                let a = patterns[idx];
                let b = patterns[(idx + 101) % patterns.len()];
                let c = patterns[(idx + 211) % patterns.len()];
                format!("{a}|{b}|{c}")
            }
            "word_boundary" => format!("\\b{}\\b", patterns[idx]),
            "deep_and" => {
                let parts: Vec<&str> = (0..5)
                    .map(|j| patterns[(idx + j * 101) % patterns.len()])
                    .collect();
                parts.join("&")
            }
            "deep_not" => {
                let a = patterns[idx];
                let b = patterns[(idx + 101) % patterns.len()];
                let c = patterns[(idx + 211) % patterns.len()];
                let d = patterns[(idx + 317) % patterns.len()];
                format!("{a}~{b}~{c}~{d}")
            }
            "mixed_ops" => {
                let a = patterns[idx];
                let b = patterns[(idx + 101) % patterns.len()];
                let c = patterns[(idx + 211) % patterns.len()];
                format!("{a}&{b}~{c}")
            }
            _ => unreachable!("unknown rule shape: {shape}"),
        };
        map.insert((i + 1) as u32, shaped);
    }
    map
}

pub fn build_mixed_script_map(size: usize) -> HashMap<u32, String> {
    let en = word_list("en");
    let cn = word_list("cn");
    let mut map = HashMap::with_capacity(size);
    for i in 0..size {
        let word = if i % 2 == 0 {
            en[(i * 997) % en.len()]
        } else {
            cn[(i * 991) % cn.len()]
        };
        map.insert((i + 1) as u32, word.to_string());
    }
    map
}

pub fn build_multi_process_table(size: usize) -> HashMap<ProcessType, HashMap<u32, String>> {
    let slice = (size / 4).max(1);
    HashMap::from([
        (ProcessType::None, build_literal_map("en", slice, true)),
        (ProcessType::Delete, build_literal_map("en", slice, true)),
        (
            ProcessType::VariantNorm,
            build_literal_map("cn", slice, true),
        ),
        (
            ProcessType::VariantNormDeleteNormalize,
            build_literal_map("cn", size - slice * 3, true),
        ),
    ])
}

pub fn wrap_table(
    pt: ProcessType,
    map: HashMap<u32, String>,
) -> HashMap<ProcessType, HashMap<u32, String>> {
    HashMap::from([(pt, map)])
}
