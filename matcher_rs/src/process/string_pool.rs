//! Thread-local string pool and text variant types for the transformation pipeline.
//!
//! [`TextVariant`] and [`ProcessedTextMasks`] are the output types of
//! [`super::process_tree::walk_process_tree`]. The string pool and [`TransformThreadState`]
//! reduce per-call allocation overhead by recycling buffers across invocations.

use std::borrow::Cow;
use std::cell::RefCell;

/// Initial capacity of the per-thread `String` pool (number of pre-allocated slots).
const STRING_POOL_INIT_CAP: usize = 16;
/// Initial capacity of the per-thread `ProcessedTextMasks` pool (number of pre-allocated slots).
const MASKS_POOL_INIT_CAP: usize = 4;
/// Maximum number of `String` buffers retained in the pool between calls; excess are dropped.
const STRING_POOL_MAX: usize = 128;
/// Maximum number of `ProcessedTextMasks` buffers retained in the pool between calls; excess are dropped.
const MASKS_POOL_MAX: usize = 16;

/// A text variant produced by the transformation pipeline, paired with metadata for matching.
#[derive(Clone)]
pub struct TextVariant<'a> {
    /// The transformed string for this variant.
    pub text: Cow<'a, str>,
    /// Bitmask of sequential [`crate::ProcessType`] indices that produced this variant;
    /// used by the matcher to filter which rules are eligible for this text.
    pub mask: u64,
    /// Whether `text` is entirely ASCII; callers use this to skip the charwise automaton
    /// without a redundant byte scan.
    pub is_ascii: bool,
}

/// All text variants produced for a single input by the transformation pipeline.
///
/// The number of distinct variants depends on the active [`ProcessType`](crate::ProcessType)
/// configuration and how many intermediate results are shared or deduplicated.
pub type ProcessedTextMasks<'a> = Vec<TextVariant<'a>>;

/// Combined thread-local state for `tree_node_indices` and `masks_pool`.
///
/// Merging into a single `thread_local!` eliminates one TLS lookup (~5ns) per
/// `walk_process_tree` call.
pub(crate) struct TransformThreadState {
    /// Maps trie node index → text variant index; resized at the start of each
    /// [`super::process_tree::walk_process_tree`] call.
    pub(crate) tree_node_indices: Vec<usize>,
    /// Recycled empty [`ProcessedTextMasks`] buffers; bounded by `MASKS_POOL_MAX`.
    pub(crate) masks_pool: Vec<ProcessedTextMasks<'static>>,
}

impl TransformThreadState {
    /// Creates empty reusable traversal state for `walk_process_tree`.
    ///
    /// `tree_node_indices` is resized per traversal to map trie node index → text variant
    /// index, while `masks_pool` stores emptied `ProcessedTextMasks` buffers for reuse.
    pub(crate) fn new() -> Self {
        Self {
            tree_node_indices: Vec::with_capacity(16),
            masks_pool: Vec::with_capacity(MASKS_POOL_INIT_CAP),
        }
    }
}

thread_local! {
    /// Pool of reusable [`String`] buffers, one per thread, to avoid repeated allocation during
    /// text transformation. Bounded to [`STRING_POOL_MAX`] entries between calls.
    pub(crate) static STRING_POOL: RefCell<Vec<String>> = RefCell::new(Vec::with_capacity(STRING_POOL_INIT_CAP));
    /// Combined per-thread traversal state for [`walk_process_tree`]: the trie-node index map
    /// and the [`ProcessedTextMasks`] pool, merged into one TLS slot to save a lookup.
    pub(crate) static TRANSFORM_STATE: RefCell<TransformThreadState> = RefCell::new(TransformThreadState::new());
}

/// Pops a reusable [`String`] from the thread-local pool, or allocates a new one.
///
/// The requested `capacity` is treated as a lower bound; a recycled string is reserved
/// upward if needed so callers can append without repeated growth.
pub(crate) fn get_string_from_pool(capacity: usize) -> String {
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
pub(crate) fn return_string_to_pool(s: String) {
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
/// between calls. External users of [`crate::walk_process_tree`] can drop the returned vector.
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
    TRANSFORM_STATE.with(|state| {
        let mut state = state.borrow_mut();
        if state.masks_pool.len() < MASKS_POOL_MAX {
            state.masks_pool.push(empty);
        }
    });
}
