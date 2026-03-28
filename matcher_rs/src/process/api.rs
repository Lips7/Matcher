//! Public processing helpers built on top of the step registry.

use std::borrow::Cow;

use crate::process::process_type::ProcessType;
use crate::process::registry::get_transform_step;
use crate::process::variant::return_string_to_pool;

#[inline(always)]
fn replace_cow<'a>(current: &mut Cow<'a, str>, next: String) {
    if let Cow::Owned(old) = std::mem::replace(current, Cow::Owned(next)) {
        return_string_to_pool(old);
    }
}

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
        let output =
            get_transform_step(process_type_bit).apply(current.as_ref(), current.is_ascii());

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
#[inline(always)]
pub fn text_process<'a>(process_type: ProcessType, text: &'a str) -> Cow<'a, str> {
    let mut result = Cow::Borrowed(text);

    for process_type_bit in process_type.iter() {
        let output = get_transform_step(process_type_bit).apply(result.as_ref(), result.is_ascii());
        if let Some(changed) = output.changed {
            replace_cow(&mut result, changed);
        }
    }

    result
}

/// Applies a composite [`ProcessType`] pipeline to `text`, recording each changed result.
#[inline(always)]
pub fn reduce_text_process<'a>(process_type: ProcessType, text: &'a str) -> Vec<Cow<'a, str>> {
    reduce_text_process_inner(process_type, text, false)
}

/// Like [`reduce_text_process`], but composing replace-type steps in-place.
#[inline(always)]
pub fn reduce_text_process_emit<'a>(process_type: ProcessType, text: &'a str) -> Vec<Cow<'a, str>> {
    reduce_text_process_inner(process_type, text, true)
}
