use std::borrow::Cow;
use std::cell::RefCell;
#[cfg(feature = "runtime_build")]
use std::collections::HashMap;
use std::collections::HashSet;
use std::fmt::Display;
use std::sync::OnceLock;

use tinyvec::TinyVec;

use bitflags::bitflags;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::process::constants::*;
use crate::process::multi_char_matcher::MultiCharMatcher;
use crate::process::single_char_matcher::{SingleCharMatch, SingleCharMatcher};

/// Initial capacity of the per-thread `String` pool (number of pre-allocated slots).
const STRING_POOL_INIT_CAP: usize = 16;
/// Initial capacity of the per-thread `ProcessedTextMasks` pool (number of pre-allocated slots).
const MASKS_POOL_INIT_CAP: usize = 4;
/// Maximum number of `String` buffers retained in the pool between calls; excess are dropped.
const STRING_POOL_MAX: usize = 128;
/// Maximum number of `ProcessedTextMasks` buffers retained in the pool between calls; excess are dropped.
const MASKS_POOL_MAX: usize = 16;

/// Combined thread-local state for `TREE_NODE_INDICES` and `MASKS_POOL`.
///
/// Merging into a single `thread_local!` eliminates one TLS lookup (~5ns) per
/// `walk_process_tree` call.
struct TransformThreadState {
    tree_node_indices: Vec<usize>,
    masks_pool: Vec<ProcessedTextMasks<'static>>,
}

impl TransformThreadState {
    /// Creates empty reusable traversal state for `walk_process_tree`.
    ///
    /// `tree_node_indices` is resized per traversal to map trie node index → text variant
    /// index, while `masks_pool` stores emptied `ProcessedTextMasks` buffers for reuse.
    fn new() -> Self {
        Self {
            tree_node_indices: Vec::with_capacity(16),
            masks_pool: Vec::with_capacity(MASKS_POOL_INIT_CAP),
        }
    }
}

thread_local! {
    /// Pool of reusable [`String`] buffers, one per thread, to avoid repeated allocation during
    /// text transformation. Bounded to [`STRING_POOL_MAX`] entries between calls.
    static STRING_POOL: RefCell<Vec<String>> = RefCell::new(Vec::with_capacity(STRING_POOL_INIT_CAP));
    /// Combined per-thread traversal state for [`walk_process_tree`]: the trie-node index map
    /// and the [`ProcessedTextMasks`] pool, merged into one TLS slot to save a lookup.
    static TRANSFORM_STATE: RefCell<TransformThreadState> = RefCell::new(TransformThreadState::new());
}

/// Pops a reusable [`String`] from the thread-local pool, or allocates a new one.
///
/// The requested `capacity` is treated as a lower bound; a recycled string is reserved
/// upward if needed so callers can append without repeated growth.
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
///
/// The pool is intentionally bounded: large bursts can allocate temporarily, but only the
/// hottest strings are retained to keep thread-local memory usage predictable.
fn return_string_to_pool(s: String) {
    STRING_POOL.with(|pool| {
        let mut pool = pool.borrow_mut();
        if pool.len() < STRING_POOL_MAX {
            pool.push(s);
        }
    });
}

/// Drains a [`ProcessedTextMasks`] collection and returns all owned strings to the pool.
///
/// This is only needed inside `matcher_rs`, where traversal output is frequently recycled
/// between calls. External users of [`walk_process_tree`] can drop the returned vector.
pub(crate) fn return_processed_string_to_pool(mut text_masks: ProcessedTextMasks) {
    for (cow, _, _) in text_masks.drain(..) {
        if let Cow::Owned(s) = cow {
            return_string_to_pool(s);
        }
    }
    // Safety: drain() has removed all elements, so no Cow<'_, str> borrows remain.
    // Transmuting the empty Vec's element lifetime to 'static is sound because an empty
    // Vec holds no values and the memory layout of Cow<'_, str> is lifetime-independent.
    let empty: ProcessedTextMasks<'static> = unsafe { std::mem::transmute(text_masks) };
    TRANSFORM_STATE.with(|state| {
        let mut state = state.borrow_mut();
        if state.masks_pool.len() < MASKS_POOL_MAX {
            state.masks_pool.push(empty);
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
    ///
    /// // Including `None` keeps the raw-text path alongside transformed ones.
    /// let raw_and_deleted = ProcessType::None | ProcessType::Delete;
    /// assert!(raw_and_deleted.contains(ProcessType::None));
    /// assert!(raw_and_deleted.contains(ProcessType::Delete));
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
        let names = self
            .iter_names()
            .map(|(name, _)| name.to_lowercase())
            .collect::<Vec<_>>();
        write!(f, "{}", names.join("_"))
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

/// A collection of `(processed_text, bitmask)` pairs produced by
/// the `reduce_text_process_*` family of functions.
///
/// Each entry pairs a text variant with a bitmask of the [`ProcessType`] flags that
/// produced it. Up to 16 distinct variants are possible given the number of flags.
///
/// # Type Parameters
/// * `'a` - The lifetime of the underlying string slice.
pub type ProcessedTextMasks<'a> = Vec<(Cow<'a, str>, u64, bool)>;

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
pub(crate) enum ProcessMatcher {
    MultiChar(MultiCharMatcher),
    SingleChar(SingleCharMatcher),
}

impl ProcessMatcher {
    /// Generic scan-and-replace engine underlying both [`replace_all`](Self::replace_all) and
    /// [`delete_all`](Self::delete_all).
    ///
    /// Iterates over non-overlapping match spans from `iter` and builds a new string by
    /// copying the gaps between spans verbatim and calling `push_replacement` to emit the
    /// substitution for each span.
    ///
    /// # Type Parameters
    /// * `I` — an iterator that yields `(start_byte, end_byte, match_data)` tuples for each
    ///   matched span (non-overlapping, in order).
    /// * `M` — the match payload forwarded to `push_replacement` (e.g. a replacement `char`,
    ///   `&str`, or a `usize` index into a replacement list).
    /// * `F` — a closure `FnMut(&mut String, M)` that writes the replacement for one span.
    ///
    /// Returns `(true, Cow::Owned(result))` when at least one span was replaced, or
    /// `(false, Cow::Borrowed(text))` when `iter` yielded no matches (zero allocations).
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
    pub(crate) fn replace_all<'a>(&self, text: &'a str) -> (bool, Cow<'a, str>) {
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
                let replacements = mc.replace_list();
                Self::replace_scan(text, mc.find_iter(text), |result, idx| {
                    result.push_str(replacements[idx]);
                })
            }
        }
    }

    /// Removes all matched patterns from `text`.
    ///
    /// Returns `(true, Cow::Owned(result))` when at least one character or span was removed, or
    /// `(false, Cow::Borrowed(text))` when nothing matched, avoiding any allocation.
    #[inline(always)]
    pub(crate) fn delete_all<'a>(&self, text: &'a str) -> (bool, Cow<'a, str>) {
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
/// This is the "final result only" helper: intermediate variants are discarded.
///
/// For use cases where multiple composite types share common prefixes, prefer
/// [`walk_process_tree`] which avoids redundant intermediate computations.
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
                if let (true, Cow::Owned(processed)) = pm.delete_all(result.as_ref())
                    && let Cow::Owned(old) = std::mem::replace(&mut result, Cow::Owned(processed))
                {
                    return_string_to_pool(old);
                }
            }
            _ => {
                if let (true, Cow::Owned(processed)) = pm.replace_all(result.as_ref())
                    && let Cow::Owned(old) = std::mem::replace(&mut result, Cow::Owned(processed))
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
/// Use this when you want a step-by-step view of the pipeline for a single composite type.
///
/// For generating all variants needed for matching, prefer [`walk_process_tree`].
///
/// # Examples
///
/// ```rust
/// use matcher_rs::{ProcessType, reduce_text_process};
///
/// let variants = reduce_text_process(ProcessType::FanjianDeleteNormalize, "~ᗩ~躶~𝚩~軆~Ⲉ~");
///
/// assert_eq!(variants.len(), 4);
/// assert_eq!(variants[0], "~ᗩ~躶~𝚩~軆~Ⲉ~");
/// assert_eq!(variants[1], "~ᗩ~裸~𝚩~軆~Ⲉ~");
/// assert_eq!(variants[2], "ᗩ裸𝚩軆Ⲉ");
/// assert_eq!(variants[3], "a裸b軆c");
/// ```
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
            ProcessType::None => continue,
            ProcessType::Delete => {
                if let (true, Cow::Owned(processed)) = pm.delete_all(current_text.as_ref()) {
                    text_list.push(Cow::Owned(processed));
                }
            }
            _ => {
                if let (true, Cow::Owned(processed)) = pm.replace_all(current_text.as_ref()) {
                    text_list.push(Cow::Owned(processed));
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
/// The returned variants correspond to distinct strings that may need to be indexed,
/// not every intermediate step that happened along the way.
///
/// # Examples
///
/// ```rust
/// use matcher_rs::{ProcessType, reduce_text_process_emit};
///
/// let variants = reduce_text_process_emit(ProcessType::FanjianDeleteNormalize, "~ᗩ~躶~𝚩~軆~Ⲉ~");
///
/// assert_eq!(variants.len(), 2);
/// assert_eq!(variants[0], "~ᗩ~裸~𝚩~軆~Ⲉ~");
/// assert_eq!(variants[1], "a裸b軆c");
/// ```
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
            ProcessType::None => continue,
            ProcessType::Delete => {
                if let (true, Cow::Owned(processed)) = pm.delete_all(current_text.as_ref()) {
                    text_list.push(Cow::Owned(processed));
                }
            }
            _ => {
                if let (true, Cow::Owned(processed)) = pm.replace_all(current_text.as_ref()) {
                    *current_text = Cow::Owned(processed);
                }
            }
        }
    }

    text_list
}

/// A node in the flat-array transformation trie used by [`walk_process_tree`].
///
/// The trie is built once by [`build_process_type_tree`] and stored inside `SimpleMatcher`.
/// Each node represents a single transformation step (`process_type_bit`) reachable from
/// its parent. The `process_type_list` records which composite [`ProcessType`] values
/// "arrive" at this node (i.e. terminate here), so the traversal can tag output text
/// variants with the correct bitmask. `children` holds flat-array indices of the next steps.
/// `folded_mask` is the pre-computed OR of `1u64 << pt.bits()` for all entries in
/// `process_type_list`, avoiding the per-call fold in the hot traversal loop.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ProcessTypeBitNode {
    /// The composite [`ProcessType`] values whose decomposed bit-path terminates at this node.
    /// A non-empty list means that one or more rules emit a text variant here.
    process_type_list: Vec<ProcessType>,
    /// The single-bit [`ProcessType`] step that this node represents (i.e., the edge label
    /// from the parent). For the root node this is [`ProcessType::None`].
    process_type_bit: ProcessType,
    /// Flat-array indices of child nodes (the next transformation steps reachable from here).
    children: Vec<usize>,
    /// Pre-computed OR of `1u64 << pt.bits()` for every `pt` in `process_type_list`.
    /// Avoids a per-traversal fold in the hot [`walk_process_tree`] loop.
    folded_mask: u64,
}

impl ProcessTypeBitNode {
    /// Re-encodes `folded_mask` using a sequential index table.
    ///
    /// The default encoding stores `1u64 << pt.bits()`, which can use bits up to 63 for
    /// composite [`ProcessType`] values. A sequential index keeps bit positions small (0..N
    /// where N is the number of unique composite types) so [`PatternEntry`] can store the
    /// index as a `u8` rather than a `u64`, halving the entry size.
    ///
    /// `pt_index_table[pt.bits()]` must contain the sequential index for every composite
    /// [`ProcessType`] that terminates at any node (i.e. every type in the original
    /// `process_type_set` plus [`ProcessType::None`]).
    pub(crate) fn recompute_mask_with_index(&mut self, pt_index_table: &[u8; 64]) {
        self.folded_mask = self.process_type_list.iter().fold(0u64, |acc, pt| {
            acc | (1u64 << pt_index_table[pt.bits() as usize])
        });
    }
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
/// [`walk_process_tree`], which performs a single BFS traversal to compute all
/// needed text variants while sharing common intermediate results.
pub fn build_process_type_tree(process_type_set: &HashSet<ProcessType>) -> Vec<ProcessTypeBitNode> {
    let mut process_type_tree = Vec::new();
    let mut root = ProcessTypeBitNode {
        process_type_list: Vec::new(),
        process_type_bit: ProcessType::None,
        children: Vec::new(),
        folded_mask: 0,
    };
    if process_type_set.contains(&ProcessType::None) {
        root.process_type_list.push(ProcessType::None);
        root.folded_mask |= 1u64 << ProcessType::None.bits();
    }
    process_type_tree.push(root);
    for &process_type in process_type_set.iter() {
        let mut current_node_index = 0;
        for process_type_bit in process_type.iter() {
            let current_node = &process_type_tree[current_node_index];
            if current_node.process_type_bit == process_type_bit {
                continue;
            }

            let found_child = current_node
                .children
                .iter()
                .find(|&&idx| process_type_tree[idx].process_type_bit == process_type_bit)
                .copied();

            if let Some(child_idx) = found_child {
                current_node_index = child_idx;
                process_type_tree[current_node_index]
                    .process_type_list
                    .push(process_type);
                process_type_tree[current_node_index].folded_mask |= 1u64 << process_type.bits();
            } else {
                let mut child = ProcessTypeBitNode {
                    process_type_list: Vec::new(),
                    process_type_bit,
                    children: Vec::new(),
                    folded_mask: 0,
                };
                child.process_type_list.push(process_type);
                child.folded_mask |= 1u64 << process_type.bits();
                process_type_tree.push(child);
                let new_node_index = process_type_tree.len() - 1;
                process_type_tree[current_node_index]
                    .children
                    .push(new_node_index);
                current_node_index = new_node_index;
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
///
/// `is_ascii` indicates whether `processed` contains only ASCII bytes; stored alongside
/// the text so callers can skip the charwise automaton without a redundant byte scan.
///
/// This keeps `walk_process_tree` in "unique string" space even when different trie paths
/// converge onto the same transformed text.
#[inline(always)]
fn dedup_insert(
    text_masks: &mut ProcessedTextMasks<'_>,
    current_index: usize,
    changed: Option<String>,
    is_ascii: bool,
) -> usize {
    match changed {
        Some(processed) => {
            if let Some(pos) = text_masks
                .iter()
                .position(|(t, _, _)| t.as_ref() == processed.as_str())
            {
                return_string_to_pool(processed);
                pos
            } else {
                text_masks.push((Cow::Owned(processed), 0u64, is_ascii));
                text_masks.len() - 1
            }
        }
        None => current_index,
    }
}

/// Walks the transformation trie, producing all text variants needed for matching.
///
/// This is the hot-path function called on every [`crate::SimpleMatcher::is_match`] /
/// [`crate::SimpleMatcher::process`] invocation. It performs a single left-to-right BFS traversal
/// of `process_type_tree`, applying each transformation step exactly once per unique path. Common
/// prefixes (e.g. the shared `Fanjian` step for both `Fanjian | Delete` and `Fanjian | Normalize`)
/// are computed only once and their result indices are reused for child nodes.
///
/// Each entry in the returned `Vec<(Cow<str>, u64, bool)>` is a
/// `(text_variant, rule_bitmask, is_ascii)` triple.
/// The bitmask encodes which composite [`ProcessType`]s produced that variant. `is_ascii` is
/// `true` when the text variant contains only ASCII bytes; callers can use it to skip the
/// charwise automaton without a redundant byte scan.
///
/// When `LAZY=true`, `on_variant(text, index, mask, is_ascii)` is called immediately after each
/// new unique variant is produced. If it returns `true`, the walk stops early. A delta phase at
/// the end re-invokes `on_variant` for any entry whose mask grew through dedup after its initial
/// callback. When `LAZY=false`, `on_variant` is never called and all lazy-only code is
/// dead-code-eliminated.
///
/// Returns `(text_masks, stopped)` where `stopped` is `true` only when `LAZY=true` and
/// `on_variant` triggered early exit. Inside `matcher_rs`, owned strings are usually returned
/// to a thread-local pool after use; external callers can simply let the returned vector drop.
///
/// # Examples
///
/// ```rust
/// use std::collections::HashSet;
///
/// use matcher_rs::{ProcessType, build_process_type_tree, walk_process_tree};
///
/// let process_types = HashSet::from([
///     ProcessType::None,
///     ProcessType::Fanjian,
///     ProcessType::Delete,
///     ProcessType::Fanjian | ProcessType::Delete,
/// ]);
/// let tree = build_process_type_tree(&process_types);
///
/// let (variants, stopped) = walk_process_tree::<false, _>(&tree, "妳！好", &mut |_, _, _, _| false);
/// assert!(!stopped);
///
/// let texts = variants
///     .into_iter()
///     .map(|(text, _, _)| text.into_owned())
///     .collect::<std::collections::HashSet<_>>();
///
/// assert_eq!(texts.len(), 4);
/// assert!(texts.contains("妳！好"));
/// assert!(texts.contains("你！好"));
/// assert!(texts.contains("妳好"));
/// assert!(texts.contains("你好"));
/// ```
#[inline(always)]
pub fn walk_process_tree<'a, const LAZY: bool, F>(
    process_type_tree: &[ProcessTypeBitNode],
    text: &'a str,
    on_variant: &mut F,
) -> (ProcessedTextMasks<'a>, bool)
where
    F: FnMut(&str, usize, u64, bool) -> bool,
{
    TRANSFORM_STATE.with(|state| {
        let mut ts = state.borrow_mut();

        let pooled: Option<ProcessedTextMasks<'static>> = ts.masks_pool.pop();
        // Safety: pool holds empty Vecs with no live borrows; transmuting from
        // 'static to 'a is safe since 'static: 'a (covariant) and Vec is empty.
        let mut text_masks: ProcessedTextMasks<'a> =
            unsafe { std::mem::transmute(pooled.unwrap_or_default()) };
        text_masks.clear();
        let root_is_ascii = text.is_ascii();
        text_masks.push((
            Cow::Borrowed(text),
            process_type_tree[0].folded_mask,
            root_is_ascii,
        ));

        let mut scanned_masks: TinyVec<[u64; 8]> = TinyVec::new();
        if LAZY {
            scanned_masks.push(0u64);
            let root_mask = process_type_tree[0].folded_mask;
            if root_mask != 0 && on_variant(text, 0, root_mask, root_is_ascii) {
                return (text_masks, true);
            }
            scanned_masks[0] = root_mask;
        }

        if process_type_tree[0].children.is_empty() {
            return (text_masks, false);
        }

        ts.tree_node_indices.clear();
        ts.tree_node_indices.resize(process_type_tree.len(), 0);

        let mut stopped = false;

        'walk: for (current_node_index, current_node) in process_type_tree.iter().enumerate() {
            let current_index = ts.tree_node_indices[current_node_index];
            let parent_is_ascii = text_masks[current_index].2;

            for &child_node_index in &current_node.children {
                let child_node = &process_type_tree[child_node_index];
                let pm = get_process_matcher(child_node.process_type_bit);

                // Compute (changed_text, is_ascii_of_result) for this transformation step.
                // - PinYin/PinYinChar output is always ASCII (romanized).
                // - Fanjian maps CJK→CJK: if changed, result is non-ASCII.
                // - Delete can only remove chars: if parent is ASCII, result is still ASCII;
                //   if parent is non-ASCII, check the shorter result directly.
                // - Normalize: check the result string.
                // - None: no transformation, inherit parent.
                // When unchanged (None returned), child inherits parent's flag (O(1)).
                let (changed, child_is_ascii) = match child_node.process_type_bit {
                    ProcessType::None => (None, parent_is_ascii),
                    ProcessType::PinYin | ProcessType::PinYinChar => {
                        let current_text = text_masks[current_index].0.as_ref();
                        match pm.replace_all(current_text) {
                            (true, Cow::Owned(processed)) => (Some(processed), true),
                            _ => (None, parent_is_ascii),
                        }
                    }
                    ProcessType::Fanjian => {
                        let current_text = text_masks[current_index].0.as_ref();
                        match pm.replace_all(current_text) {
                            (true, Cow::Owned(processed)) => (Some(processed), false),
                            _ => (None, parent_is_ascii),
                        }
                    }
                    ProcessType::Delete => {
                        let current_text = text_masks[current_index].0.as_ref();
                        match pm.delete_all(current_text) {
                            (true, Cow::Owned(processed)) => {
                                // If parent was ASCII, result is still ASCII (only ASCII chars removed).
                                // If parent was non-ASCII, some non-ASCII may have been deleted —
                                // check the shorter result string.
                                let ia = parent_is_ascii || processed.is_ascii();
                                (Some(processed), ia)
                            }
                            _ => (None, parent_is_ascii),
                        }
                    }
                    _ => {
                        let current_text = text_masks[current_index].0.as_ref();
                        match pm.replace_all(current_text) {
                            (true, Cow::Owned(processed)) => {
                                let ia = processed.is_ascii();
                                (Some(processed), ia)
                            }
                            _ => (None, parent_is_ascii),
                        }
                    }
                };

                let old_len = if LAZY { text_masks.len() } else { 0 };
                let child_index =
                    dedup_insert(&mut text_masks, current_index, changed, child_is_ascii);

                if LAZY {
                    while scanned_masks.len() < text_masks.len() {
                        scanned_masks.push(0u64);
                    }
                }

                ts.tree_node_indices[child_node_index] = child_index;
                text_masks[child_index].1 |= child_node.folded_mask;

                if LAZY && child_index >= old_len {
                    // New unique text: call on_variant immediately.
                    let mask = text_masks[child_index].1;
                    let is_ascii = text_masks[child_index].2;
                    if mask != 0
                        && on_variant(
                            text_masks[child_index].0.as_ref(),
                            child_index,
                            mask,
                            is_ascii,
                        )
                    {
                        stopped = true;
                        break 'walk;
                    }
                    scanned_masks[child_index] = mask;
                }
                // Dedup'd entry: mask may have grown; handled in delta phase below.
            }
        }

        if LAZY {
            if stopped {
                return (text_masks, true);
            }
            // Delta phase: re-scan entries whose mask grew after their initial callback.
            for i in 0..text_masks.len() {
                let delta = text_masks[i].1 & !scanned_masks[i];
                if delta != 0 && on_variant(text_masks[i].0.as_ref(), i, delta, text_masks[i].2) {
                    return (text_masks, true);
                }
            }
        }

        (text_masks, false)
    })
}
