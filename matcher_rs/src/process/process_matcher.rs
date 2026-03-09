use std::borrow::Cow;
use std::cell::RefCell;
#[cfg(feature = "runtime_build")]
use std::collections::HashMap;
use std::collections::HashSet;
use std::fmt::Display;
use std::sync::OnceLock;

use aho_corasick::AhoCorasick;
#[cfg(feature = "dfa")]
use aho_corasick::{AhoCorasickBuilder, AhoCorasickKind, MatchKind as AhoCorasickMatchKind};
use bitflags::bitflags;
#[cfg(not(feature = "dfa"))]
use daachorse::CharwiseDoubleArrayAhoCorasick;
#[cfg(all(feature = "runtime_build", not(feature = "dfa")))]
use daachorse::{
    CharwiseDoubleArrayAhoCorasickBuilder, MatchKind as DoubleArrayAhoCorasickMatchKind,
};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::process::constants::*;
use crate::process::single_char_matcher::{SingleCharMatch, SingleCharMatcher};

thread_local! {
    static STRING_POOL: RefCell<Vec<String>> = RefCell::new(Vec::with_capacity(16));
    static REDUCE_STATE: RefCell<Vec<usize>> = RefCell::new(Vec::with_capacity(16));
}

/// Pops a reusable [`String`] from the thread-local pool, or allocates a new one.
///
/// If the pool is non-empty, the returned string is cleared and its capacity
/// is grown to at least `capacity` bytes. When finished, callers should
/// return the string via [`return_string_to_pool`] to avoid repeated allocations.
///
/// # Arguments
/// * `capacity` - Minimum byte capacity the returned string must satisfy.
///
/// # Returns
/// An empty [`String`] with at least `capacity` bytes of allocated space.
pub fn get_string_from_pool(capacity: usize) -> String {
    STRING_POOL.with(|pool| {
        if let Some(mut s) = pool.borrow_mut().pop() {
            s.clear();
            if s.capacity() < capacity {
                s.reserve(capacity - s.capacity());
            }
            s
        } else {
            String::with_capacity(capacity)
        }
    })
}

/// Returns a [`String`] to the thread-local pool for future reuse.
///
/// If the pool already holds 128 strings the value is dropped instead,
/// preventing unbounded memory growth on long-lived threads.
///
/// # Arguments
/// * `s` - The string to recycle. Its contents are irrelevant; it will be
///   cleared the next time it is checked out via [`get_string_from_pool`].
pub fn return_string_to_pool(s: String) {
    STRING_POOL.with(|pool| {
        let mut pool = pool.borrow_mut();
        if pool.len() < 128 {
            pool.push(s);
        }
    });
}

/// Drains a [`ProcessedTextMasks`] collection and returns all owned strings to the pool.
///
/// Borrowed variants (`Cow::Borrowed`) are simply dropped. This should be called
/// after a [`reduce_text_process_with_tree`] / [`reduce_text_process_with_set`] pipeline
/// is finished to reclaim temporary string allocations.
///
/// # Arguments
/// * `processed_text_process_type_masks` - The collection to drain; consumed by this call.
pub fn return_processed_string_to_pool(mut processed_text_process_type_masks: ProcessedTextMasks) {
    for (cow, _) in processed_text_process_type_masks.drain(..) {
        if let Cow::Owned(s) = cow {
            return_string_to_pool(s);
        }
    }
}

bitflags! {
    /// Bitflags controlling which text normalization steps to apply before matching.
    ///
    /// Flags can be combined freely. The matcher builds an internal transformation DAG
    /// from the active flag set and reuses shared intermediate results (e.g., a
    /// `Fanjian | Delete` rule and a `Fanjian | Normalize` rule share the Fanjian output).
    ///
    /// | Flag | Transformation |
    /// |------|----------------|
    /// | `None` | No transformation; match the raw input. |
    /// | `Fanjian` | Traditional Chinese → Simplified Chinese (O(1) page-table lookup). |
    /// | `Delete` | Remove noise characters and whitespace (flat BitSet, O(1) per codepoint). |
    /// | `Normalize` | Full-width → half-width, digit normalization, etc. (leftmost-longest AC). |
    /// | `DeleteNormalize` | Shorthand for `Delete \| Normalize`. |
    /// | `FanjianDeleteNormalize` | Shorthand for `Fanjian \| Delete \| Normalize`. |
    /// | `PinYin` | Convert Chinese characters to space-separated Pinyin syllables. |
    /// | `PinYinChar` | Convert Chinese characters to Pinyin with leading characters only (no spaces). |
    ///
    /// # Examples
    ///
    /// ```rust
    /// use matcher_rs::ProcessType;
    ///
    /// // Compose flags with | just like standard bitflags.
    /// let combined = ProcessType::Fanjian | ProcessType::Delete;
    /// assert!(combined.contains(ProcessType::Fanjian));
    /// assert!(combined.contains(ProcessType::Delete));
    ///
    /// // Serialize/deserialize as a raw u8.
    /// let bits = combined.bits();
    /// assert_eq!(ProcessType::from_bits_retain(bits), combined);
    /// ```
    #[derive(Hash, PartialEq, Eq, Clone, Copy, Debug, Default)]
    pub struct ProcessType: u8 {
        /// No transformation; match the raw input.
        const None = 0b00000001;

        /// Traditional Chinese → Simplified Chinese conversion.
        const Fanjian = 0b00000010;

        /// Remove noise characters and whitespace.
        const Delete = 0b00000100;

        /// Unicode normalization (full-width→half-width, digit normalization, etc.).
        const Normalize = 0b00001000;

        /// Shorthand for `Delete | Normalize`.
        const DeleteNormalize = 0b00001100;

        /// Shorthand for `Fanjian | Delete | Normalize`.
        const FanjianDeleteNormalize = 0b00001110;

        /// Convert Chinese characters to space-separated Pinyin syllables.
        const PinYin = 0b00010000;

        /// Convert Chinese characters to Pinyin, stripping inter-syllable spaces.
        const PinYinChar = 0b00100000;
    }
}

impl Serialize for ProcessType {
    /// Serializes as the raw `u8` bit representation.
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.bits().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ProcessType {
    /// Deserializes from a raw `u8` bit representation, preserving unknown bits via
    /// [`from_bits_retain`](ProcessType::from_bits_retain).
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let bits: u8 = u8::deserialize(deserializer)?;
        Ok(ProcessType::from_bits_retain(bits))
    }
}

impl Display for ProcessType {
    /// Formats the [`ProcessType`] as a `"_"`-joined list of lowercase flag names.
    ///
    /// For example, `ProcessType::Fanjian | ProcessType::Delete` renders as `"fanjian_delete"`.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let display_str_list = self
            .iter_names()
            .map(|(name, _)| name.to_lowercase())
            .collect::<Vec<_>>();
        write!(f, "{:?}", display_str_list.join("_"))
    }
}

type ProcessMatcherResult = (Vec<&'static str>, ProcessMatcher);

/// A lock-free, lazily-initialized array mapping bit positions to process matchers.
///
/// Maps the bit position of a single-bit [`ProcessType`] to an [`Arc`] instance holding
/// a tuple of a replacement-string list and a `ProcessMatcher`.
///
/// The array has a capacity of 8, covering all possible bits in the 8-bit [`ProcessType`].
pub static PROCESS_MATCHER_CACHE: [OnceLock<ProcessMatcherResult>; 8] = [
    OnceLock::new(),
    OnceLock::new(),
    OnceLock::new(),
    OnceLock::new(),
    OnceLock::new(),
    OnceLock::new(),
    OnceLock::new(),
    OnceLock::new(),
];

/// A fixed-capacity array of `(processed_text, u64)` pairs produced by
/// the `reduce_text_process_*` family of functions.
///
/// The capacity of 16 supports up to 16 distinct text-processing variants per input,
/// which is the practical upper bound given the number of [`ProcessType`] flags.
///
/// # Type Parameters
/// * `'a` - The lifetime of the underlying string slice.
pub type ProcessedTextMasks<'a> = Vec<(Cow<'a, str>, u64)>;

/// Underlying engine used by a single-step text transformation.
///
/// Consumers should not construct this directly; use [`get_process_matcher`] to
/// obtain a cached instance for a given [`ProcessType`] bit.
///
/// # Variants
///
/// * `DAAC` *(non-`dfa` builds)* — [`daachorse`] double-array Aho-Corasick; used for
///   leftmost-longest multi-character substitutions (Normalize).
/// * `AC` — Standard [`aho_corasick::AhoCorasick`]; used for Normalize when the `dfa`
///   feature is enabled, and as an empty no-op for `ProcessType::None`.
/// * `SingleChar` — O(1) per-codepoint dispatch via a [`SingleCharMatcher`]; used for
///   Fanjian (2-stage page table), Pinyin (2-stage page table + string buffer), and
///   Delete (flat BitSet covering all Unicode planes).
#[derive(Clone)]
pub enum ProcessMatcher {
    #[cfg(not(feature = "dfa"))]
    DAAC(CharwiseDoubleArrayAhoCorasick<u32>),
    AC(AhoCorasick),
    SingleChar(SingleCharMatcher),
}

impl ProcessMatcher {
    /// Replaces all matched patterns in `text` with the corresponding entries from
    /// `process_replace_list`.
    ///
    /// Returns `(true, Cow::Owned(result))` when at least one replacement was made, or
    /// `(false, Cow::Borrowed(text))` when the text is unchanged, avoiding any allocation.
    #[inline(always)]
    pub fn replace_all<'a>(
        &self,
        text: &'a str,
        process_replace_list: &[&str],
    ) -> (bool, Cow<'a, str>) {
        /// Shared iteration logic for replace: given a mutable iterator of matches,
        /// builds the replaced string using the `$idx` expression to look up the
        /// replacement index from each match object.
        macro_rules! do_replace {
            ($iter:expr, $idx:expr) => {{
                let mut iter = $iter;
                if let Some(first_mat) = iter.next() {
                    let mut result = get_string_from_pool(text.len());
                    result.push_str(&text[0..first_mat.start()]);
                    result.push_str(process_replace_list[$idx(&first_mat)]);
                    let mut last_end = first_mat.end();
                    for mat in iter {
                        result.push_str(&text[last_end..mat.start()]);
                        result.push_str(process_replace_list[$idx(&mat)]);
                        last_end = mat.end();
                    }
                    result.push_str(&text[last_end..]);
                    return (true, Cow::Owned(result));
                }
            }};
        }

        match self {
            ProcessMatcher::SingleChar(matcher) => match matcher {
                SingleCharMatcher::Delete { .. } => unreachable!(),
                _ => {
                    let mut iter = matcher.find_iter(text);
                    if let Some((start, end, m)) = iter.next() {
                        let mut result = get_string_from_pool(text.len());
                        result.push_str(&text[0..start]);
                        match m {
                            SingleCharMatch::Char(c) => result.push(c),
                            SingleCharMatch::Str(s) => result.push_str(s),
                            SingleCharMatch::Delete => {}
                        }
                        let mut last_end = end;
                        for (start, end, m) in iter {
                            result.push_str(&text[last_end..start]);
                            match m {
                                SingleCharMatch::Char(c) => result.push(c),
                                SingleCharMatch::Str(s) => result.push_str(s),
                                SingleCharMatch::Delete => {}
                            }
                            last_end = end;
                        }
                        result.push_str(&text[last_end..]);
                        return (true, Cow::Owned(result));
                    }
                    return (false, Cow::Borrowed(text));
                }
            },
            #[cfg(not(feature = "dfa"))]
            ProcessMatcher::DAAC(ac) => do_replace!(
                ac.leftmost_find_iter(text),
                |m: &daachorse::Match<u32>| m.value() as usize
            ),
            ProcessMatcher::AC(ac) => {
                do_replace!(ac.find_iter(text), |m: &aho_corasick::Match| m
                    .pattern()
                    .as_usize())
            }
        }
        (false, Cow::Borrowed(text))
    }

    /// Removes all matched patterns from `text`.
    ///
    /// Returns `(true, Cow::Owned(result))` when at least one character or span was removed, or
    /// `(false, Cow::Borrowed(text))` when nothing matched, avoiding any allocation.
    #[inline(always)]
    pub fn delete_all<'a>(&self, text: &'a str) -> (bool, Cow<'a, str>) {
        /// Shared iteration logic for delete: given a mutable iterator of matches,
        /// builds the result string by concatenating only the non-matched segments.
        macro_rules! do_delete {
            ($iter:expr) => {{
                let mut iter = $iter;
                if let Some(first_mat) = iter.next() {
                    let mut result = get_string_from_pool(text.len());
                    result.push_str(&text[0..first_mat.start()]);
                    let mut last_end = first_mat.end();
                    for mat in iter {
                        result.push_str(&text[last_end..mat.start()]);
                        last_end = mat.end();
                    }
                    result.push_str(&text[last_end..]);
                    return (true, Cow::Owned(result));
                }
            }};
        }

        match self {
            ProcessMatcher::SingleChar(matcher) => match matcher {
                SingleCharMatcher::Delete { .. } => {
                    let mut iter = matcher.find_iter(text);
                    if let Some((start, end, _)) = iter.next() {
                        let mut result = get_string_from_pool(text.len());
                        result.push_str(&text[0..start]);
                        let mut last_end = end;
                        for (start, end, _) in iter {
                            result.push_str(&text[last_end..start]);
                            last_end = end;
                        }
                        result.push_str(&text[last_end..]);
                        return (true, Cow::Owned(result));
                    }
                    return (false, Cow::Borrowed(text));
                }
                _ => unreachable!(),
            },
            #[cfg(not(feature = "dfa"))]
            ProcessMatcher::DAAC(ac) => do_delete!(ac.leftmost_find_iter(text)),
            ProcessMatcher::AC(ac) => do_delete!(ac.find_iter(text)),
        }
        (false, Cow::Borrowed(text))
    }
}

/// Returns a lazily-initialized `ProcessMatcher` for a **single-bit** [`ProcessType`].
///
/// The result is cached in `PROCESS_MATCHER_CACHE` keyed by the bit index, so subsequent
/// calls for the same type are O(1) `Arc::clone` operations without any lock contention
/// (uses [`std::sync::OnceLock`]).
///
/// The construction strategy depends on the type:
/// - **Normalize** — builds a leftmost-longest Aho-Corasick automaton (`daachorse` by default,
///   DFA variant under the `dfa` feature). With `runtime_build` the normalization table is
///   read from `process_map/` text files; otherwise a pre-compiled binary is embedded at
///   build time.
/// - **Fanjian / PinYin / PinYinChar** — zero-copy 2-stage page tables loaded from binary
///   data embedded at build time (or built dynamically under `runtime_build`).
/// - **Delete** — a 139 KB flat BitSet embedded at build time (or built dynamically).
/// - **None** — an empty Aho-Corasick automaton (no-op).
///
/// # Arguments
/// * `process_type_bit` — A single-bit [`ProcessType`]. Passing a composite (multi-bit) value
///   is not supported and will panic in the non-`runtime_build` path (`unreachable!()`).
///
/// # Returns
/// An [`Arc`] holding `(replacement_list, matcher)`. The replacement list is only
/// populated for `Normalize`; it is empty for all other types.
///
/// # Panics
/// Panics (via `unreachable!()`) if `process_type_bit` is a composite or unrecognized value
/// when the `runtime_build` feature is disabled.
///
/// # Examples
///
/// ```rust
/// use matcher_rs::{ProcessType, get_process_matcher};
///
/// let result = get_process_matcher(ProcessType::Fanjian);
/// let (_, matcher) = result;
/// let (changed, simplified) = matcher.replace_all("漢字", &[]);
/// // Traditional '漢' and '字' map to Simplified '汉' and '字'
/// ```
pub fn get_process_matcher(
    process_type_bit: ProcessType,
) -> &'static (Vec<&'static str>, ProcessMatcher) {
    let index = process_type_bit.bits().trailing_zeros() as usize;
    debug_assert!(index < 8, "ProcessType bit index out of bounds");

    PROCESS_MATCHER_CACHE[index].get_or_init(|| {
        #[cfg(feature = "runtime_build")]
        {
            fn build_2_stage_table_runtime(map: &HashMap<u32, u32>) -> (Vec<u8>, Vec<u8>) {
                let mut pages = HashSet::new();
                for &k in map.keys() {
                    pages.insert(k >> 8);
                }
                let mut page_list: Vec<u32> = pages.into_iter().collect();
                page_list.sort_unstable();
                let mut l1 = vec![0u16; 4352];
                let mut l2 = vec![0u32; (page_list.len() + 1) * 256];
                for (i, &page) in page_list.iter().enumerate() {
                    let l2_page_idx = (i + 1) as u16;
                    l1[page as usize] = l2_page_idx;
                    for char_idx in 0..256 {
                        let cp = (page << 8) | char_idx;
                        if let Some(&val) = map.get(&cp) {
                            l2[(l2_page_idx as usize * 256) + char_idx as usize] = val;
                        }
                    }
                }
                let mut l1_bytes = Vec::with_capacity(l1.len() * 2);
                for val in l1 {
                    l1_bytes.extend_from_slice(&val.to_le_bytes());
                }
                let mut l2_bytes = Vec::with_capacity(l2.len() * 4);
                for val in l2 {
                    l2_bytes.extend_from_slice(&val.to_le_bytes());
                }
                (l1_bytes, l2_bytes)
            }

            let process_matcher = match process_type_bit {
                ProcessType::Fanjian => {
                    let mut map = HashMap::new();
                    for line in FANJIAN.trim().lines() {
                        let mut split = line.split('\t');
                        let k = split.next().unwrap().chars().next().unwrap() as u32;
                        let v = split.next().unwrap().chars().next().unwrap() as u32;
                        if k != v {
                            map.insert(k, v);
                        }
                    }
                    let (l1, l2) = build_2_stage_table_runtime(&map);
                    ProcessMatcher::SingleChar(SingleCharMatcher::Fanjian {
                        l1: Cow::Owned(l1),
                        l2: Cow::Owned(l2),
                    })
                }
                ProcessType::PinYin | ProcessType::PinYinChar => {
                    let mut map = HashMap::new();
                    let mut strings = String::new();
                    for line in PINYIN.trim().lines() {
                        let mut split = line.split('\t');
                        let k = split.next().unwrap().chars().next().unwrap() as u32;
                        let v = split.next().unwrap();
                        let offset = strings.len();
                        strings.push_str(v);
                        let length = v.len();
                        map.insert(k, ((offset as u32) << 8) | (length as u32));
                    }
                    let (l1, l2) = build_2_stage_table_runtime(&map);
                    ProcessMatcher::SingleChar(SingleCharMatcher::Pinyin {
                        l1: Cow::Owned(l1),
                        l2: Cow::Owned(l2),
                        strings: Cow::Owned(strings),
                        trim_space: process_type_bit == ProcessType::PinYinChar,
                    })
                }
                ProcessType::Delete => {
                    let mut bitset = vec![0u8; 139264];
                    for line in TEXT_DELETE.trim().lines() {
                        for c in line.chars() {
                            let cp = c as usize;
                            bitset[cp / 8] |= 1 << (cp % 8);
                        }
                    }
                    for &val in WHITE_SPACE {
                        for c in val.chars() {
                            let cp = c as usize;
                            bitset[cp / 8] |= 1 << (cp % 8);
                        }
                    }
                    ProcessMatcher::SingleChar(SingleCharMatcher::Delete {
                        bitset: Cow::Owned(bitset),
                    })
                }
                ProcessType::Normalize => {
                    let mut process_dict = HashMap::new();
                    for process_map in [NORM, NUM_NORM] {
                        process_dict.extend(process_map.trim().lines().map(|pair_str| {
                            let mut pair_str_split = pair_str.split('\t');
                            (
                                pair_str_split.next().unwrap(),
                                pair_str_split.next().unwrap(),
                            )
                        }));
                    }
                    process_dict.retain(|&key, &mut value| key != value);
                    let mut keys: Vec<&str> = process_dict.keys().copied().collect();
                    keys.sort_unstable();
                    #[cfg(not(feature = "dfa"))]
                    {
                        ProcessMatcher::DAAC(
                            CharwiseDoubleArrayAhoCorasickBuilder::new()
                                .match_kind(DoubleArrayAhoCorasickMatchKind::LeftmostLongest)
                                .build(&keys)
                                .unwrap(),
                        )
                    }
                    #[cfg(feature = "dfa")]
                    {
                        ProcessMatcher::AC(
                            AhoCorasickBuilder::new()
                                .kind(Some(AhoCorasickKind::DFA))
                                .match_kind(AhoCorasickMatchKind::LeftmostLongest)
                                .build(&keys)
                                .unwrap(),
                        )
                    }
                }
                _ => ProcessMatcher::AC(AhoCorasick::new(Vec::<&str>::new()).unwrap()),
            };

            let process_replace_list = match process_type_bit {
                ProcessType::Normalize => {
                    let mut process_dict = HashMap::new();
                    for process_map in [NORM, NUM_NORM] {
                        process_dict.extend(process_map.trim().lines().map(|pair_str| {
                            let mut pair_str_split = pair_str.split('\t');
                            (
                                pair_str_split.next().unwrap(),
                                pair_str_split.next().unwrap(),
                            )
                        }));
                    }
                    process_dict.retain(|&key, &mut value| key != value);
                    let mut pairs: Vec<(&str, &str)> = process_dict.into_iter().collect();
                    pairs.sort_unstable_by_key(|&(k, _)| k);
                    pairs.into_iter().map(|(_, v)| v).collect()
                }
                _ => Vec::new(),
            };

            (process_replace_list, process_matcher)
        }

        #[cfg(not(feature = "runtime_build"))]
        {
            let (process_replace_list, process_matcher) = match process_type_bit {
                ProcessType::None => {
                    let empty_patterns: Vec<&str> = Vec::new();
                    (
                        Vec::new(),
                        ProcessMatcher::AC(AhoCorasick::new(&empty_patterns).unwrap()),
                    )
                }
                ProcessType::Fanjian => (
                    Vec::new(),
                    ProcessMatcher::SingleChar(SingleCharMatcher::Fanjian {
                        l1: Cow::Borrowed(FANJIAN_L1_BYTES),
                        l2: Cow::Borrowed(FANJIAN_L2_BYTES),
                    }),
                ),
                ProcessType::Delete => (
                    Vec::new(),
                    ProcessMatcher::SingleChar(SingleCharMatcher::Delete {
                        bitset: Cow::Borrowed(DELETE_BITSET_BYTES),
                    }),
                ),
                ProcessType::Normalize => {
                    #[cfg(feature = "dfa")]
                    {
                        (
                            NORMALIZE_PROCESS_REPLACE_LIST_STR.lines().collect(),
                            ProcessMatcher::AC(
                                AhoCorasickBuilder::new()
                                    .kind(Some(AhoCorasickKind::DFA))
                                    .match_kind(AhoCorasickMatchKind::LeftmostLongest)
                                    .build(NORMALIZE_PROCESS_LIST_STR.lines())
                                    .unwrap(),
                            ),
                        )
                    }
                    #[cfg(not(feature = "dfa"))]
                    {
                        (
                            NORMALIZE_PROCESS_REPLACE_LIST_STR.lines().collect(),
                            ProcessMatcher::DAAC(unsafe {
                                CharwiseDoubleArrayAhoCorasick::<u32>::deserialize_unchecked(
                                    NORMALIZE_PROCESS_MATCHER_BYTES,
                                )
                                .0
                            }),
                        )
                    }
                }
                ProcessType::PinYin => (
                    Vec::new(),
                    ProcessMatcher::SingleChar(SingleCharMatcher::Pinyin {
                        l1: Cow::Borrowed(PINYIN_L1_BYTES),
                        l2: Cow::Borrowed(PINYIN_L2_BYTES),
                        strings: Cow::Borrowed(PINYIN_STR_BYTES),
                        trim_space: false,
                    }),
                ),
                ProcessType::PinYinChar => (
                    Vec::new(),
                    ProcessMatcher::SingleChar(SingleCharMatcher::Pinyin {
                        l1: Cow::Borrowed(PINYIN_L1_BYTES),
                        l2: Cow::Borrowed(PINYIN_L2_BYTES),
                        strings: Cow::Borrowed(PINYIN_STR_BYTES),
                        trim_space: true,
                    }),
                ),
                _ => unreachable!(),
            };
            (process_replace_list, process_matcher)
        }
    })
}

/// Applies a composite [`ProcessType`] pipeline to `text` and returns the final result.
///
/// Transformations are applied left-to-right in bit order. Each step fetches a cached
/// matcher from `PROCESS_MATCHER_CACHE` and either replaces or deletes matching spans.
/// `Cow::Borrowed` is returned when no step modifies the text.
///
/// For use cases where multiple composite types share common prefixes, prefer
/// [`reduce_text_process_with_tree`] which avoids redundant intermediate computations.
///
/// # Examples
///
/// ```rust
/// use matcher_rs::{text_process, ProcessType};
///
/// // Full-width digit '２' (U+FF12) normalizes to ASCII '2'.
/// let result = text_process(ProcessType::Normalize, "２");
/// assert_eq!(result.as_ref(), "2");
/// ```
#[inline(always)]
pub fn text_process<'a>(process_type_bit: ProcessType, text: &'a str) -> Cow<'a, str> {
    let mut result = Cow::Borrowed(text);

    for bit in process_type_bit.iter() {
        let (process_replace_list, process_matcher) = get_process_matcher(bit);

        match (bit, process_matcher) {
            (ProcessType::None, _) => continue,
            (ProcessType::Delete, pm) => {
                if let (true, Cow::Owned(pt)) = pm.delete_all(result.as_ref()) {
                    result = Cow::Owned(pt);
                }
            }
            (_, pm) => {
                if let (true, Cow::Owned(pt)) =
                    pm.replace_all(result.as_ref(), process_replace_list)
                {
                    result = Cow::Owned(pt);
                }
            }
        }
    }

    result
}

/// Applies a composite [`ProcessType`] pipeline to `text`, recording every intermediate
/// variant that diverges from its predecessor.
///
/// Unlike [`text_process`], which returns only the final result, this function pushes a
/// new entry into the output vector whenever a step actually changes the text. The first
/// entry is always `Cow::Borrowed(text)` (the original input). Steps that leave the text
/// unchanged add no entry.
///
/// Primarily useful when you need access to each pipeline stage independently (e.g.
/// scoring). For the common case of generating all variants needed for matching, prefer
/// [`reduce_text_process_with_tree`] which shares intermediate results across multiple
/// `ProcessType` combinations.
///
/// # Arguments
/// * `process_type` - Composite transformation flags to apply.
/// * `text` - The input string.
///
/// # Returns
/// A `Vec` whose first element is the original text and whose subsequent elements are the
/// outputs of each step that produced a new string.
#[inline(always)]
pub fn reduce_text_process<'a>(process_type: ProcessType, text: &'a str) -> Vec<Cow<'a, str>> {
    let mut processed_text_list: Vec<Cow<'a, str>> = Vec::new();
    processed_text_list.push(Cow::Borrowed(text));

    for process_type_bit in process_type.iter() {
        let (process_replace_list, process_matcher) = get_process_matcher(process_type_bit);
        let tmp_processed_text = processed_text_list
            .last_mut()
            .expect("It should always have at least one element");

        match (process_type_bit, process_matcher) {
            (ProcessType::None, _) => {}
            (ProcessType::Delete, pm) => match pm.delete_all(tmp_processed_text.as_ref()) {
                (true, Cow::Owned(pt)) => {
                    processed_text_list.push(Cow::Owned(pt));
                }
                (false, _) => {}
                (_, _) => unreachable!(),
            },
            (_, pm) => match pm.replace_all(tmp_processed_text.as_ref(), process_replace_list) {
                (true, Cow::Owned(pt)) => {
                    processed_text_list.push(Cow::Owned(pt));
                }
                (false, _) => {}
                (_, _) => unreachable!(),
            },
        }
    }

    processed_text_list
}

/// Like [`reduce_text_process`], but composing replace-type steps in-place.
///
/// The key difference from [`reduce_text_process`]: when a *replace*-type step
/// (`Fanjian`, `Normalize`, `PinYin`, `PinYinChar`) changes the text, the result
/// overwrites the last entry in the output vector rather than appending a new one.
/// Only `Delete` steps append a new entry, because deletion creates an independent
/// branch (the pre-deletion text may also be needed for matching).
///
/// This compact representation is used internally by `SimpleMatcher::new` to
/// register all required automaton patterns for each rule: each entry in the output
/// is a distinct normalized form that must be indexed.
///
/// # Arguments
/// * `process_type` - Composite transformation flags to apply.
/// * `text` - The input string.
///
/// # Returns
/// A `Vec` whose first entry is the original text. Replace-type steps that modify the
/// text update the last entry in-place; `Delete` steps append a new entry.
#[inline(always)]
pub fn reduce_text_process_emit<'a>(process_type: ProcessType, text: &'a str) -> Vec<Cow<'a, str>> {
    let mut processed_text_list: Vec<Cow<'a, str>> = Vec::new();
    processed_text_list.push(Cow::Borrowed(text));

    for process_type_bit in process_type.iter() {
        let (process_replace_list, process_matcher) = get_process_matcher(process_type_bit);
        let tmp_processed_text = processed_text_list
            .last_mut()
            .expect("It should always have at least one element");

        match (process_type_bit, process_matcher) {
            (ProcessType::None, _) => {}
            (ProcessType::Delete, pm) => match pm.delete_all(tmp_processed_text.as_ref()) {
                (true, Cow::Owned(pt)) => {
                    processed_text_list.push(Cow::Owned(pt));
                }
                (false, _) => {}
                (_, _) => unreachable!(),
            },
            (_, pm) => match pm.replace_all(tmp_processed_text.as_ref(), process_replace_list) {
                (true, Cow::Owned(pt)) => {
                    *tmp_processed_text = Cow::Owned(pt);
                }
                (false, _) => {}
                (_, _) => unreachable!(),
            },
        }
    }

    processed_text_list
}

/// A node in the flat-array transformation trie used by [`reduce_text_process_with_tree`].
///
/// The trie is built once by [`build_process_type_tree`] and stored inside `SimpleMatcher`.
/// Each node represents a single transformation step (`process_type_bit`) reachable from
/// its parent. The `process_type_list` records which composite [`ProcessType`] values
/// "arrive" at this node (i.e. terminate here), so the traversal can tag output text
/// variants with the correct bitmask. `children` holds flat-array indices of the next steps.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ProcessTypeBitNode {
    process_type_list: Vec<ProcessType>,
    process_type_bit: ProcessType,
    children: Vec<usize>,
}

/// Builds a flat-array trie from a set of composite [`ProcessType`] bitmasks.
///
/// The trie encodes every unique prefix path among the given composite types. A root node
/// with `process_type_bit = ProcessType::None` is always present at index 0. For each
/// composite type (e.g. `Fanjian | Delete`), the constructor walks its constituent bits in
/// order, reusing existing child nodes where the path overlaps with previously inserted types
/// and creating new child nodes when a path diverges.
///
/// The resulting flat `Vec<ProcessTypeBitNode>` is passed to
/// [`reduce_text_process_with_tree`], which performs a single BFS traversal to compute all
/// needed text variants while sharing common intermediate results.
///
/// # Arguments
/// * `process_type_set` - Raw `u8` bit patterns of all composite `ProcessType`s needed by a matcher.
///
/// # Returns
/// A flat `Vec` of `ProcessTypeBitNode`s whose `children` fields hold indices into the same `Vec`.
pub fn build_process_type_tree(process_type_set: &HashSet<u8>) -> Vec<ProcessTypeBitNode> {
    let mut process_type_tree = Vec::new();
    let root = ProcessTypeBitNode {
        process_type_list: Vec::new(),
        process_type_bit: ProcessType::None,
        children: Vec::new(),
    };
    process_type_tree.push(root);
    for process_type_bits in process_type_set.iter() {
        let process_type = ProcessType::from_bits(*process_type_bits).unwrap();
        let mut current_node_index = 0;
        for process_type_bit in process_type.into_iter() {
            let current_node = &process_type_tree[current_node_index];
            if current_node.process_type_bit == process_type_bit {
                continue;
            }

            let mut is_found = false;
            for child_node_index in &current_node.children {
                if process_type_bit == process_type_tree[*child_node_index].process_type_bit {
                    current_node_index = *child_node_index;
                    is_found = true;
                    break;
                }
            }

            if !is_found {
                let mut child = ProcessTypeBitNode {
                    process_type_list: Vec::new(),
                    process_type_bit,
                    children: Vec::new(),
                };
                child.process_type_list.push(process_type);
                process_type_tree.push(child);
                let new_node_index = process_type_tree.len() - 1;
                process_type_tree[current_node_index]
                    .children
                    .push(new_node_index);
                current_node_index = new_node_index;
            } else {
                process_type_tree[current_node_index]
                    .process_type_list
                    .push(process_type);
            }
        }
    }
    process_type_tree
}

/// Generates all text variants required for matching, using a pre-built transformation trie.
///
/// This is the hot-path function called on every [`crate::SimpleMatcher::is_match`] /
/// [`crate::SimpleMatcher::process`] invocation. It performs a single left-to-right traversal of
/// `process_type_tree`, applying each transformation step exactly once per unique path. Common
/// prefixes (e.g. the shared `Fanjian` step for both `Fanjian | Delete` and
/// `Fanjian | Normalize`) are computed only once and their result indices are reused for child
/// nodes.
///
/// Each entry in the returned [`ProcessedTextMasks`] is a `(text_variant, rule_bitmask)` pair.
/// The bitmask encodes which composite [`ProcessType`]s produced that variant, so the matcher
/// can filter automaton hits by the correct pipeline.
///
/// # Arguments
/// * `process_type_tree` - The trie produced by [`build_process_type_tree`], typically stored
///   inside the `SimpleMatcher`.
/// * `text` - The raw input string.
///
/// # Returns
/// All text variants and their associated [`ProcessType`] bitmasks. The caller must return
/// owned strings to the pool via `return_processed_string_to_pool` when done.
#[inline(always)]
pub fn reduce_text_process_with_tree<'a>(
    process_type_tree: &[ProcessTypeBitNode],
    text: &'a str,
) -> ProcessedTextMasks<'a> {
    REDUCE_STATE.with(|state| {
        let mut node_processed_indices = state.borrow_mut();
        node_processed_indices.clear();
        node_processed_indices.resize(process_type_tree.len(), 0);

        let mut processed_text_process_type_masks: ProcessedTextMasks<'a> = Vec::new();
        processed_text_process_type_masks
            .push((Cow::Borrowed(text), 1u64 << ProcessType::None.bits()));

        for (current_node_index, current_node) in process_type_tree.iter().enumerate() {
            let current_index = node_processed_indices[current_node_index];

            for &child_node_index in &current_node.children {
                let child_node = &process_type_tree[child_node_index];
                let mut child_index = current_index;

                let (process_replace_list, process_matcher) =
                    get_process_matcher(child_node.process_type_bit);

                match child_node.process_type_bit {
                    ProcessType::None => {}
                    ProcessType::Delete => {
                        let current_text =
                            processed_text_process_type_masks[current_index].0.as_ref();
                        match process_matcher.delete_all(current_text) {
                            (true, Cow::Owned(pt)) => {
                                processed_text_process_type_masks.push((Cow::Owned(pt), 0u64));
                                child_index = processed_text_process_type_masks.len() - 1;
                            }
                            (false, _) => {
                                child_index = current_index;
                            }
                            (_, _) => unreachable!(),
                        }
                    }
                    _ => {
                        let current_text =
                            processed_text_process_type_masks[current_index].0.as_ref();
                        match process_matcher.replace_all(current_text, process_replace_list) {
                            (true, Cow::Owned(pt)) => {
                                processed_text_process_type_masks.push((Cow::Owned(pt), 0u64));
                                child_index = processed_text_process_type_masks.len() - 1;
                            }
                            (false, _) => {
                                child_index = current_index;
                            }
                            (_, _) => unreachable!(),
                        }
                    }
                }

                node_processed_indices[child_node_index] = child_index;
                let processed_text_process_type_tuple =
                    &mut processed_text_process_type_masks[child_index];
                processed_text_process_type_tuple.1 |= child_node
                    .process_type_list
                    .iter()
                    .fold(0u64, |mask, smt| mask | (1u64 << smt.bits()));
            }
        }

        processed_text_process_type_masks
    })
}

/// Generates all text variants by building the transformation trie on-the-fly.
///
/// Semantically identical to [`reduce_text_process_with_tree`], but constructs the trie
/// from `process_type_set` at call time rather than using a pre-built one. Use this when
/// the set of required `ProcessType`s is dynamic or not known at construction time.
/// For static sets, prefer pre-building with [`build_process_type_tree`] and calling
/// [`reduce_text_process_with_tree`] instead.
///
/// # Arguments
/// * `process_type_set` - Raw `u8` bit patterns of all composite `ProcessType`s to handle.
/// * `text` - The raw input string.
///
/// # Returns
/// All text variants and their associated [`ProcessType`] bitmasks.
#[inline(always)]
pub fn reduce_text_process_with_set<'a>(
    process_type_set: &HashSet<u8>,
    text: &'a str,
) -> ProcessedTextMasks<'a> {
    let mut process_type_tree = Vec::with_capacity(8);
    let root = ProcessTypeBitNode {
        process_type_list: Vec::new(),
        process_type_bit: ProcessType::None,
        children: Vec::new(),
    };
    process_type_tree.push(root);

    let mut node_processed_indices = Vec::with_capacity(8);
    node_processed_indices.push(0);

    let mut processed_text_process_type_masks: ProcessedTextMasks<'a> = Vec::new();
    processed_text_process_type_masks.push((Cow::Borrowed(text), 1u64 << ProcessType::None.bits()));

    for process_type_bits in process_type_set.iter() {
        let process_type = ProcessType::from_bits(*process_type_bits).unwrap();
        let mut current_node_index = 0;

        for process_type_bit in process_type.iter() {
            let current_node = &process_type_tree[current_node_index];
            if current_node.process_type_bit == process_type_bit {
                continue;
            }

            let mut is_found = false;
            for child_node_index in &current_node.children {
                if process_type_bit == process_type_tree[*child_node_index].process_type_bit {
                    current_node_index = *child_node_index;
                    is_found = true;
                    break;
                }
            }

            if !is_found {
                let current_index = node_processed_indices[current_node_index];
                let mut child_index = current_index;

                let (process_replace_list, process_matcher) = get_process_matcher(process_type_bit);

                match process_type_bit {
                    ProcessType::None => {}
                    ProcessType::Delete => {
                        let current_text =
                            processed_text_process_type_masks[current_index].0.as_ref();
                        match process_matcher.delete_all(current_text) {
                            (true, Cow::Owned(pt)) => {
                                processed_text_process_type_masks.push((Cow::Owned(pt), 0u64));
                                child_index = processed_text_process_type_masks.len() - 1;
                            }
                            (false, _) => {
                                child_index = current_index;
                            }
                            (_, _) => unreachable!(),
                        }
                    }
                    _ => {
                        let current_text =
                            processed_text_process_type_masks[current_index].0.as_ref();
                        match process_matcher.replace_all(current_text, process_replace_list) {
                            (true, Cow::Owned(pt)) => {
                                processed_text_process_type_masks.push((Cow::Owned(pt), 0u64));
                                child_index = processed_text_process_type_masks.len() - 1;
                            }
                            (false, _) => {
                                child_index = current_index;
                            }
                            (_, _) => unreachable!(),
                        }
                    }
                }

                let mut child = ProcessTypeBitNode {
                    process_type_list: Vec::new(),
                    process_type_bit,
                    children: Vec::new(),
                };
                child.process_type_list.push(process_type);
                process_type_tree.push(child);
                node_processed_indices.push(child_index);

                let new_node_index = process_type_tree.len() - 1;
                process_type_tree[current_node_index]
                    .children
                    .push(new_node_index);
                current_node_index = new_node_index;
            } else {
                process_type_tree[current_node_index]
                    .process_type_list
                    .push(process_type);
            }

            let current_index = node_processed_indices[current_node_index];
            let processed_text_process_type_tuple =
                &mut processed_text_process_type_masks[current_index];
            processed_text_process_type_tuple.1 |= 1u64 << process_type.bits();
        }
    }

    processed_text_process_type_masks
}
