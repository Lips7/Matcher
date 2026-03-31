//! Thread-local storage and text variant types for the transformation pipeline.
//!
//! [`TextVariant`] and [`ProcessedTextMasks`] are the output types of
//! [`super::graph::walk_process_tree`]. The string pool ([`STRING_POOL`]) and traversal
//! state ([`TRANSFORM_STATE`]) reduce allocation churn by recycling buffers across
//! matcher calls within each thread.
//!
//! # Safety model
//!
//! Thread-local statics use `UnsafeCell` with `#[thread_local]` (a nightly feature)
//! to avoid the closure overhead of the `thread_local!` macro. Safety relies on two
//! invariants:
//!
//! 1. `#[thread_local]` guarantees single-threaded access — no data races.
//! 2. No public function in this module is re-entrant: the borrow from `UnsafeCell::get()`
//!    is always dropped before any call that could re-enter the same pool.

use std::borrow::Cow;
use std::cell::UnsafeCell;

/// Maximum number of [`String`] buffers retained in the pool between calls; excess are dropped.
const STRING_POOL_MAX: usize = 128;

/// A single text variant produced by the transformation pipeline, paired with matching metadata.
///
/// [`walk_process_tree`](super::walk_process_tree) emits one `TextVariant` per unique
/// transformed string. The matcher scans each variant's `text` with the Aho-Corasick
/// automaton and uses `mask` to credit hits to the correct rules.
///
/// # Examples
///
/// ```rust
/// use std::collections::HashSet;
/// use matcher_rs::{ProcessType, TextVariant, build_process_type_tree, walk_process_tree};
///
/// let tree = build_process_type_tree(&HashSet::from([ProcessType::None]));
/// let variants = walk_process_tree(&tree, "hello");
///
/// assert_eq!(variants.len(), 1);
/// assert_eq!(variants[0].text, "hello");
/// assert!(variants[0].is_ascii);
/// ```
#[derive(Clone)]
pub struct TextVariant<'a> {
    /// The transformed string for this variant.
    ///
    /// Borrows from the original input when no transformation was applied; owned otherwise.
    pub text: Cow<'a, str>,
    /// Bitmask of sequential [`ProcessType`](crate::ProcessType) indices that produced
    /// this variant.
    ///
    /// Each set bit at position `i` means the `i`-th [`ProcessType`](crate::ProcessType)
    /// in the matcher's index table contributed this text. The matcher uses the mask to
    /// filter which rules are eligible for hits found in this variant.
    pub mask: u64,
    /// Whether `text` consists entirely of ASCII bytes.
    ///
    /// When `true`, the matcher skips the charwise (non-ASCII) Aho-Corasick automaton
    /// for this variant, avoiding a redundant scan.
    pub is_ascii: bool,
}

/// All text variants produced for a single input by the transformation pipeline.
///
/// Returned by [`walk_process_tree`](super::walk_process_tree). The number of elements
/// depends on the active [`ProcessType`](crate::ProcessType) configuration and how many
/// intermediate results are deduplicated (different trie paths that produce the same
/// string share a single entry with a merged `mask`).
///
/// # Examples
///
/// ```rust
/// use std::collections::HashSet;
/// use matcher_rs::{ProcessType, ProcessedTextMasks, build_process_type_tree, walk_process_tree};
///
/// let types = HashSet::from([ProcessType::None, ProcessType::Fanjian]);
/// let tree = build_process_type_tree(&types);
/// let masks: ProcessedTextMasks<'_> = walk_process_tree(&tree, "妳好");
///
/// // At least two variants: original + Fanjian-converted.
/// assert!(masks.len() >= 2);
/// ```
pub type ProcessedTextMasks<'a> = Vec<TextVariant<'a>>;

/// Thread-local scratch state for [`walk_process_tree`](super::walk_process_tree).
pub(crate) struct TransformThreadState {
    /// Maps trie node index to the index of its text variant in the output
    /// [`ProcessedTextMasks`].
    pub(crate) tree_node_indices: Vec<usize>,
}

impl TransformThreadState {
    pub(crate) const fn new() -> Self {
        Self {
            tree_node_indices: Vec::new(),
        }
    }
}

/// Pool of reusable [`String`] buffers, one per thread.
///
/// # Safety
///
/// Uses `#[thread_local]` + `UnsafeCell` to eliminate the `thread_local!` macro's
/// `.with()` closure overhead. Single-threaded access is guaranteed by the
/// `#[thread_local]` attribute. No function in this module is re-entrant while the
/// mutable reference from `UnsafeCell::get()` is live.
#[thread_local]
pub(crate) static STRING_POOL: UnsafeCell<Vec<String>> = UnsafeCell::new(Vec::new());

/// Per-thread traversal state for [`walk_process_tree`](super::walk_process_tree).
///
/// # Safety
///
/// Same invariants as [`STRING_POOL`].
#[thread_local]
pub(crate) static TRANSFORM_STATE: UnsafeCell<TransformThreadState> =
    UnsafeCell::new(TransformThreadState::new());

/// Pops a reusable [`String`] from the thread-local pool, or allocates a new one.
pub(crate) fn get_string_from_pool(capacity: usize) -> String {
    // SAFETY: #[thread_local] guarantees single-threaded access; non-re-entrant.
    let pool = unsafe { &mut *STRING_POOL.get() };
    if let Some(mut s) = pool.pop() {
        s.clear();
        if s.capacity() < capacity {
            s.reserve(capacity - s.capacity());
        }
        s
    } else {
        String::with_capacity(capacity)
    }
}

/// Returns a [`String`] to the thread-local pool for future reuse.
pub(crate) fn return_string_to_pool(s: String) {
    // SAFETY: #[thread_local] guarantees single-threaded access; non-re-entrant.
    let pool = unsafe { &mut *STRING_POOL.get() };
    if pool.len() < STRING_POOL_MAX {
        pool.push(s);
    }
}
