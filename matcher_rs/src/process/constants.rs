//! This module defines several constants for processing and normalization of text data,
//! including definitions for whitespace characters, conditional includes for files,
//! and configurations for runtime build and DFA (Deterministic Finite Automaton) features.
/// These constants are conditionally included when the `runtime_build` feature is enabled.
/// They provide paths to various text processing maps used for normalization and replacement.
///
/// - `FANJIAN`: Maps traditional Chinese characters to simplified Chinese characters.
/// - `TEXT_DELETE`: Defines text segments that should be removed during preprocessing.
/// - `NUM_NORM`: Specifies numeric normalization rules.
/// - `NORM`: Contains general normalization rules.
/// - `PINYIN`: Provides mappings for converting Chinese characters to Pinyin.
#[cfg(feature = "runtime_build")]
pub const FANJIAN: &str = include_str!("../../process_map/FANJIAN.txt");
#[cfg(feature = "runtime_build")]
pub const TEXT_DELETE: &str = include_str!("../../process_map/TEXT-DELETE.txt");
#[cfg(feature = "runtime_build")]
pub const NUM_NORM: &str = include_str!("../../process_map/NUM-NORM.txt");
#[cfg(feature = "runtime_build")]
pub const NORM: &str = include_str!("../../process_map/NORM.txt");
#[cfg(feature = "runtime_build")]
pub const PINYIN: &str = include_str!("../../process_map/PINYIN.txt");

/// List of Unicode code points considered as whitespace characters.
#[cfg(feature = "runtime_build")]
pub const WHITE_SPACE: &[&str; 27] = &[
    "\u{0009}", "\u{000A}", "\u{000B}", "\u{000C}", "\u{000D}", "\u{0020}", "\u{0085}", "\u{00A0}",
    "\u{1680}", "\u{2000}", "\u{2001}", "\u{2002}", "\u{2003}", "\u{2004}", "\u{2005}", "\u{2006}",
    "\u{2007}", "\u{2008}", "\u{2009}", "\u{200A}", "\u{200D}", "\u{200F}", "\u{2028}", "\u{2029}",
    "\u{202F}", "\u{205F}", "\u{3000}",
];

/// These constants are for normalization processing and are included based on different
/// feature flags.
#[cfg(all(not(feature = "runtime_build"), feature = "dfa"))]
pub const NORMALIZE_PROCESS_LIST_STR: &str =
    include_str!(concat!(env!("OUT_DIR"), "/normalize_process_list.bin"));
#[cfg(all(not(feature = "runtime_build"), not(feature = "dfa")))]
pub const NORMALIZE_PROCESS_MATCHER_BYTES: &[u8] = include_bytes!(concat!(
    env!("OUT_DIR"),
    "/normalize_daachorse_charwise_u32_matcher.bin"
));
#[cfg(not(feature = "runtime_build"))]
pub const NORMALIZE_PROCESS_REPLACE_LIST_STR: &str = include_str!(concat!(
    env!("OUT_DIR"),
    "/normalize_process_replace_list.bin"
));

#[cfg(not(feature = "runtime_build"))]
pub const FANJIAN_L1_BYTES: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/fanjian_l1.bin"));
#[cfg(not(feature = "runtime_build"))]
pub const FANJIAN_L2_BYTES: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/fanjian_l2.bin"));

#[cfg(not(feature = "runtime_build"))]
pub const PINYIN_L1_BYTES: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/pinyin_l1.bin"));
#[cfg(not(feature = "runtime_build"))]
pub const PINYIN_L2_BYTES: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/pinyin_l2.bin"));
#[cfg(not(feature = "runtime_build"))]
pub const PINYIN_STR_BYTES: &str = include_str!(concat!(env!("OUT_DIR"), "/pinyin_str.bin"));

#[cfg(not(feature = "runtime_build"))]
pub const DELETE_BITSET_BYTES: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/delete_bitset.bin"));
