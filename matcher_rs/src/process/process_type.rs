//! [`ProcessType`] bitflags definition and its serde/display implementations.

use std::fmt::Display;

use bitflags::bitflags;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

bitflags! {
    /// Bitflags controlling which text-transformation steps to apply before matching.
    ///
    /// Flags can be combined freely. The matcher decomposes each composite value into
    /// single-step edges, builds a shared transform tree from the active set, and reuses
    /// intermediate results where prefixes overlap.
    ///
    /// `DeleteNormalize` and `FanjianDeleteNormalize` are named aliases for common
    /// combinations, not separate transformation primitives.
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

        /// Remove the codepoints configured in the Delete tables, including the built-in
        /// whitespace set.
        const Delete = 0b00000100;

        /// Apply the Normalize replacement tables (for example full-width forms and
        /// digit-like variants).
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
