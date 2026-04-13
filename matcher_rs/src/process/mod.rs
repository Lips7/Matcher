//! Text transformation pipeline for standardizing input before pattern
//! matching.
//!
//! This module converts raw input text through a configurable series of steps —
//! Traditional-to-Simplified Chinese conversion, codepoint deletion,
//! normalization, and CJK romanization — so that [`crate::SimpleMatcher`] can
//! match patterns against both raw and transformed forms of the same text.
//!
//! # Public API
//!
//! | Item | Purpose |
//! |------|---------|
//! | [`ProcessType`] | Bitflags selecting which transformation steps to apply. |
//! | [`text_process`] | Applies a composite pipeline and returns the final result. |
//! | [`reduce_text_process`] | Applies a pipeline and records each intermediate change. |
//! | [`reduce_text_process_emit`] | Like `reduce_text_process`, but merges replace-type steps in-place. |
//!
//! # Internal structure
//!
//! - [`step`] — [`TransformStep`](step::TransformStep) enum and the global
//!   `OnceLock` cache that lazily compiles each single-bit step once.
//! - [`transform`] — Low-level engines (page-table lookups for
//!   VariantNorm/Normalize/Romanize/EmojiNorm, bitset-based Delete, SIMD skip
//!   helpers).
pub(crate) mod process_type;
pub(crate) mod step;
pub(crate) mod transform;

use std::borrow::Cow;

pub use process_type::ProcessType;
use step::get_transform_step;

fn reduce_text_process_inner<'a>(
    process_type: ProcessType,
    text: &'a str,
    overwrite_replace: bool,
) -> Vec<Cow<'a, str>> {
    let mut text_list = vec![Cow::Borrowed(text)];

    for process_type_bit in process_type.iter() {
        // SAFETY invariant: text_list is seeded with `text` above and only grows.
        let Some(current) = text_list.last_mut() else {
            unreachable!()
        };
        let density = if current.is_ascii() { 1.0 } else { 0.5 };
        let changed = get_transform_step(process_type_bit).apply(current.as_ref(), density);

        if let Some((s, _)) = changed {
            if overwrite_replace && process_type_bit != ProcessType::Delete {
                *current = Cow::Owned(s);
            } else {
                text_list.push(Cow::Owned(s));
            }
        }
    }

    text_list
}

/// Applies a composite [`ProcessType`] pipeline to `text` and returns the final
/// result.
///
/// Steps run in [`ProcessType::iter`] order (ascending bit position). If no
/// step changes the text, the return value borrows directly from `text` (zero
/// allocation). When one or more steps produce changes, intermediate
/// allocations are recycled through the thread-local string pool so only the
/// final result is returned as `Cow::Owned`.
///
/// This function is best for one-shot use.
///
/// # Examples
///
/// ```rust
/// use matcher_rs::{ProcessType, text_process};
///
/// // VariantNorm normalizes CJK variants; Delete removes punctuation.
/// let processed = text_process(ProcessType::VariantNorm | ProcessType::Delete, "測！試");
/// assert_eq!(processed, "测试");
///
/// // No-op when the text has nothing to transform.
/// let unchanged = text_process(ProcessType::VariantNorm, "hello");
/// assert_eq!(unchanged, "hello");
/// // Borrowed — no allocation occurred.
/// assert!(matches!(unchanged, std::borrow::Cow::Borrowed(_)));
/// ```
#[inline(always)]
pub fn text_process<'a>(process_type: ProcessType, text: &'a str) -> Cow<'a, str> {
    let mut result = Cow::Borrowed(text);

    for process_type_bit in process_type.iter() {
        let density = if result.is_ascii() { 1.0 } else { 0.5 };
        if let Some((s, _)) = get_transform_step(process_type_bit).apply(result.as_ref(), density) {
            result = Cow::Owned(s);
        }
    }

    result
}

/// Applies a composite [`ProcessType`] pipeline to `text`, recording every
/// intermediate change.
///
/// Returns a `Vec` whose first element is always the original `text`
/// (borrowed). Each subsequent element is the output of a step that actually
/// changed the text; steps that leave the text unchanged are skipped. The final
/// element is therefore the fully transformed result.
///
/// This is useful for inspecting how each stage transforms the input, or for
/// collecting all intermediate forms that should be indexed.
///
/// # Examples
///
/// ```rust
/// use matcher_rs::{ProcessType, reduce_text_process};
///
/// // VariantNormDeleteNormalize = VariantNorm | Delete | Normalize, applied in that order.
/// let variants = reduce_text_process(ProcessType::VariantNormDeleteNormalize, "~測~Ａ~");
/// // First entry is always the original input.
/// assert_eq!(variants[0], "~測~Ａ~");
/// // Last entry is the fully transformed result.
/// assert_eq!(variants.last().unwrap(), "测a");
/// ```
#[inline]
pub fn reduce_text_process<'a>(process_type: ProcessType, text: &'a str) -> Vec<Cow<'a, str>> {
    reduce_text_process_inner(process_type, text, false)
}

/// Like [`reduce_text_process`], but merges replace-type steps in-place.
///
/// This variant is used during matcher construction to keep only the strings
/// that the Aho-Corasick automaton will actually scan at match time.
/// Replace-style steps (VariantNorm, Normalize, Romanize, RomanizeChar)
/// overwrite the last entry rather than appending, because the pre-replacement
/// form is never scanned separately. Delete steps still append because deletion
/// changes which character sequences are adjacent, affecting which patterns can
/// match.
///
/// The result therefore contains fewer entries than [`reduce_text_process`]:
/// one entry per "scan boundary" rather than one per transformation step.
///
/// # Examples
///
/// ```rust
/// use matcher_rs::{ProcessType, reduce_text_process_emit};
///
/// // VariantNormDeleteNormalize = VariantNorm | Delete | Normalize.
/// let variants = reduce_text_process_emit(ProcessType::VariantNormDeleteNormalize, "~測~Ａ~");
/// // Only two entries: VariantNorm overwrites the original, then Delete appends.
/// // The Normalize step overwrites the Delete entry in-place.
/// assert_eq!(variants.len(), 2);
/// assert_eq!(variants[0], "~测~Ａ~"); // after VariantNorm (replace, overwrites original)
/// assert_eq!(variants[1], "测a"); // after Delete+Normalize
/// ```
#[inline]
pub fn reduce_text_process_emit<'a>(process_type: ProcessType, text: &'a str) -> Vec<Cow<'a, str>> {
    reduce_text_process_inner(process_type, text, true)
}
