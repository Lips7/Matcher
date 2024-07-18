#[cfg(feature = "serde")]
use std::borrow::Cow;

#[cfg(feature = "serde")]
use fancy_regex::Regex;
#[cfg(feature = "serde")]
use regex::RegexSet;
#[cfg(feature = "serde")]
use serde::{de::Error, Deserialize, Deserializer, Serialize, Serializer};

#[cfg(feature = "serde")]
pub mod serde_regex {
    use super::*;

    /// Deserialize and serialize functions for `Regex` type.
    ///
    /// This module provides custom serialization and deserialization
    /// for the `Regex` type from the `fancy_regex` crate using Serde.
    /// The regex is serialized as a string and deserialized back into a `Regex` object.
    ///
    /// To use the custom serialization and deserialization, the field in the struct must
    /// be annotated with `#[serde(with = "serde_regex")]`.
    ///
    /// The provided methods ensure that regex patterns are correctly handled during
    /// serialization and deserialization processes without losing the actual regex functionalities.
    pub fn deserialize<'de, D>(d: D) -> Result<Regex, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = <Cow<str>>::deserialize(d)?;

        match Regex::new(s.as_ref()) {
            Ok(regex) => Ok(regex),
            Err(err) => Err(D::Error::custom(err)),
        }
    }

    pub fn serialize<S>(regex: &Regex, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        regex.as_str().serialize(serializer)
    }
}

#[cfg(feature = "serde")]
pub mod serde_regex_list {
    use serde::ser::SerializeSeq;

    use super::*;

    /// Deserialize and serialize functions for a list of `Regex` types.
    ///
    /// This module provides custom serialization and deserialization
    /// for lists of the `Regex` type from the `fancy_regex` crate using Serde.
    /// Each regex in the list is serialized as a string and deserialized back into a `Regex` object.
    ///
    /// To use the custom serialization and deserialization, the field in the struct must
    /// be annotated with `#[serde(with = "serde_regex_list")]`.
    ///
    /// These methods ensure that lists of regex patterns are correctly handled during
    /// serialization and deserialization processes without losing the actual regex functionalities.
    pub fn deserialize<'de, D>(d: D) -> Result<Vec<Regex>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = <Vec<Cow<str>>>::deserialize(d)?;
        let mut regex_list = Vec::with_capacity(s.len());
        for e in s.into_iter() {
            let regex = Regex::new(e.as_ref()).map_err(D::Error::custom)?;
            regex_list.push(regex);
        }

        Ok(regex_list)
    }

    pub fn serialize<S>(regex_list: &Vec<Regex>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut seq = serializer.serialize_seq(Some(regex_list.len()))?;
        for e in regex_list {
            seq.serialize_element(e.as_str())?;
        }
        seq.end()
    }
}

#[cfg(feature = "serde")]
pub mod serde_regex_set {
    use serde::ser::SerializeSeq;

    use super::*;

    /// Deserialize and serialize functions for `RegexSet` type.
    ///
    /// This module provides custom serialization and deserialization
    /// for the `RegexSet` type from the `regex` crate using Serde.
    /// The regex set is serialized as a list of strings and deserialized back into a `RegexSet` object.
    ///
    /// To use the custom serialization and deserialization, the field in the struct must
    /// be annotated with `#[serde(with = "serde_regex_set")]`.
    ///
    /// These methods ensure that regex set patterns are correctly handled during
    /// serialization and deserialization processes without losing the actual regex functionalities.
    pub fn deserialize<'de, D>(d: D) -> Result<RegexSet, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = <Vec<Cow<str>>>::deserialize(d)?;
        let regex_set = RegexSet::new(s).map_err(D::Error::custom)?;

        Ok(regex_set)
    }

    pub fn serialize<S>(regex_set: &RegexSet, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut seq = serializer.serialize_seq(Some(regex_set.len()))?;
        for e in regex_set.patterns() {
            seq.serialize_element(e.as_str())?;
        }
        seq.end()
    }
}
