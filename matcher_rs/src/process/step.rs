//! Canonical execution semantics for a single transformation step.

use crate::process::transform::charwise::{FanjianMatcher, PinyinMatcher};
use crate::process::transform::delete::DeleteMatcher;
use crate::process::transform::normalize::NormalizeMatcher;

/// Result of applying one compiled pipeline step to a text variant.
pub(crate) struct StepOutput {
    pub(crate) changed: Option<String>,
    pub(crate) is_ascii: bool,
}

impl StepOutput {
    #[inline(always)]
    pub(crate) fn unchanged(is_ascii: bool) -> Self {
        Self {
            changed: None,
            is_ascii,
        }
    }

    #[inline(always)]
    pub(crate) fn changed(changed: String, is_ascii: bool) -> Self {
        Self {
            changed: Some(changed),
            is_ascii,
        }
    }
}

/// Compiled single-bit transformation step.
#[derive(Clone)]
pub(crate) enum TransformStep {
    None,
    Fanjian(FanjianMatcher),
    Delete(DeleteMatcher),
    Normalize(NormalizeMatcher),
    PinYin(PinyinMatcher),
    PinYinChar(PinyinMatcher),
}

impl TransformStep {
    /// Applies one step to `text`, returning the produced string when it changed.
    #[inline(always)]
    pub(crate) fn apply(&self, text: &str, parent_is_ascii: bool) -> StepOutput {
        match self {
            Self::None => StepOutput::unchanged(parent_is_ascii),
            Self::Fanjian(matcher) => matcher.replace(text).map_or_else(
                || StepOutput::unchanged(parent_is_ascii),
                |changed| StepOutput::changed(changed, false),
            ),
            Self::Delete(matcher) => matcher.delete(text).map_or_else(
                || StepOutput::unchanged(parent_is_ascii),
                |(changed, is_ascii)| StepOutput::changed(changed, parent_is_ascii || is_ascii),
            ),
            Self::Normalize(matcher) => matcher.replace(text).map_or_else(
                || StepOutput::unchanged(parent_is_ascii),
                |changed| {
                    let is_ascii = changed.is_ascii();
                    StepOutput::changed(changed, is_ascii)
                },
            ),
            Self::PinYin(matcher) | Self::PinYinChar(matcher) => matcher.replace(text).map_or_else(
                || StepOutput::unchanged(parent_is_ascii),
                |changed| StepOutput::changed(changed, true),
            ),
        }
    }
}
