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
    /// `DeleteNormalize` and `FanjianDeleteNormalize` are named aliases for common
    /// combinations, not separate transformation primitives. Iterating over a composite
    /// value with [`ProcessType::iter()`] yields individual single-bit flags in ascending
    /// bit order: `Fanjian`, then `Delete`, then `Normalize`, etc.
    ///
    /// The default value is `ProcessType::empty()` (no bits set), which differs from
    /// [`ProcessType::None`] (the explicit "raw text" flag at bit 0).
    ///
    /// # Layout
    ///
    /// | Flag | Bit | Value |
    /// |------|-----|-------|
    /// | `None` | 0 | `0x01` |
    /// | `Fanjian` | 1 | `0x02` |
    /// | `Delete` | 2 | `0x04` |
    /// | `Normalize` | 3 | `0x08` |
    /// | `PinYin` | 4 | `0x10` |
    /// | `PinYinChar` | 5 | `0x20` |
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
    /// // Iterate over the individual bits in order.
    /// let bits: Vec<_> = combined.iter().collect();
    /// assert_eq!(bits, vec![ProcessType::Fanjian, ProcessType::Delete]);
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
    ///     ProcessType::FanjianDeleteNormalize,
    ///     ProcessType::Fanjian | ProcessType::Delete | ProcessType::Normalize,
    /// );
    /// ```
    #[derive(Hash, PartialEq, Eq, Clone, Copy, Debug, Default)]
    pub struct ProcessType: u8 {
        /// No transformation; match the raw input.
        ///
        /// Including this flag alongside others ensures the untransformed text is also
        /// checked during matching.
        const None = 0b00000001;

        /// Traditional Chinese to Simplified Chinese conversion.
        ///
        /// Uses a page-table lookup compiled from `process_map/FANJIAN.txt`.
        const Fanjian = 0b00000010;

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

        /// Shorthand for `Fanjian | Delete | Normalize`.
        const FanjianDeleteNormalize = 0b00001110;

        /// Converts Chinese characters to space-separated Pinyin syllables.
        ///
        /// Uses a page-table lookup compiled from `process_map/PINYIN.txt`.
        const PinYin = 0b00010000;

        /// Converts Chinese characters to Pinyin with inter-syllable spaces stripped.
        ///
        /// Uses the same source as [`PinYin`](Self::PinYin) but trims the leading space
        /// from each mapping at build time.
        const PinYinChar = 0b00100000;
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
/// let combined = ProcessType::Fanjian | ProcessType::Delete;
/// let json = serde_json::to_string(&combined).unwrap();
/// // Fanjian = 0x02, Delete = 0x04 → 6
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
/// Rejects values with undefined bits (bits 6–7) to prevent out-of-bounds indexing in
/// downstream lookup tables that are sized for the 6-bit flag space.
///
/// # Examples
///
/// ```rust
/// use matcher_rs::ProcessType;
///
/// // Valid round-trip:
/// let combined = ProcessType::Fanjian | ProcessType::Delete;
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
/// Active flag names are lowercased and joined with underscores. For example,
/// `ProcessType::Fanjian | ProcessType::Delete` formats as `"fanjian_delete"`.
///
/// # Examples
///
/// ```rust
/// use matcher_rs::ProcessType;
///
/// assert_eq!(format!("{}", ProcessType::None), "none");
/// assert_eq!(
///     format!("{}", ProcessType::Fanjian | ProcessType::Delete),
///     "fanjian_delete"
/// );
/// assert_eq!(
///     format!("{}", ProcessType::FanjianDeleteNormalize),
///     "fanjian_delete_normalize"
/// );
/// // Empty flags (no bits set) produce an empty string.
/// assert_eq!(format!("{}", ProcessType::empty()), "");
/// ```
impl Display for ProcessType {
    /// Formats active flag names as lowercase strings joined by underscores.
    ///
    /// Empty flags produce an empty string; single flags produce just the lowercased name.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let names = self
            .iter_names()
            .map(|(name, _)| name.to_lowercase())
            .collect::<Vec<_>>();
        write!(f, "{}", names.join("_"))
    }
}
