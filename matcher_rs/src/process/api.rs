//! Public processing helpers built on top of the step registry.
//!
//! These are the standalone entry points for the transformation pipeline. They decompose
//! a composite [`ProcessType`] into single-bit steps, apply each in order via the global
//! [`super::step`] registry, and return the results as borrowed or owned [`Cow<str>`] values.

use std::borrow::Cow;

use crate::process::process_type::ProcessType;
use crate::process::step::get_transform_step;
use crate::process::string_pool::return_string_to_pool;
use crate::process::transform::simd::multibyte_density;

/// Replaces the current owned value in a [`Cow`] and returns the old allocation to the pool.
///
/// Borrowed values are replaced without any pool interaction. Owned values are swapped out
/// and recycled so chained transformations do not leak short-lived buffers.
#[inline(always)]
fn replace_cow<'a>(current: &mut Cow<'a, str>, next: String) {
    if let Cow::Owned(old) = std::mem::replace(current, Cow::Owned(next)) {
        return_string_to_pool(old);
    }
}

/// Shared implementation for the public reduction helpers.
///
/// When `overwrite_replace` is `false`, every changed step appends a new entry. When it is
/// `true`, replace-style steps overwrite the last entry in place while `Delete` still appends,
/// preserving the emitted-variant semantics used by matcher construction.
///
/// # Panics
///
/// Panics (via `.expect()`) if `text_list` is somehow emptied during iteration, which
/// cannot happen since the original input is always the first entry.
fn reduce_text_process_inner<'a>(
    process_type: ProcessType,
    text: &'a str,
    overwrite_replace: bool,
) -> Vec<Cow<'a, str>> {
    let mut text_list = vec![Cow::Borrowed(text)];

    for process_type_bit in process_type.iter() {
        let current = text_list
            .last_mut()
            .expect("text_list is never empty (seeded with original text)");
        let output = get_transform_step(process_type_bit)
            .apply(current.as_ref(), multibyte_density(current.as_bytes()));

        if let Some(changed) = output.changed {
            if overwrite_replace && process_type_bit != ProcessType::Delete {
                replace_cow(current, changed);
            } else {
                text_list.push(Cow::Owned(changed));
            }
        }
    }

    text_list
}

/// Applies a composite [`ProcessType`] pipeline to `text` and returns the final result.
///
/// Steps run in [`ProcessType::iter`] order (ascending bit position). If no step changes
/// the text, the return value borrows directly from `text` (zero allocation). When one or
/// more steps produce changes, intermediate allocations are recycled through the
/// thread-local string pool so only the final result is returned as `Cow::Owned`.
///
/// This function is best for one-shot use.
///
/// # Examples
///
/// ```rust
/// use matcher_rs::{ProcessType, text_process};
///
/// // Fanjian converts Traditional→Simplified; Delete removes punctuation.
/// let processed = text_process(ProcessType::Fanjian | ProcessType::Delete, "測！試");
/// assert_eq!(processed, "测试");
///
/// // No-op when the text has nothing to transform.
/// let unchanged = text_process(ProcessType::Fanjian, "hello");
/// assert_eq!(unchanged, "hello");
/// // Borrowed — no allocation occurred.
/// assert!(matches!(unchanged, std::borrow::Cow::Borrowed(_)));
/// ```
#[inline(always)]
pub fn text_process<'a>(process_type: ProcessType, text: &'a str) -> Cow<'a, str> {
    let mut result = Cow::Borrowed(text);

    for process_type_bit in process_type.iter() {
        let output = get_transform_step(process_type_bit)
            .apply(result.as_ref(), multibyte_density(result.as_bytes()));
        if let Some(changed) = output.changed {
            replace_cow(&mut result, changed);
        }
    }

    result
}

/// Applies a composite [`ProcessType`] pipeline to `text`, recording every intermediate change.
///
/// Returns a `Vec` whose first element is always the original `text` (borrowed). Each
/// subsequent element is the output of a step that actually changed the text; steps that
/// leave the text unchanged are skipped. The final element is therefore the fully
/// transformed result.
///
/// This is useful for inspecting how each stage transforms the input, or for collecting
/// all intermediate forms that should be indexed.
///
/// # Examples
///
/// ```rust
/// use matcher_rs::{ProcessType, reduce_text_process};
///
/// // FanjianDeleteNormalize = Fanjian | Delete | Normalize, applied in that order.
/// let variants = reduce_text_process(ProcessType::FanjianDeleteNormalize, "~測~Ａ~");
/// // First entry is always the original input.
/// assert_eq!(variants[0], "~測~Ａ~");
/// // Last entry is the fully transformed result.
/// assert_eq!(variants.last().unwrap(), "测a");
/// ```
#[inline(always)]
pub fn reduce_text_process<'a>(process_type: ProcessType, text: &'a str) -> Vec<Cow<'a, str>> {
    reduce_text_process_inner(process_type, text, false)
}

/// Like [`reduce_text_process`], but merges replace-type steps in-place.
///
/// This variant is used during matcher construction to keep only the strings that the
/// Aho-Corasick automaton will actually scan at match time. Replace-style steps
/// (Fanjian, Normalize, PinYin, PinYinChar) overwrite the last entry rather than
/// appending, because the pre-replacement form is never scanned separately. Delete steps
/// still append because deletion changes which character sequences are adjacent, affecting
/// which patterns can match.
///
/// The result therefore contains fewer entries than [`reduce_text_process`]: one entry per
/// "scan boundary" rather than one per transformation step.
///
/// # Examples
///
/// ```rust
/// use matcher_rs::{ProcessType, reduce_text_process_emit};
///
/// // FanjianDeleteNormalize = Fanjian | Delete | Normalize.
/// let variants = reduce_text_process_emit(ProcessType::FanjianDeleteNormalize, "~測~Ａ~");
/// // Only two entries: Fanjian overwrites the original, then Delete appends.
/// // The Normalize step overwrites the Delete entry in-place.
/// assert_eq!(variants.len(), 2);
/// assert_eq!(variants[0], "~测~Ａ~");  // after Fanjian (replace, overwrites original)
/// assert_eq!(variants[1], "测a");       // after Delete+Normalize
/// ```
#[inline(always)]
pub fn reduce_text_process_emit<'a>(process_type: ProcessType, text: &'a str) -> Vec<Cow<'a, str>> {
    reduce_text_process_inner(process_type, text, true)
}
