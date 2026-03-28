//! Thread-local storage and text variant types for the transformation pipeline.
//!
//! [`TextVariant`] and [`ProcessedTextMasks`] are the output types of
//! [`super::graph::walk_process_tree`]. The string pool ([`STRING_POOL`]) and combined
//! traversal state ([`TRANSFORM_STATE`]) reduce allocation churn by recycling buffers
//! across matcher calls within each thread.
//!
//! # Safety model
//!
//! Both thread-local statics use `UnsafeCell` with `#[thread_local]` (a nightly feature)
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
/// Maximum number of [`ProcessedTextMasks`] buffers retained in the pool between calls; excess are dropped.
const MASKS_POOL_MAX: usize = 16;

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
/// let (variants, _) = walk_process_tree::<false, _>(&tree, "hello", &mut |_, _, _, _| false);
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
/// let (masks, _): (ProcessedTextMasks<'_>, _) =
///     walk_process_tree::<false, _>(&tree, "妳好", &mut |_, _, _, _| false);
///
/// // At least two variants: original + Fanjian-converted.
/// assert!(masks.len() >= 2);
/// ```
pub type ProcessedTextMasks<'a> = Vec<TextVariant<'a>>;

/// Combined thread-local state for tree-walk scratch data and the masks buffer pool.
///
/// Keeping both in a single `#[thread_local]` static avoids a second TLS lookup on every
/// [`walk_process_tree`](super::walk_process_tree) call.
pub(crate) struct TransformThreadState {
    /// Maps trie node index to the index of its text variant in the output
    /// [`ProcessedTextMasks`].
    ///
    /// Resized to `process_type_tree.len()` at the start of each
    /// [`walk_process_tree`](super::walk_process_tree) call.
    pub(crate) tree_node_indices: Vec<usize>,
    /// Recycled empty [`ProcessedTextMasks`] buffers, bounded by [`MASKS_POOL_MAX`].
    ///
    /// The `'static` lifetime is sound because every buffer in this pool has been drained
    /// (no live `Cow` borrows remain) before being pushed here. See
    /// [`return_processed_string_to_pool`] for the transmute rationale.
    pub(crate) masks_pool: Vec<ProcessedTextMasks<'static>>,
}

impl TransformThreadState {
    /// Creates empty traversal state.
    ///
    /// `const`-compatible for `#[thread_local]` initialization; capacity grows on first use.
    pub(crate) const fn new() -> Self {
        Self {
            tree_node_indices: Vec::new(),
            masks_pool: Vec::new(),
        }
    }
}

/// Pool of reusable [`String`] buffers, one per thread.
///
/// Avoids repeated allocation during text transformation. Bounded to [`STRING_POOL_MAX`]
/// entries between calls; excess strings are dropped.
///
/// # Safety
///
/// Uses `#[thread_local]` + `UnsafeCell` to eliminate the `thread_local!` macro's
/// `.with()` closure overhead. Single-threaded access is guaranteed by the
/// `#[thread_local]` attribute. No function in this module is re-entrant while the
/// mutable reference from `UnsafeCell::get()` is live.
#[thread_local]
pub(crate) static STRING_POOL: UnsafeCell<Vec<String>> = UnsafeCell::new(Vec::new());

/// Combined per-thread traversal state for [`walk_process_tree`](super::walk_process_tree).
///
/// Merges the trie-node-to-text-index map and the [`ProcessedTextMasks`] buffer pool into
/// one TLS slot to save a lookup on every matcher call.
///
/// # Safety
///
/// Same invariants as [`STRING_POOL`]: `#[thread_local]` guarantees single-threaded access,
/// and no function re-enters this static while a mutable reference is live.
#[thread_local]
pub(crate) static TRANSFORM_STATE: UnsafeCell<TransformThreadState> =
    UnsafeCell::new(TransformThreadState::new());

/// Pops a reusable [`String`] from the thread-local pool, or allocates a new one.
///
/// The requested `capacity` is treated as a lower bound; a recycled string is reserved
/// upward if needed so callers can append without repeated growth.
///
/// # Safety
///
/// Accesses [`STRING_POOL`] through `UnsafeCell::get()`. This is safe because:
/// - `#[thread_local]` guarantees no concurrent access from other threads.
/// - No caller holds a mutable reference to the pool when this function is entered
///   (the pool functions are not re-entrant).
pub(crate) fn get_string_from_pool(capacity: usize) -> String {
    // SAFETY: #[thread_local] guarantees single-threaded access; no re-entrant calls
    // into this function while the mutable reference is live.
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
///
/// The pool is intentionally bounded to [`STRING_POOL_MAX`]: large bursts can allocate
/// temporarily, but only the hottest buffers are retained to keep per-thread memory
/// usage predictable.
///
/// # Safety
///
/// Same safety model as [`get_string_from_pool`] — single-threaded, non-re-entrant
/// access to [`STRING_POOL`].
pub(crate) fn return_string_to_pool(s: String) {
    // SAFETY: #[thread_local] guarantees single-threaded access; no re-entrant calls.
    let pool = unsafe { &mut *STRING_POOL.get() };
    if pool.len() < STRING_POOL_MAX {
        pool.push(s);
    }
}

/// Drains a [`ProcessedTextMasks`] collection, returns all owned strings to the string
/// pool, and stashes the emptied `Vec` in the masks pool for reuse.
///
/// This is used internally by [`crate::SimpleMatcher`] to recycle traversal output
/// between calls. External users of [`crate::walk_process_tree`] can simply drop the
/// returned vector — no manual recycling is needed.
///
/// # Safety
///
/// Contains two `unsafe` blocks:
///
/// 1. **`transmute` of the empty `Vec`** — After `drain()`, the `Vec` holds zero
///    elements, so no `Cow<'_, str>` borrows exist. Transmuting `Vec<TextVariant<'_>>`
///    to `Vec<TextVariant<'static>>` is sound because an empty `Vec` stores no values
///    and `Cow<'_, str>` has identical layout regardless of lifetime.
///
/// 2. **`TRANSFORM_STATE.get()`** — Same TLS safety model as the string pool functions:
///    `#[thread_local]` guarantees single-threaded access, and no caller holds a mutable
///    reference when this function is entered.
pub(crate) fn return_processed_string_to_pool(mut text_masks: ProcessedTextMasks) {
    for TextVariant { text: cow, .. } in text_masks.drain(..) {
        if let Cow::Owned(s) = cow {
            return_string_to_pool(s);
        }
    }
    // Safety: drain() has removed all elements, so no Cow<'_, str> borrows remain.
    // Transmuting the empty Vec's element lifetime to 'static is sound because an empty
    // Vec holds no values and the memory layout of Cow<'_, str> is lifetime-independent.
    let empty: ProcessedTextMasks<'static> = unsafe { std::mem::transmute(text_masks) };
    // SAFETY: #[thread_local] guarantees single-threaded access; no re-entrant calls.
    let state = unsafe { &mut *TRANSFORM_STATE.get() };
    if state.masks_pool.len() < MASKS_POOL_MAX {
        state.masks_pool.push(empty);
    }
}
