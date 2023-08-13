use std::borrow::Cow;
use std::intrinsics::{likely, unlikely};

use ahash::{AHashMap, AHashSet};
use aho_corasick::{AhoCorasick, AhoCorasickBuilder, AhoCorasickKind::DFA, MatchKind};
use bitflags::bitflags;
use nohash_hasher::{IntMap, IntSet};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use tinyvec::{ArrayVec, TinyVec};

use super::TextMatcherTrait;

const FANJIAN: &str = include_str!("../str_conv_dat/RASEMAT-FANJIAN.txt"); // 繁简
const CN_SPECIAL: &str = include_str!("../str_conv_dat/RASEMAT-CN-SPECIAL.txt"); // 中文特殊字符
const EN_SPECIAL: &str = include_str!("../str_conv_dat/RASEMAT-EN-SPECIAL.txt"); // 英文特殊字符
const PUNCTUATION_SPECIAL: &str = include_str!("../str_conv_dat/RASEMAT-PUNCTUATION-SPECIAL.txt"); // 特殊符号
const EN_VARIATION: &str = include_str!("../str_conv_dat/RASEMAT-EN-VARIATION.txt"); // 英文变体
const UNICODE: &str = include_str!("../str_conv_dat/RASEMAT-UNICODE.txt"); // UNICODE变体
const NUM_NORM: &str = include_str!("../str_conv_dat/RASEMAT-NUM-NORM.txt"); // 数字变体
const UPPER_LOWER: &str = include_str!("../str_conv_dat/RASEMAT-UPPER-LOWER.txt"); // 大小写
const PINYIN: &str = include_str!("../str_conv_dat/RASEMAT-PINYIN.txt"); // 中文拼音
const PINYIN_CHAR: &str = include_str!("../str_conv_dat/RASEMAT-PINYIN-CHAR.txt"); // 中文拼音

const WHITE_SPACE: &[&str] = &[
    // 不可见字符
    "\u{0009}", "\u{000A}", "\u{000B}", "\u{000C}", "\u{000D}", "\u{0020}", "\u{0085}", "\u{00A0}",
    "\u{1680}", "\u{2000}", "\u{2001}", "\u{2002}", "\u{2003}", "\u{2004}", "\u{2005}", "\u{2006}",
    "\u{2007}", "\u{2008}", "\u{2009}", "\u{200A}", "\u{2028}", "\u{2029}", "\u{202F}", "\u{205F}",
    "\u{3000}",
];

#[derive(Serialize, Deserialize)]
pub struct SimpleWord<'a> {
    pub word_id: u64,  // 词ID
    pub word: &'a str, // 敏感词
}

bitflags! {
    #[derive(Hash, PartialEq, Eq, Clone, Copy, Debug)]
    pub struct StrConvType: u8 {
        const None = 0b00000000;       // 无
        const Fanjian = 0b00000001;    // 繁简
        const WordDelete = 0b00000010; // 词 删除归一
        const TextDelete = 0b00000100; // 文本 删除归一
        const Delete = 0b00000110;     // 删除归一
        const Normalize = 0b00001000;  // 替换归一
        const DeleteNormalize = 0b00001110; // 替换删除归一
        const FanjianDeleteNormalize = 0b00001111; // 繁简替换删除归一
        const PinYin = 0b00010000;     // 拼音转换
        const PinYinChar = 0b00100000; // 拼音字符转换
    }
}

pub type SimpleMatchType = StrConvType;

impl Serialize for StrConvType {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.bits().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for StrConvType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let bits: u8 = u8::deserialize(deserializer)?;
        Ok(StrConvType::from_bits_retain(bits))
    }
}

pub type SimpleWordlistDict<'a> = AHashMap<SimpleMatchType, Vec<SimpleWord<'a>>>;

struct WordConf {
    word: String,                  // 词
    split_bit: TinyVec<[u64; 64]>, // 词的命中bit列表，eg. "你好" -> [1]，“你好,你真棒” -> [1, 1]，“无,法,无,天” -> [2, 1, 1]，这里 "无" 出现了2次，对应bit为 1 << (2 - 1) = 2
}

struct SimpleAcTable {
    ac_matcher: AhoCorasick,              // ac自动机
    ac_word_conf_list: Vec<(u64, usize)>, // ac词ID对 词ID 以及 偏移量（上述split_bit的索引）的映射
}

#[derive(Debug, Serialize)]
pub struct SimpleResult<'a> {
    pub word_id: u64,       // 命中词ID
    pub word: Cow<'a, str>, // 命中词
}

pub struct SimpleMatcher {
    str_conv_process_dict: AHashMap<StrConvType, (Vec<&'static str>, AhoCorasick)>, // 转换方式对替换词表，替换词ac自动机的映射
    simple_ac_table_dict: AHashMap<SimpleMatchType, SimpleAcTable>,                 // simple ac词表
    simple_word_map: IntMap<u64, WordConf>, // 词ID对 词以及词命中bit列表的映射
    min_text_len: usize, // 要求的文本最小长度，小于该长度直接返回空命中列表，在最小词长度相对较长时，可高效过滤短文本
}

impl SimpleMatcher {
    pub fn new(simple_wordlist_dict: &SimpleWordlistDict) -> SimpleMatcher {
        let mut simple_matcher = SimpleMatcher {
            str_conv_process_dict: AHashMap::new(),
            simple_ac_table_dict: AHashMap::new(),
            simple_word_map: IntMap::default(),
            min_text_len: 255,
        };

        for (simple_match_type, simple_wordlist) in simple_wordlist_dict {
            for str_conv_type in simple_match_type.iter() {
                simple_matcher
                    .str_conv_process_dict
                    .entry(str_conv_type)
                    .or_insert_with(|| Self::_get_process_matcher(str_conv_type));
            }

            let word_str_conv_list = *simple_match_type - StrConvType::TextDelete;

            let simple_ac_table =
                simple_matcher.build_simple_ac_table(&word_str_conv_list, simple_wordlist);

            simple_matcher.simple_ac_table_dict.insert(
                *simple_match_type - StrConvType::WordDelete,
                simple_ac_table,
            );
        }

        simple_matcher
    }

    fn _get_process_matcher(str_conv_type: StrConvType) -> (Vec<&'static str>, AhoCorasick) {
        let mut process_dict = AHashMap::new();

        match str_conv_type {
            StrConvType::Fanjian => {
                for str_conv_dat in [FANJIAN, UNICODE] {
                    process_dict.extend(str_conv_dat.trim().split('\n').map(|pair_str| {
                        let mut pair_str_split = pair_str.split('\t');
                        (
                            pair_str_split.next().unwrap(),
                            pair_str_split.next().unwrap(),
                        )
                    }));
                }
            }
            StrConvType::WordDelete => {
                process_dict.extend(
                    PUNCTUATION_SPECIAL
                        .trim()
                        .split('\n')
                        .map(|pair_str| (pair_str, "")),
                );

                process_dict.extend(WHITE_SPACE.iter().map(|&c| (c, "")));
            }
            StrConvType::TextDelete => {
                for str_conv_dat in [PUNCTUATION_SPECIAL, CN_SPECIAL, EN_SPECIAL] {
                    process_dict.extend(
                        str_conv_dat
                            .trim()
                            .split('\n')
                            .map(|pair_str| (pair_str, "")),
                    );
                }

                process_dict.extend(WHITE_SPACE.iter().map(|&c| (c, "")));
            }
            StrConvType::Normalize => {
                for str_conv_dat in [UPPER_LOWER, EN_VARIATION, NUM_NORM] {
                    process_dict.extend(str_conv_dat.trim().split('\n').map(|pair_str| {
                        let mut pair_str_split = pair_str.split('\t');
                        (
                            pair_str_split.next().unwrap(),
                            pair_str_split.next().unwrap(),
                        )
                    }));
                }
            }
            StrConvType::PinYin => {
                process_dict.extend(PINYIN.trim().split('\n').map(|pair_str| {
                    let mut pair_str_split = pair_str.split('\t');
                    (
                        pair_str_split.next().unwrap(),
                        pair_str_split.next().unwrap(),
                    )
                }));
            }
            StrConvType::PinYinChar => {
                process_dict.extend(PINYIN_CHAR.trim().split('\n').map(|pair_str| {
                    let mut pair_str_split = pair_str.split('\t');
                    (
                        pair_str_split.next().unwrap(),
                        pair_str_split.next().unwrap(),
                    )
                }));
            }
            _ => {}
        }

        process_dict
            .retain(|&key, &mut value| (key == "#" || !key.starts_with('#')) && key != value); // 剔除注释词以及无效映射关系

        let process_matcher = AhoCorasickBuilder::new()
            .kind(Some(DFA)) // dfa更快但更占内存
            .match_kind(MatchKind::LeftmostLongest) // 转换词之间可能会有重叠，eg."A","Ą̴̡̣̠̮̓̋", 此时以bytes最长的为准
            .build(
                process_dict
                    .iter()
                    .map(|(&key, _)| key)
                    .collect::<Vec<&str>>(),
            )
            .unwrap();
        let process_replace_list = process_dict.iter().map(|(_, &val)| val).collect();

        (process_replace_list, process_matcher)
    }

    fn build_simple_ac_table(
        &mut self,
        str_conv_type_list: &StrConvType,
        simple_wordlist: &Vec<SimpleWord>,
    ) -> SimpleAcTable {
        let mut ac_wordlist = Vec::with_capacity(simple_wordlist.len());
        let mut ac_word_conf_list = Vec::with_capacity(simple_wordlist.len());

        for simple_word in simple_wordlist {
            let char_unique_cnt = simple_word
                .word
                .chars()
                .filter(|&c| c != ',')
                .collect::<AHashSet<char>>()
                .len();

            if self.min_text_len > char_unique_cnt {
                self.min_text_len = char_unique_cnt; // 计算最小长度文本
            }

            let mut ac_split_word_counter: AHashMap<&str, u8> = AHashMap::new(); // 计算重复词的个数
            for ac_split_word in simple_word.word.split(',').filter(|&x| !x.is_empty()) {
                ac_split_word_counter
                    .entry(ac_split_word)
                    .and_modify(|cnt| *cnt += 1)
                    .or_insert(1);
            }

            let split_bit = ac_split_word_counter
                .values()
                .map(|&x| if x < 64 { 1 << (x - 1) } else { 1 << 63 }) // 最多重复64次
                .collect();

            self.simple_word_map.insert(
                simple_word.word_id,
                WordConf {
                    word: simple_word.word.to_owned(),
                    split_bit,
                },
            );

            for (offset, split_word) in ac_split_word_counter.keys().enumerate() {
                for ac_word in self.reduce_text_process(str_conv_type_list, split_word.as_bytes()) {
                    ac_wordlist.push(ac_word.into_owned());
                    ac_word_conf_list.push((simple_word.word_id, offset));
                }
            }
        }

        SimpleAcTable {
            ac_matcher: AhoCorasickBuilder::new()
                .kind(Some(DFA))
                .ascii_case_insensitive(true) // 大小写不敏感
                .build(&ac_wordlist)
                .unwrap(),
            ac_word_conf_list,
        }
    }

    #[inline]
    fn reduce_text_process<'a>(
        &self,
        str_conv_type_list: &StrConvType,
        text_bytes: &'a [u8],
    ) -> ArrayVec<[Cow<'a, [u8]>; 4]> {
        // 链式转换文本，先验信息确定了最大为4组
        let mut processed_text_bytes_list: ArrayVec<[Cow<'a, [u8]>; 4]> = ArrayVec::new();
        processed_text_bytes_list.push(Cow::Borrowed(text_bytes));

        for str_conv_type in str_conv_type_list.iter() {
            let (process_replace_list, process_matcher) = unsafe {
                self.str_conv_process_dict
                    .get(&str_conv_type)
                    .unwrap_unchecked()
            };
            let tmp_processed_text_bytes =
                unsafe { processed_text_bytes_list.last_mut().unwrap_unchecked() };

            if likely(process_matcher.is_match(tmp_processed_text_bytes.as_ref())) {
                // 按先验信息，删除归一 与 替换归一 是大概率命中的
                match str_conv_type {
                    StrConvType::Fanjian => {
                        // 由于词和文本都做了相同的繁简变换，那么原文本是没必要的，直接匹配繁简转换后的文本即可
                        *tmp_processed_text_bytes = Cow::Owned(
                            process_matcher.replace_all_bytes(text_bytes, process_replace_list),
                        );
                    }
                    StrConvType::TextDelete | StrConvType::WordDelete => {
                        // 省去n次 string.push('')的操作
                        let mut processed_text = Vec::with_capacity(tmp_processed_text_bytes.len());
                        let mut last_match = 0;

                        for mat in process_matcher.find_iter(tmp_processed_text_bytes.as_ref()) {
                            processed_text.extend(unsafe {
                                tmp_processed_text_bytes.get_unchecked(last_match..mat.start())
                            });
                            last_match = mat.end();
                        }
                        processed_text.extend(unsafe {
                            tmp_processed_text_bytes.get_unchecked(last_match..)
                        });

                        processed_text_bytes_list.push(Cow::Owned(processed_text));
                    }
                    _ => {
                        let processed_text = process_matcher
                            .replace_all_bytes(tmp_processed_text_bytes, process_replace_list);
                        processed_text_bytes_list.push(Cow::Owned(processed_text));
                    }
                }
            }
        }

        processed_text_bytes_list
    }
}

impl<'a> TextMatcherTrait<'a, SimpleResult<'a>> for SimpleMatcher {
    fn is_match(&self, text: &str) -> bool {
        // 后续再优化
        !self.process(text).is_empty()
    }

    fn process(&'a self, text: &str) -> Vec<SimpleResult<'a>> {
        let text_bytes = text.as_bytes();
        let mut result_list = Vec::new();

        if unlikely(bytecount::num_chars(text_bytes) < self.min_text_len) {
            // 过滤短文本
            return result_list;
        }

        let mut word_id_set = IntSet::default();

        // 词ID对其命中轮次以及命中bit的映射，eg.“无,法,无,天” 繁简+删除归一+替换归一 3轮匹配，1 -> [[2，2，2], [1, 1, 1], [1, 1, 1]]
        // 当且仅当 所有内部数组都至少有一个0时 代表命中
        let mut word_id_split_bit_map = IntMap::default();

        for (simple_match_type, simple_ac_table) in &self.simple_ac_table_dict {
            let processed_text_bytes_list = self.reduce_text_process(simple_match_type, text_bytes);
            for (index, processed_text) in processed_text_bytes_list.iter().enumerate() {
                for ac_result in simple_ac_table
                    .ac_matcher
                    .find_overlapping_iter(processed_text)
                // ac词会重复，需要遍历所有的ac命中词
                {
                    let ac_word_id = ac_result.pattern().as_usize();
                    let ac_word_conf =
                        unsafe { simple_ac_table.ac_word_conf_list.get_unchecked(ac_word_id) };
                    let word_id = ac_word_conf.0;
                    let word_conf =
                        unsafe { self.simple_word_map.get(&word_id).unwrap_unchecked() };

                    let split_bit = word_id_split_bit_map.entry(word_id).or_insert_with(|| {
                        word_conf
                            .split_bit
                            .iter()
                            .map(|&x| {
                                processed_text_bytes_list
                                    .iter()
                                    .map(|_| x)
                                    .collect::<ArrayVec<[u64; 4]>>()
                            })
                            .collect::<TinyVec<[_; 64]>>()
                    });

                    *unsafe {
                        split_bit
                            .get_unchecked_mut(ac_word_conf.1)
                            .get_unchecked_mut(index)
                    } >>= 1; // 右移一位，不用 -1 是因为不能确定命中次数，u64 - 1 最后可能会越界

                    if unlikely(
                        split_bit.iter().all(|bit| bit.iter().any(|&b| b == 0))
                            && !word_id_set.contains(&word_id),
                    ) {
                        word_id_set.insert(word_id);
                        result_list.push(SimpleResult {
                            word_id,
                            word: Cow::Borrowed(&word_conf.word),
                        });
                    }
                }
            }
        }

        result_list
    }
}
