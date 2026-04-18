use std::fmt;

/// Error returned when [`super::SimpleMatcher`] construction fails.
///
/// Each variant describes a specific failure mode. The enum is
/// `#[non_exhaustive]`, so new variants may be added in future minor releases
/// without breaking callers who use a wildcard arm.
///
/// # When does construction fail?
///
/// - **Empty pattern set** — no patterns remain after parsing (all entries were
///   empty strings or pure-NOT rules).
/// - **Invalid [`crate::ProcessType`] bits** — the caller passed a bitflag
///   value that is either zero (`ProcessType::empty()`) or has bit 7 set
///   (undefined). Use [`crate::ProcessType::None`] for raw-text matching.
/// - **Automaton build failure** — the underlying Aho-Corasick libraries
///   (`daachorse` or `aho-corasick`) rejected the compiled pattern set (e.g.,
///   the pattern set exceeded internal capacity limits).
///
/// # Examples
///
/// ```rust
/// use std::collections::HashMap;
///
/// use matcher_rs::{ProcessType, SimpleMatcher, SimpleTable};
///
/// // Empty tables are rejected.
/// let empty: SimpleTable = HashMap::new();
/// assert!(SimpleMatcher::new(&empty).is_err());
/// ```
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum MatcherError {
    /// The underlying Aho-Corasick library (`daachorse` or `aho-corasick`)
    /// failed to compile the pattern set.
    AutomatonBuild {
        /// Human-readable description from the third-party builder.
        reason: String,
    },

    /// A [`crate::ProcessType`] value is invalid for matcher construction:
    /// either zero (`ProcessType::empty()`, which has no defined matching
    /// semantics) or has bit 7 set (undefined). Use
    /// [`crate::ProcessType::None`] for raw-text matching.
    InvalidProcessType {
        /// The raw bitflag value that was rejected.
        bits: u8,
    },

    /// The pattern set is empty — no scannable patterns remain after parsing.
    ///
    /// This can happen when the input table is entirely empty, all pattern
    /// strings are empty, or every rule was a pure-NOT rule (which is
    /// unsatisfiable and skipped during parsing).
    EmptyPatterns,
}

impl fmt::Display for MatcherError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AutomatonBuild { reason } => write!(f, "automaton build failed: {reason}"),
            Self::InvalidProcessType { bits } => {
                if *bits == 0 {
                    write!(
                        f,
                        "invalid ProcessType bits: 0x00 \
                         (empty process type has no matching semantics; \
                         use ProcessType::None for raw-text matching)"
                    )
                } else {
                    write!(
                        f,
                        "invalid ProcessType bits: {bits:#04x} \
                         (only bits 0\u{2013}6 are defined; bit 7 must be zero)"
                    )
                }
            }
            Self::EmptyPatterns => write!(
                f,
                "empty pattern set: at least one scannable pattern is required"
            ),
        }
    }
}

impl std::error::Error for MatcherError {}

impl MatcherError {
    /// Wraps a third-party automaton build error (from `daachorse` or
    /// `aho-corasick`) into a [`MatcherError`].
    pub(crate) fn automaton_build(source: impl fmt::Display) -> Self {
        Self::AutomatonBuild {
            reason: source.to_string(),
        }
    }

    /// Creates an error for a [`crate::ProcessType`] value that is either zero
    /// or has bit 7 (undefined) set.
    pub(crate) fn invalid_process_type(bits: u8) -> Self {
        Self::InvalidProcessType { bits }
    }
}
