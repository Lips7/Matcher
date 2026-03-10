use std::borrow::Cow;
use std::cell::RefCell;
#[cfg(feature = "runtime_build")]
use std::collections::HashMap;
use std::collections::HashSet;
use std::fmt::Display;
use std::sync::OnceLock;

use bitflags::bitflags;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::process::constants::*;
use crate::process::multi_char_matcher::MultiCharMatcher;
use crate::process::single_char_matcher::{SingleCharMatch, SingleCharMatcher};

const STRING_POOL_INIT_CAP: usize = 16;
const TREE_NODE_INDICES_INIT_CAP: usize = 16;
const MASKS_POOL_INIT_CAP: usize = 4;
const STRING_POOL_MAX: usize = 128;
const MASKS_POOL_MAX: usize = 16;

thread_local! {
    static STRING_POOL: RefCell<Vec<String>> = RefCell::new(Vec::with_capacity(STRING_POOL_INIT_CAP));
    static TREE_NODE_INDICES: RefCell<Vec<usize>> = RefCell::new(Vec::with_capacity(TREE_NODE_INDICES_INIT_CAP));
    static MASKS_POOL: RefCell<Vec<ProcessedTextMasks<'static>>> =
        RefCell::new(Vec::with_capacity(MASKS_POOL_INIT_CAP));
}

/// Pops a reusable [`String`] from the thread-local pool, or allocates a new one.
fn get_string_from_pool(capacity: usize) -> String {
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
fn return_string_to_pool(s: String) {
    STRING_POOL.with(|pool| {
        let mut pool = pool.borrow_mut();
        if pool.len() < STRING_POOL_MAX {
            pool.push(s);
        }
    });
}

/// Drains a [`ProcessedTextMasks`] collection and returns all owned strings to the pool.
pub fn return_processed_string_to_pool(mut text_masks: ProcessedTextMasks) {
    for (cow, _) in text_masks.drain(..) {
        if let Cow::Owned(s) = cow {
            return_string_to_pool(s);
        }
    }
    // Safety: drain() has removed all elements, so no Cow<'_, str> borrows remain.
    // Transmuting the empty Vec's element lifetime to 'static is sound because an empty
    // Vec holds no values and the memory layout of Cow<'_, str> is lifetime-independent.
    let empty: ProcessedTextMasks<'static> = unsafe { std::mem::transmute(text_masks) };
    MASKS_POOL.with(|pool| {
        let mut pool = pool.borrow_mut();
        if pool.len() < MASKS_POOL_MAX {
            pool.push(empty);
        }
    });
}

bitflags! {
    /// Bitflags controlling which text normalization steps to apply before matching.
    ///
    /// Flags can be combined freely. The matcher builds an internal transformation DAG
    /// from the active flag set and reuses shared intermediate results (e.g., a
    /// `Fanjian | Delete` rule and a `Fanjian | Normalize` rule share the Fanjian output).
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
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.bits().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ProcessType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let bits: u8 = u8::deserialize(deserializer)?;
        Ok(ProcessType::from_bits_retain(bits))
    }
}

impl Display for ProcessType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let display_str_list = self
            .iter_names()
            .map(|(name, _)| name.to_lowercase())
            .collect::<Vec<_>>();
        write!(f, "{}", display_str_list.join("_"))
    }
}

/// Maps the bit position of a single-bit [`ProcessType`] to its compiled [`ProcessMatcher`].
static PROCESS_MATCHER_CACHE: [OnceLock<ProcessMatcher>; 8] = [
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
/// * `MultiChar` — Multi-character pattern matching via [`MultiCharMatcher`]; used for
///   leftmost-longest multi-character substitutions (Normalize) and as an empty no-op
///   for `ProcessType::None`.
/// * `SingleChar` — O(1) per-codepoint dispatch via a [`SingleCharMatcher`]; used for
///   Fanjian (2-stage page table), Pinyin (2-stage page table + string buffer), and
///   Delete (flat BitSet covering all Unicode planes).
#[derive(Clone)]
pub enum ProcessMatcher {
    MultiChar(MultiCharMatcher),
    SingleChar(SingleCharMatcher),
}

impl ProcessMatcher {
    #[inline(always)]
    fn replace_scan<'a, I, M, F>(
        text: &'a str,
        mut iter: I,
        mut push_replacement: F,
    ) -> (bool, Cow<'a, str>)
    where
        I: Iterator<Item = (usize, usize, M)>,
        F: FnMut(&mut String, M),
    {
        if let Some((start, end, m)) = iter.next() {
            let mut result = get_string_from_pool(text.len());
            result.push_str(&text[0..start]);
            push_replacement(&mut result, m);
            let mut last_end = end;
            for (start, end, m) in iter {
                result.push_str(&text[last_end..start]);
                push_replacement(&mut result, m);
                last_end = end;
            }
            result.push_str(&text[last_end..]);
            (true, Cow::Owned(result))
        } else {
            (false, Cow::Borrowed(text))
        }
    }

    /// Replaces all matched patterns in `text`.
    ///
    /// Returns `(true, Cow::Owned(result))` when at least one replacement was made, or
    /// `(false, Cow::Borrowed(text))` when the text is unchanged, avoiding any allocation.
    #[inline(always)]
    pub fn replace_all<'a>(&self, text: &'a str) -> (bool, Cow<'a, str>) {
        match self {
            ProcessMatcher::SingleChar(matcher) => match matcher {
                SingleCharMatcher::Fanjian { .. } => {
                    Self::replace_scan(text, matcher.fanjian_iter(text), |result, m| {
                        if let SingleCharMatch::Char(c) = m {
                            result.push(c);
                        }
                    })
                }
                SingleCharMatcher::Pinyin { .. } => {
                    Self::replace_scan(text, matcher.pinyin_iter(text), |result, m| {
                        if let SingleCharMatch::Str(s) = m {
                            result.push_str(s);
                        }
                    })
                }
                SingleCharMatcher::Delete { .. } => {
                    debug_assert!(false, "replace_all called on Delete matcher");
                    (false, Cow::Borrowed(text))
                }
            },
            ProcessMatcher::MultiChar(mc) => {
                let rl = mc.replace_list();
                Self::replace_scan(text, mc.find_iter(text), |result, idx| {
                    result.push_str(rl[idx]);
                })
            }
        }
    }

    /// Removes all matched patterns from `text`.
    ///
    /// Returns `(true, Cow::Owned(result))` when at least one character or span was removed, or
    /// `(false, Cow::Borrowed(text))` when nothing matched, avoiding any allocation.
    #[inline(always)]
    pub fn delete_all<'a>(&self, text: &'a str) -> (bool, Cow<'a, str>) {
        let ProcessMatcher::SingleChar(matcher) = self else {
            debug_assert!(false, "delete_all called on non-Delete matcher");
            return (false, Cow::Borrowed(text));
        };
        Self::replace_scan(text, matcher.delete_iter(text), |_, _| {})
    }
}

/// Returns a lazily-initialized `ProcessMatcher` for a **single-bit** [`ProcessType`].
///
/// The result is cached as the same `&'static` reference via OnceLock, so subsequent
/// calls for the same type return the same `&'static ProcessMatcher` without lock contention.
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
/// # Panics
/// Panics (via `unreachable!()`) if `process_type_bit` is a composite or unrecognized value
/// when the `runtime_build` feature is disabled.
///
/// # Examples
///
/// ```rust
/// use matcher_rs::{ProcessType, get_process_matcher};
///
/// let matcher = get_process_matcher(ProcessType::Fanjian);
/// let (changed, simplified) = matcher.replace_all("漢字");
/// // Traditional '漢' and '字' map to Simplified '汉' and '字'
/// ```
pub fn get_process_matcher(process_type_bit: ProcessType) -> &'static ProcessMatcher {
    let index = process_type_bit.bits().trailing_zeros() as usize;
    debug_assert!(index < 8, "ProcessType bit index out of bounds");

    PROCESS_MATCHER_CACHE[index].get_or_init(|| {
        #[cfg(feature = "runtime_build")]
        {
            match process_type_bit {
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
                    ProcessMatcher::SingleChar(SingleCharMatcher::fanjian_from_map(map))
                }
                ProcessType::PinYin | ProcessType::PinYinChar => {
                    let mut map = HashMap::new();
                    for line in PINYIN.trim().lines() {
                        let mut split = line.split('\t');
                        let k = split.next().unwrap().chars().next().unwrap() as u32;
                        let v = split.next().unwrap();
                        map.insert(k, v);
                    }
                    ProcessMatcher::SingleChar(SingleCharMatcher::pinyin_from_map(
                        map,
                        process_type_bit == ProcessType::PinYinChar,
                    ))
                }
                ProcessType::Delete => ProcessMatcher::SingleChar(
                    SingleCharMatcher::delete_from_sources(TEXT_DELETE, WHITE_SPACE),
                ),
                ProcessType::Normalize => {
                    let mut process_dict: HashMap<&'static str, &'static str> = HashMap::new();
                    for process_map in [NORM, NUM_NORM] {
                        process_dict.extend(process_map.trim().lines().map(|pair_str| {
                            let mut split = pair_str.split('\t');
                            (split.next().unwrap(), split.next().unwrap())
                        }));
                    }
                    process_dict.retain(|&key, &mut value| key != value);
                    ProcessMatcher::MultiChar(MultiCharMatcher::new_from_dict(process_dict))
                }
                _ => ProcessMatcher::MultiChar(MultiCharMatcher::new_empty()),
            }
        }

        #[cfg(not(feature = "runtime_build"))]
        {
            match process_type_bit {
                ProcessType::None => ProcessMatcher::MultiChar(MultiCharMatcher::new_empty()),
                ProcessType::Fanjian => ProcessMatcher::SingleChar(SingleCharMatcher::fanjian(
                    Cow::Borrowed(FANJIAN_L1_BYTES),
                    Cow::Borrowed(FANJIAN_L2_BYTES),
                )),
                ProcessType::Delete => ProcessMatcher::SingleChar(SingleCharMatcher::delete(
                    Cow::Borrowed(DELETE_BITSET_BYTES),
                )),
                ProcessType::Normalize => {
                    #[cfg(feature = "dfa")]
                    {
                        ProcessMatcher::MultiChar(
                            MultiCharMatcher::new(NORMALIZE_PROCESS_LIST_STR.lines())
                                .with_replace_list(
                                    NORMALIZE_PROCESS_REPLACE_LIST_STR.lines().collect(),
                                ),
                        )
                    }
                    #[cfg(not(feature = "dfa"))]
                    {
                        ProcessMatcher::MultiChar(
                            MultiCharMatcher::deserialize_from(NORMALIZE_PROCESS_MATCHER_BYTES)
                                .with_replace_list(
                                    NORMALIZE_PROCESS_REPLACE_LIST_STR.lines().collect(),
                                ),
                        )
                    }
                }
                ProcessType::PinYin => ProcessMatcher::SingleChar(SingleCharMatcher::pinyin(
                    Cow::Borrowed(PINYIN_L1_BYTES),
                    Cow::Borrowed(PINYIN_L2_BYTES),
                    Cow::Borrowed(PINYIN_STR_BYTES),
                    false,
                )),
                ProcessType::PinYinChar => ProcessMatcher::SingleChar(SingleCharMatcher::pinyin(
                    Cow::Borrowed(PINYIN_L1_BYTES),
                    Cow::Borrowed(PINYIN_L2_BYTES),
                    Cow::Borrowed(PINYIN_STR_BYTES),
                    true,
                )),
                _ => unreachable!(),
            }
        }
    })
}

/// Applies a composite [`ProcessType`] pipeline to `text` and returns the final result.
///
/// Transformations are applied left-to-right in bit order. Each step fetches a cached
/// matcher and either replaces or deletes matching spans.
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
pub fn text_process<'a>(process_type: ProcessType, text: &'a str) -> Cow<'a, str> {
    let mut result = Cow::Borrowed(text);

    for process_type_bit in process_type.iter() {
        let pm = get_process_matcher(process_type_bit);

        match process_type_bit {
            ProcessType::None => continue,
            ProcessType::Delete => {
                if let (true, Cow::Owned(pt)) = pm.delete_all(result.as_ref())
                    && let Cow::Owned(old) = std::mem::replace(&mut result, Cow::Owned(pt))
                {
                    return_string_to_pool(old);
                }
            }
            _ => {
                if let (true, Cow::Owned(pt)) = pm.replace_all(result.as_ref())
                    && let Cow::Owned(old) = std::mem::replace(&mut result, Cow::Owned(pt))
                {
                    return_string_to_pool(old);
                }
            }
        }
    }

    result
}

/// Applies a composite [`ProcessType`] pipeline to `text`, recording every intermediate
/// variant that diverges from its predecessor.
///
/// The first entry is always `Cow::Borrowed(text)` (the original input). Steps that leave
/// the text unchanged add no entry.
///
/// For generating all variants needed for matching, prefer [`reduce_text_process_with_tree`].
#[inline(always)]
pub fn reduce_text_process<'a>(process_type: ProcessType, text: &'a str) -> Vec<Cow<'a, str>> {
    let mut text_list: Vec<Cow<'a, str>> = Vec::new();
    text_list.push(Cow::Borrowed(text));

    for process_type_bit in process_type.iter() {
        let pm = get_process_matcher(process_type_bit);
        let current_text = text_list
            .last_mut()
            .expect("It should always have at least one element");

        match process_type_bit {
            ProcessType::None => {}
            ProcessType::Delete => {
                if let (true, Cow::Owned(pt)) = pm.delete_all(current_text.as_ref()) {
                    text_list.push(Cow::Owned(pt));
                }
            }
            _ => {
                if let (true, Cow::Owned(pt)) = pm.replace_all(current_text.as_ref()) {
                    text_list.push(Cow::Owned(pt));
                }
            }
        }
    }

    text_list
}

/// Like [`reduce_text_process`], but composing replace-type steps in-place.
///
/// When a *replace*-type step changes the text, the result overwrites the last entry
/// rather than appending a new one. Only `Delete` steps append a new entry.
///
/// Used internally by `SimpleMatcher::new` to register all required automaton patterns.
#[inline(always)]
pub fn reduce_text_process_emit<'a>(process_type: ProcessType, text: &'a str) -> Vec<Cow<'a, str>> {
    let mut text_list: Vec<Cow<'a, str>> = Vec::new();
    text_list.push(Cow::Borrowed(text));

    for process_type_bit in process_type.iter() {
        let pm = get_process_matcher(process_type_bit);
        let current_text = text_list
            .last_mut()
            .expect("It should always have at least one element");

        match process_type_bit {
            ProcessType::None => {}
            ProcessType::Delete => {
                if let (true, Cow::Owned(pt)) = pm.delete_all(current_text.as_ref()) {
                    text_list.push(Cow::Owned(pt));
                }
            }
            _ => {
                if let (true, Cow::Owned(pt)) = pm.replace_all(current_text.as_ref()) {
                    *current_text = Cow::Owned(pt);
                }
            }
        }
    }

    text_list
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
pub fn build_process_type_tree(process_type_set: &HashSet<ProcessType>) -> Vec<ProcessTypeBitNode> {
    let mut process_type_tree = Vec::new();
    let root = ProcessTypeBitNode {
        process_type_list: Vec::new(),
        process_type_bit: ProcessType::None,
        children: Vec::new(),
    };
    process_type_tree.push(root);
    for &process_type in process_type_set.iter() {
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

/// Inserts a transformed text into `text_masks`, deduplicating against existing entries.
///
/// If `changed` is `Some(pt)` and `pt` already exists in `text_masks`, the string is
/// returned to the pool and the existing index is returned. Otherwise `pt` is appended.
/// If `changed` is `None`, `current_index` is returned unchanged.
#[inline(always)]
fn dedup_insert(
    text_masks: &mut ProcessedTextMasks<'_>,
    current_index: usize,
    changed: Option<String>,
) -> usize {
    match changed {
        Some(pt) => {
            if let Some(pos) = text_masks
                .iter()
                .position(|(t, _)| t.as_ref() == pt.as_str())
            {
                return_string_to_pool(pt);
                pos
            } else {
                text_masks.push((Cow::Owned(pt), 0u64));
                text_masks.len() - 1
            }
        }
        None => current_index,
    }
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
/// The caller must return owned strings to the pool via `return_processed_string_to_pool` when done.
#[inline(always)]
pub fn reduce_text_process_with_tree<'a>(
    process_type_tree: &[ProcessTypeBitNode],
    text: &'a str,
) -> ProcessedTextMasks<'a> {
    TREE_NODE_INDICES.with(|state| {
        let mut node_indices = state.borrow_mut();
        node_indices.clear();
        node_indices.resize(process_type_tree.len(), 0);

        let pooled: Option<ProcessedTextMasks<'static>> = MASKS_POOL.with(|p| p.borrow_mut().pop());
        // Safety: pool holds empty Vecs with no live borrows; transmuting from
        // 'static to 'a is safe since 'static: 'a (covariant) and Vec is empty.
        let mut text_masks: ProcessedTextMasks<'a> =
            unsafe { std::mem::transmute(pooled.unwrap_or_default()) };
        text_masks.clear();
        text_masks.push((Cow::Borrowed(text), 1u64 << ProcessType::None.bits()));

        for (current_node_index, current_node) in process_type_tree.iter().enumerate() {
            let current_index = node_indices[current_node_index];

            for &child_node_index in &current_node.children {
                let child_node = &process_type_tree[child_node_index];
                let pm = get_process_matcher(child_node.process_type_bit);

                let changed = match child_node.process_type_bit {
                    ProcessType::None => None,
                    ProcessType::Delete => {
                        let current_text = text_masks[current_index].0.as_ref();
                        match pm.delete_all(current_text) {
                            (true, Cow::Owned(pt)) => Some(pt),
                            _ => None,
                        }
                    }
                    _ => {
                        let current_text = text_masks[current_index].0.as_ref();
                        match pm.replace_all(current_text) {
                            (true, Cow::Owned(pt)) => Some(pt),
                            _ => None,
                        }
                    }
                };
                let child_index = dedup_insert(&mut text_masks, current_index, changed);

                node_indices[child_node_index] = child_index;
                let entry = &mut text_masks[child_index];
                entry.1 |= child_node
                    .process_type_list
                    .iter()
                    .fold(0u64, |mask, pt| mask | (1u64 << pt.bits()));
            }
        }

        text_masks
    })
}
