/// This module defines several constants for processing and normalization of text data,
/// including definitions for whitespace characters, conditional includes for files,
/// and configurations for runtime build and DFA (Deterministic Finite Automaton) features.

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

/// These constants are for normalization processing and are included based on different
/// feature flags.
///
/// When the `runtime_build` feature is not enabled and the `dfa` feature is enabled,
/// `NORMALIZE_PROCESS_LIST_STR` is included. This constant provides the path to the
/// normalization process list, which is generated at compile time.
///
/// When `runtime_build` is not enabled and the `dfa` feature is not enabled,
/// `NORMALIZE_PROCESS_MATCHER_BYTES` is included. This constant provides the path to
/// the normalization matcher bytes, which is also generated during the build process.
///
/// Additionally, `NORMALIZE_PROCESS_REPLACE_LIST_STR` is included when `runtime_build`
/// is not enabled. This constant provides the path to the normalization replace list,
/// used for text replacement operations during normalization.
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

/// These constants are related to Fanjian (simplified vs traditional Chinese conversion)
/// processing and are included based on feature flags.
///
/// - When the `runtime_build` feature is not enabled, `FANJIAN_PROCESS_REPLACE_LIST_STR`
///   is included. This constant provides the path to the Fanjian process replace list,
///   which is used for converting traditional Chinese characters to simplified Chinese
///   characters during normalization.
///
/// - Additionally, when the `runtime_build` feature is not enabled, `FANJIAN_PROCESS_MATCHER_BYTES`
///   is included. This constant provides the path to the Fanjian matcher bytes, which are
///   used for matching Fanjian text patterns during the normalization process.
#[cfg(not(feature = "runtime_build"))]
pub const FANJIAN_PROCESS_REPLACE_LIST_STR: &str = include_str!(concat!(
    env!("OUT_DIR"),
    "/fanjian_process_replace_list.bin"
));
#[cfg(not(feature = "runtime_build"))]
pub const FANJIAN_PROCESS_MATCHER_BYTES: &[u8] = include_bytes!(concat!(
    env!("OUT_DIR"),
    "/fanjian_daachorse_charwise_u32_matcher.bin"
));

/// These constants are related to Pinyin processing and are included based on feature flags.
///
/// - When the `runtime_build` feature is not enabled, `PINYIN_PROCESS_REPLACE_LIST_STR`
///   is included. This constant provides the path to the Pinyin process replace list,
///   which is used for converting Chinese characters to Pinyin during normalization.
///
/// - Similarly, when the `runtime_build` feature is not enabled, `PINYINCHAR_PROCESS_REPLACE_LIST_STR`
///   is included. This constant provides the path to the Pinyin character process replace list,
///   which is also used for text replacement operations.
///
/// - Additionally, when the `runtime_build` feature is not enabled, `PINYIN_PROCESS_MATCHER_BYTES`
///   is included. This constant provides the path to the Pinyin matcher bytes, which are
///   used for matching Pinyin text patterns during the normalization process.
#[cfg(not(feature = "runtime_build"))]
pub const PINYIN_PROCESS_REPLACE_LIST_STR: &str =
    include_str!(concat!(env!("OUT_DIR"), "/pinyin_process_replace_list.bin"));
#[cfg(not(feature = "runtime_build"))]
pub const PINYINCHAR_PROCESS_REPLACE_LIST_STR: &str = include_str!(concat!(
    env!("OUT_DIR"),
    "/pinyinchar_process_replace_list.bin"
));
#[cfg(not(feature = "runtime_build"))]
pub const PINYIN_PROCESS_MATCHER_BYTES: &[u8] = include_bytes!(concat!(
    env!("OUT_DIR"),
    "/pinyin_daachorse_charwise_u32_matcher.bin"
));

/// List of Unicode code points considered as whitespace characters.
#[cfg(any(feature = "runtime_build", feature = "dfa"))]
pub const WHITE_SPACE: &[&str] = &[
    "\u{0009}", "\u{000A}", "\u{000B}", "\u{000C}", "\u{000D}", "\u{0020}", "\u{0085}", "\u{00A0}",
    "\u{1680}", "\u{2000}", "\u{2001}", "\u{2002}", "\u{2003}", "\u{2004}", "\u{2005}", "\u{2006}",
    "\u{2007}", "\u{2008}", "\u{2009}", "\u{200A}", "\u{200D}", "\u{200F}", "\u{2028}", "\u{2029}",
    "\u{202F}", "\u{205F}", "\u{3000}",
];

/// These constants are related to the text deletion processing and are included based on feature flags.
///
/// - When the `runtime_build` feature is not enabled and the `dfa` feature is enabled,
///   `TEXT_DELETE` is included. This constant provides the path to the text deletion map,
///   used for identifying text segments to be deleted during normalization.
///
/// - When the `runtime_build` feature is not enabled and the `dfa` feature is not enabled,
///   `TEXT_DELETE_PROCESS_MATCHER_BYTES` is included. This constant provides the path
///   to the text deletion matcher bytes, which are generated during the build process and
///   used for matching text patterns to be deleted during normalization.
#[cfg(all(not(feature = "runtime_build"), feature = "dfa"))]
pub const TEXT_DELETE: &str = include_str!("../../process_map/TEXT-DELETE.txt");
#[cfg(all(not(feature = "runtime_build"), not(feature = "dfa")))]
pub const TEXT_DELETE_PROCESS_MATCHER_BYTES: &[u8] = include_bytes!(concat!(
    env!("OUT_DIR"),
    "/delete_daachorse_charwise_u32_matcher.bin"
));
