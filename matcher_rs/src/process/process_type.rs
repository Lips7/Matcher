//! [`ProcessType`] bitflags definition and its serde/display implementations.
//!
//! `ProcessType` is the user-facing knob for the transformation pipeline: each bit selects
//! one transformation step, and bits compose freely with `|`. Named aliases like
//! [`ProcessType::DeleteNormalize`] are provided for common combinations. The raw `u8`
//! representation is used for serialization so that the wire format stays compact.

use std::fmt::Display;

use bitflags::bitflags;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

bitflags! {
    /// Bitflags controlling which text-transformation steps to apply before matching.
    ///
    /// Each flag selects one transformation primitive. Flags compose freely with `|`:
    /// the matcher decomposes each composite value into single-step edges, builds a
    /// shared transform tree from the active set, and reuses intermediate results where
    /// prefixes overlap.
    ///
    /// `DeleteNormalize` and `VariantNormDeleteNormalize` are named aliases for common
    /// combinations, not separate transformation primitives. Iterating over a composite
    /// value with [`ProcessType::iter()`] yields individual single-bit flags in ascending
    /// bit order: `VariantNorm`, then `Delete`, then `Normalize`, etc.
    ///
    /// The default value is `ProcessType::empty()` (no bits set), which differs from
    /// [`ProcessType::None`] (the explicit "raw text" flag at bit 0).
    ///
    /// # Layout
    ///
    /// | Flag | Bit | Value |
    /// |------|-----|-------|
    /// | `None` | 0 | `0x01` |
    /// | `VariantNorm` | 1 | `0x02` |
    /// | `Delete` | 2 | `0x04` |
    /// | `Normalize` | 3 | `0x08` |
    /// | `Romanize` | 4 | `0x10` |
    /// | `RomanizeChar` | 5 | `0x20` |
    /// | `EmojiNorm` | 6 | `0x40` |
    ///
    /// # Examples
    ///
    /// ```rust
    /// use matcher_rs::ProcessType;
    ///
    /// // Compose flags with | just like standard bitflags.
    /// let combined = ProcessType::VariantNorm | ProcessType::Delete;
    /// assert!(combined.contains(ProcessType::VariantNorm));
    /// assert!(combined.contains(ProcessType::Delete));
    ///
    /// // Iterate over the individual bits in order.
    /// let bits: Vec<_> = combined.iter().collect();
    /// assert_eq!(bits, vec![ProcessType::VariantNorm, ProcessType::Delete]);
    ///
    /// // Serialize/deserialize as a raw u8 for compact wire format.
    /// let raw = combined.bits();
    /// assert_eq!(ProcessType::from_bits_retain(raw), combined);
    ///
    /// // Including `None` keeps the raw-text path alongside transformed ones.
    /// let raw_and_deleted = ProcessType::None | ProcessType::Delete;
    /// assert!(raw_and_deleted.contains(ProcessType::None));
    /// assert!(raw_and_deleted.contains(ProcessType::Delete));
    ///
    /// // Named aliases are just shorthand for the equivalent OR.
    /// assert_eq!(
    ///     ProcessType::VariantNormDeleteNormalize,
    ///     ProcessType::VariantNorm | ProcessType::Delete | ProcessType::Normalize,
    /// );
    /// ```
    #[derive(Hash, PartialEq, Eq, Clone, Copy, Debug, Default)]
    pub struct ProcessType: u8 {
        /// No transformation; match the raw input.
        ///
        /// Including this flag alongside others ensures the untransformed text is also
        /// checked during matching.
        const None = 0b00000001;

        /// CJK variant normalization (Chinese Traditional→Simplified, Japanese
        /// Kyūjitai→Shinjitai, half-width katakana→full-width).
        ///
        /// Uses a page-table lookup compiled from `process_map/VARIANT_NORM.txt`.
        const VariantNorm = 0b00000010;

        /// Removes codepoints configured in the Delete tables.
        ///
        /// Uses a bitset compiled from `process_map/TEXT-DELETE.txt` with optional
        /// SIMD acceleration for ASCII-heavy input.
        const Delete = 0b00000100;

        /// Applies the Normalize replacement tables (e.g. full-width forms, digit-like
        /// variants).
        ///
        /// Uses an Aho-Corasick automaton compiled from `process_map/NORM.txt` and
        /// `process_map/NUM-NORM.txt`.
        const Normalize = 0b00001000;

        /// Shorthand for `Delete | Normalize`.
        const DeleteNormalize = 0b00001100;

        /// Shorthand for `VariantNorm | Delete | Normalize`.
        const VariantNormDeleteNormalize = 0b00001110;

        /// Converts CJK characters to space-separated romanized syllables
        /// (Chinese Pinyin, Japanese kana Romaji, Korean Revised Romanization).
        ///
        /// Uses a page-table lookup compiled from `process_map/ROMANIZE.txt`.
        const Romanize = 0b00010000;

        /// Converts CJK characters to romanized form with inter-syllable spaces stripped.
        ///
        /// Uses the same source as [`Romanize`](Self::Romanize) but trims the leading space
        /// from each mapping at build time.
        const RomanizeChar = 0b00100000;

        /// Converts emoji codepoints to space-prefixed English words using CLDR short names.
        ///
        /// Uses a page-table lookup compiled from `process_map/EMOJI_NORM.txt`.
        /// Also strips emoji modifiers (ZWJ, VS16, skin tones) by mapping them to empty string.
        ///
        /// Does NOT compose usefully with [`Delete`](Self::Delete) — Delete removes emoji
        /// before EmojiNorm can see them. Use one or the other.
        const EmojiNorm = 0b01000000;
    }
}

/// Compact serde serialization: writes the raw `u8` bitfield.
///
/// This keeps the wire format tiny (one byte) regardless of which flags are set.
/// Composite flags serialize as the bitwise OR of their components.
///
/// # Examples
///
/// ```rust
/// use matcher_rs::ProcessType;
///
/// let combined = ProcessType::VariantNorm | ProcessType::Delete;
/// let json = serde_json::to_string(&combined).unwrap();
/// // VariantNorm = 0x02, Delete = 0x04 → 6
/// assert_eq!(json, "6");
///
/// // Single flag:
/// assert_eq!(serde_json::to_string(&ProcessType::None).unwrap(), "1");
/// ```
impl Serialize for ProcessType {
    /// Serializes the bitflags value as its underlying `u8` representation.
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.bits().serialize(serializer)
    }
}

/// Compact serde deserialization: reads a `u8` and validates that only known bits are set.
///
/// Rejects values with undefined bits (bit 7) to prevent out-of-bounds indexing in
/// downstream lookup tables that are sized for the 7-bit flag space.
///
/// # Examples
///
/// ```rust
/// use matcher_rs::ProcessType;
///
/// // Valid round-trip:
/// let combined = ProcessType::VariantNorm | ProcessType::Delete;
/// let json = serde_json::to_string(&combined).unwrap();
/// let back: ProcessType = serde_json::from_str(&json).unwrap();
/// assert_eq!(back, combined);
///
/// // Invalid bits are rejected:
/// let result: Result<ProcessType, _> = serde_json::from_str("128");
/// assert!(result.is_err());
/// ```
impl<'de> Deserialize<'de> for ProcessType {
    /// Deserializes a `u8` into [`ProcessType`], rejecting unknown bit combinations.
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let bits: u8 = u8::deserialize(deserializer)?;
        ProcessType::from_bits(bits).ok_or_else(|| {
            serde::de::Error::custom(format!(
                "invalid ProcessType bits: {bits:#04x} (unknown bits set)"
            ))
        })
    }
}

/// Human-readable formatting for [`ProcessType`] combinations.
///
/// Active flag names are lowercased with underscores and joined with underscores.
/// For example, `ProcessType::VariantNorm | ProcessType::Delete` formats as
/// `"variant_norm_delete"`.
///
/// # Examples
///
/// ```rust
/// use matcher_rs::ProcessType;
///
/// assert_eq!(format!("{}", ProcessType::None), "none");
/// assert_eq!(
///     format!("{}", ProcessType::VariantNorm | ProcessType::Delete),
///     "variant_norm_delete"
/// );
/// assert_eq!(
///     format!("{}", ProcessType::VariantNormDeleteNormalize),
///     "variant_norm_delete_normalize"
/// );
/// // Empty flags (no bits set) produce an empty string.
/// assert_eq!(format!("{}", ProcessType::empty()), "");
/// ```
impl Display for ProcessType {
    /// Formats active flag names as snake_case strings joined by underscores.
    ///
    /// Empty flags produce an empty string; single flags produce just the snake_case name.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        fn to_snake(name: &str) -> &str {
            match name {
                "VariantNorm" => "variant_norm",
                "DeleteNormalize" => "delete_normalize",
                "VariantNormDeleteNormalize" => "variant_norm_delete_normalize",
                "RomanizeChar" => "romanize_char",
                "None" => "none",
                "Delete" => "delete",
                "Normalize" => "normalize",
                "Romanize" => "romanize",
                _ => "unknown",
            }
        }

        let mut first = true;
        for (name, _) in self.iter_names() {
            if !first {
                f.write_str("_")?;
            }
            f.write_str(to_snake(name))?;
            first = false;
        }
        Ok(())
    }
}
