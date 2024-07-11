#[cfg(feature = "runtime_build")]
pub mod runtime_build_feature {
    pub const FANJIAN: &str = include_str!("../../str_conv/FANJIAN.txt");
    pub const TEXT_DELETE: &str = include_str!("../../str_conv/TEXT-DELETE.txt");
    pub const NUM_NORM: &str = include_str!("../../str_conv/NUM-NORM.txt");
    pub const NORM: &str = include_str!("../../str_conv/NORM.txt");
    pub const PINYIN: &str = include_str!("../../str_conv/PINYIN.txt");
    pub const PINYIN_CHAR: &str = include_str!("../../str_conv/PINYIN-CHAR.txt");

    pub const WHITE_SPACE: &[&str] = &[
        "\u{0009}", "\u{000A}", "\u{000B}", "\u{000C}", "\u{000D}", "\u{0020}", "\u{0085}",
        "\u{00A0}", "\u{1680}", "\u{2000}", "\u{2001}", "\u{2002}", "\u{2003}", "\u{2004}",
        "\u{2005}", "\u{2006}", "\u{2007}", "\u{2008}", "\u{2009}", "\u{200A}", "\u{200D}",
        "\u{200F}", "\u{2028}", "\u{2029}", "\u{202F}", "\u{205F}", "\u{3000}",
    ];
}

#[cfg(feature = "prebuilt")]
pub mod prebuilt_feature {
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
        "/fanjian_daachorse_charwise_u32_matcher.bin"
    ));
    pub const PINYIN_PROCESS_REPLACE_LIST_STR: &str =
        include_str!(concat!(env!("OUT_DIR"), "/pinyin_process_replace_list.bin"));
    pub const PINYINCHAR_PROCESS_REPLACE_LIST_STR: &str = include_str!(concat!(
        env!("OUT_DIR"),
        "/pinyinchar_process_replace_list.bin"
    ));
    pub const PINYIN_PROCESS_MATCHER_BYTES: &[u8] = include_bytes!(concat!(
        env!("OUT_DIR"),
        "/pinyin_daachorse_charwise_u32_matcher.bin"
    ));

    pub const TEXT_DELETE: &str = include_str!("../../str_conv/TEXT-DELETE.txt");

    pub const WHITE_SPACE: &[&str] = &[
        "\u{0009}", "\u{000A}", "\u{000B}", "\u{000C}", "\u{000D}", "\u{0020}", "\u{0085}",
        "\u{00A0}", "\u{1680}", "\u{2000}", "\u{2001}", "\u{2002}", "\u{2003}", "\u{2004}",
        "\u{2005}", "\u{2006}", "\u{2007}", "\u{2008}", "\u{2009}", "\u{200A}", "\u{200D}",
        "\u{200F}", "\u{2028}", "\u{2029}", "\u{202F}", "\u{205F}", "\u{3000}",
    ];
}
