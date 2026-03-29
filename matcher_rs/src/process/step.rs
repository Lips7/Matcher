//! Canonical execution semantics for a single transformation step.
//!
//! A [`TransformStep`] wraps one of the low-level transform engines (Fanjian, Delete,
//! Normalize, PinYin, PinYinChar) and provides a uniform [`apply`](TransformStep::apply)
//! interface. The step is compiled once by the [`super::registry`] and cached for the
//! lifetime of the process; callers never construct these directly.
//!
//! [`StepOutput`] carries the result: either `changed = None` (the text was unaffected) or
//! `changed = Some(new_string)` together with an updated `is_ascii` flag.

use crate::process::transform::charwise::{FanjianMatcher, PinyinMatcher};
use crate::process::transform::delete::DeleteMatcher;
use crate::process::transform::normalize::NormalizeMatcher;

/// Result of applying one compiled pipeline step to a text variant.
///
/// `changed` is [`None`] when the step is a no-op for the provided input (the text was
/// not modified at all). `is_ascii` always describes the *post-step* text, regardless of
/// whether the text changed. Callers use this to decide whether to scan with the
/// ASCII-only or charwise Aho-Corasick automaton.
pub(crate) struct StepOutput {
    /// The transformed string, or [`None`] if the step did not modify the input.
    pub(crate) changed: Option<String>,
    /// Whether the post-step text consists entirely of ASCII bytes.
    pub(crate) is_ascii: bool,
}

/// Constructors for [`StepOutput`].
impl StepOutput {
    /// Creates an unchanged result that preserves the caller-provided ASCII flag.
    ///
    /// Used when a step determines that no characters in the input are affected by its
    /// transformation table.
    #[inline(always)]
    pub(crate) fn unchanged(is_ascii: bool) -> Self {
        Self {
            changed: None,
            is_ascii,
        }
    }

    /// Creates a changed result with the produced `String` and its ASCII status.
    #[inline(always)]
    pub(crate) fn changed(changed: String, is_ascii: bool) -> Self {
        Self {
            changed: Some(changed),
            is_ascii,
        }
    }
}

/// Compiled single-bit transformation step.
///
/// Each variant wraps the corresponding low-level matcher from [`super::transform`].
/// Instances are created by [`super::registry::build_transform_step`] and cached in
/// `TRANSFORM_STEP_CACHE` for the lifetime of the process. The [`apply`](Self::apply)
/// method provides a uniform dispatch point.
#[derive(Clone)]
pub(crate) enum TransformStep {
    /// Raw-text path; returns the input unchanged.
    None,
    /// Traditional-to-Simplified Chinese conversion via page-table lookup.
    Fanjian(FanjianMatcher),
    /// Codepoint deletion using a bitset, with optional SIMD acceleration.
    Delete(DeleteMatcher),
    /// Multi-character normalization replacements via Aho-Corasick.
    Normalize(NormalizeMatcher),
    /// Pinyin conversion with inter-syllable spaces preserved.
    PinYin(PinyinMatcher),
    /// Pinyin conversion with inter-syllable spaces stripped.
    PinYinChar(PinyinMatcher),
}

/// Execution policy for one cached transform step.
impl TransformStep {
    /// Applies this step to `text`, returning a [`StepOutput`] indicating what changed.
    ///
    /// `parent_is_ascii` is the ASCII flag inherited from the parent text variant.
    /// Steps such as [`Delete`](Self::Delete) use it to skip redundant byte scans
    /// when the input is already known to be ASCII. The returned `is_ascii` flag is
    /// always authoritative for the *output* text:
    ///
    /// - **Fanjian** — always sets `is_ascii = false` (output may contain CJK).
    /// - **Delete** — ORs the parent flag with its own scan (deletion can only remove
    ///   non-ASCII chars, so if parent was ASCII the output is too).
    /// - **Normalize** — tracked incrementally during the replacement loop.
    /// - **PinYin / PinYinChar** — always sets `is_ascii = true` (Pinyin is ASCII).
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
                |(changed, is_ascii)| StepOutput::changed(changed, is_ascii),
            ),
            Self::PinYin(matcher) | Self::PinYinChar(matcher) => matcher.replace(text).map_or_else(
                || StepOutput::unchanged(parent_is_ascii),
                |changed| StepOutput::changed(changed, true),
            ),
        }
    }
}
