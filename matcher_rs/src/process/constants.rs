#[cfg(feature = "runtime_build")]
pub mod runtime_build_feature {
    /// A collection of constant string slices that include various string conversion mappings.
    ///
    /// Each constant below is loaded from a corresponding text file using the [include_str!] macro.
    /// These files contain mappings used for different conversion and normalization processes,
    /// such as simplifying characters, handling punctuation, and converting between upper and lower case.
    ///
    /// These mappings are utilized in text processing to apply transformations based on different
    /// [SimpleMatchType] values. They facilitate efficient text matching and replacement operations
    /// by providing a predefined set of conversion rules.
    ///
    /// # Constants
    ///
    /// * [FANJIAN] - Simplifies traditional Chinese characters to simplified ones.
    /// * [CN_SPECIAL] - Contains special Chinese characters.
    /// * [EN_SPECIAL] - Contains special English characters.
    /// * [PUNCTUATION_SPECIAL] - Contains special punctuation characters.
    /// * [EN_VARIATION] - Contains variations of English characters.
    /// * [UNICODE] - Contains unicode specific mappings.
    /// * [NUM_NORM] - Normalizes numeric characters.
    /// * [UPPER_LOWER] - Maps between upper and lower case characters.
    /// * [PINYIN] - Converts Chinese characters to Pinyin.
    /// * [PINYIN_CHAR] - Converts individual Chinese characters to Pinyin.
    pub const FANJIAN: &str = include_str!("../../str_conv_map/FANJIAN.txt");
    pub const CN_SPECIAL: &str = include_str!("../../str_conv_map/CN-SPECIAL.txt");
    pub const EN_SPECIAL: &str = include_str!("../../str_conv_map/EN-SPECIAL.txt");
    pub const PUNCTUATION_SPECIAL: &str =
        include_str!("../../str_conv_map/PUNCTUATION-SPECIAL.txt");
    pub const EN_VARIATION: &str = include_str!("../../str_conv_map/EN-VARIATION.txt");
    pub const UNICODE: &str = include_str!("../../str_conv_map/UNICODE.txt");
    pub const NUM_NORM: &str = include_str!("../../str_conv_map/NUM-NORM.txt");
    pub const UPPER_LOWER: &str = include_str!("../../str_conv_map/UPPER-LOWER.txt");
    pub const PINYIN: &str = include_str!("../../str_conv_map/PINYIN.txt");
    pub const PINYIN_CHAR: &str = include_str!("../../str_conv_map/PINYIN-CHAR.txt");

    /// A constant slice containing string references to various Unicode whitespace characters.
    ///
    /// These characters include:
    ///
    /// - Horizontal tab (`\u{0009}`).
    /// - Line feed (`\u{000A}`).
    /// - Vertical tab (`\u{000B}`).
    /// - Form feed (`\u{000C}`).
    /// - Carriage return (`\u{000D}`).
    /// - Space (`\u{0020}`).
    /// - Next line (`\u{0085}`).
    /// - No-break space (`\u{00A0}`).
    /// - Ogham space mark (`\u{1680}`).
    /// - En quad (`\u{2000}`).
    /// - Em quad (`\u{2001}`).
    /// - En space (`\u{2002}`).
    /// - Em space (`\u{2003}`).
    /// - Three-per-em space (`\u{2004}`).
    /// - Four-per-em space (`\u{2005}`).
    /// - Six-per-em space (`\u{2006}`).
    /// - Figure space (`\u{2007}`).
    /// - Punctuation space (`\u{2008}`).
    /// - Thin space (`\u{2009}`).
    /// - Hair space (`\u{200A}`).
    /// - Line separator (`\u{2028}`).
    /// - Paragraph separator (`\u{2029}`).
    /// - Narrow no-break space (`\u{202F}`).
    /// - Medium mathematical space (`\u{205F}`).
    /// - Ideographic space (`\u{3000}`).
    pub const WHITE_SPACE: &[&str] = &[
        "\u{0009}", "\u{000A}", "\u{000B}", "\u{000C}", "\u{000D}", "\u{0020}", "\u{0085}",
        "\u{00A0}", "\u{1680}", "\u{2000}", "\u{2001}", "\u{2002}", "\u{2003}", "\u{2004}",
        "\u{2005}", "\u{2006}", "\u{2007}", "\u{2008}", "\u{2009}", "\u{200A}", "\u{2028}",
        "\u{2029}", "\u{202F}", "\u{205F}", "\u{3000}",
    ];
}

#[cfg(feature = "prebuilt")]
pub mod prebuilt_feature {
    /// This module contains constants that reference various prebuilt string conversion maps.
    ///
    /// These constants are typically used for normalizing text data, converting between different
    /// character sets, and handling special cases in text processing. The data is included from
    /// prebuilt binary files and text files located in specific directories.
    ///
    /// # Constants
    ///
    /// * [NORMALIZE_PROCESS_LIST_STR] - A string containing normalized process list rules.
    /// * [NORMALIZE_PROCESS_REPLACE_LIST_STR] - A string containing normalized process replace rules.
    /// * [FANJIAN_PROCESS_REPLACE_LIST_STR] - A string containing rules for replacing traditional Chinese characters with simplified ones.
    /// * [FANJIAN_PROCESS_MATCHER_BYTES] - A byte slice representing a prebuilt matcher for `SimpleMatchType::Fanjian`.
    /// * [PINYIN_PROCESS_REPLACE_LIST_STR] - A string containing rules for converting Chinese characters to Pinyin.
    /// * [PINYIN_PROCESS_MATCHER_BYTES] - A byte slice representing a prebuilt matcher for `SimpleMatchType::PinYin`.
    /// * [PINYINCHAR_PROCESS_REPLACE_LIST_STR] - A string containing rules for converting individual Chinese characters to Pinyin.
    /// * [PINYINCHAR_PROCESS_MATCHER_BYTES] - A byte slice representing a prebuilt matcher for `SimpleMatchType::PinYinChar`.
    /// * [CN_SPECIAL] - A string containing special Chinese characters.
    /// * [EN_SPECIAL] - A string containing special English characters.
    /// * [PUNCTUATION_SPECIAL] - A string containing special punctuation characters.
    pub const NORMALIZE_PROCESS_LIST_STR: &str =
        include_str!(concat!(env!("OUT_DIR"), "/normalize_process_list.bin"));
    pub const NORMALIZE_PROCESS_REPLACE_LIST_STR: &str = include_str!(concat!(
        env!("OUT_DIR"),
        "/normalize_process_replace_list.bin"
    ));

    pub const FANJIAN_PROCESS_REPLACE_LIST_STR: &str = include_str!(concat!(
        env!("OUT_DIR"),
        "/fanjian_process_replace_list.bin"
    ));
    pub const FANJIAN_PROCESS_MATCHER_BYTES: &[u8] = include_bytes!(concat!(
        env!("OUT_DIR"),
        "/fanjian_daachorse_charwise_u64_matcher.bin"
    ));
    pub const PINYIN_PROCESS_REPLACE_LIST_STR: &str =
        include_str!(concat!(env!("OUT_DIR"), "/pinyin_process_replace_list.bin"));
    pub const PINYIN_PROCESS_MATCHER_BYTES: &[u8] = include_bytes!(concat!(
        env!("OUT_DIR"),
        "/pinyin_daachorse_charwise_u64_matcher.bin"
    ));
    pub const PINYINCHAR_PROCESS_REPLACE_LIST_STR: &str = include_str!(concat!(
        env!("OUT_DIR"),
        "/pinyinchar_process_replace_list.bin"
    ));
    pub const PINYINCHAR_PROCESS_MATCHER_BYTES: &[u8] = include_bytes!(concat!(
        env!("OUT_DIR"),
        "/pinyinchar_daachorse_charwise_u64_matcher.bin"
    ));

    pub const CN_SPECIAL: &str = include_str!("../../str_conv_map/CN-SPECIAL.txt");
    pub const EN_SPECIAL: &str = include_str!("../../str_conv_map/EN-SPECIAL.txt");
    pub const PUNCTUATION_SPECIAL: &str =
        include_str!("../../str_conv_map/PUNCTUATION-SPECIAL.txt");

    /// A constant slice containing string references to various Unicode whitespace characters.
    ///
    /// These characters include:
    ///
    /// - Horizontal tab (`\u{0009}`).
    /// - Line feed (`\u{000A}`).
    /// - Vertical tab (`\u{000B}`).
    /// - Form feed (`\u{000C}`).
    /// - Carriage return (`\u{000D}`).
    /// - Space (`\u{0020}`).
    /// - Next line (`\u{0085}`).
    /// - No-break space (`\u{00A0}`).
    /// - Ogham space mark (`\u{1680}`).
    /// - En quad (`\u{2000}`).
    /// - Em quad (`\u{2001}`).
    /// - En space (`\u{2002}`).
    /// - Em space (`\u{2003}`).
    /// - Three-per-em space (`\u{2004}`).
    /// - Four-per-em space (`\u{2005}`).
    /// - Six-per-em space (`\u{2006}`).
    /// - Figure space (`\u{2007}`).
    /// - Punctuation space (`\u{2008}`).
    /// - Thin space (`\u{2009}`).
    /// - Hair space (`\u{200A}`).
    /// - Line separator (`\u{2028}`).
    /// - Paragraph separator (`\u{2029}`).
    /// - Narrow no-break space (`\u{202F}`).
    /// - Medium mathematical space (`\u{205F}`).
    /// - Ideographic space (`\u{3000}`).
    pub const WHITE_SPACE: &[&str] = &[
        "\u{0009}", "\u{000A}", "\u{000B}", "\u{000C}", "\u{000D}", "\u{0020}", "\u{0085}",
        "\u{00A0}", "\u{1680}", "\u{2000}", "\u{2001}", "\u{2002}", "\u{2003}", "\u{2004}",
        "\u{2005}", "\u{2006}", "\u{2007}", "\u{2008}", "\u{2009}", "\u{200A}", "\u{2028}",
        "\u{2029}", "\u{202F}", "\u{205F}", "\u{3000}",
    ];
}
