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

#[cfg(feature = "serde")]
#[cfg(test)]
mod test_serde {
    use super::*;

    #[derive(Serialize, Deserialize)]
    struct A {
        #[serde(with = "serde_regex")]
        b: Regex,
    }

    #[derive(Serialize, Deserialize)]
    struct B {
        #[serde(with = "serde_regex_list")]
        c: Vec<Regex>,
    }

    #[derive(Serialize, Deserialize)]
    struct C {
        #[serde(with = "serde_regex_set")]
        d: RegexSet,
    }

    #[test]
    fn test_serde_regex() {
        let sample = r#"[a-z"\]]+\d{1,10}""#;
        let sample_regex = A {
            b: Regex::new(sample).unwrap(),
        };
        let sample_regex_se = sonic_rs::to_string(&sample_regex).unwrap();
        let sample_regex_de: A = sonic_rs::from_str(&sample_regex_se).unwrap();

        assert_eq!(sample_regex_de.b.as_str(), sample);
    }

    #[test]
    fn test_serde_regex_list() {
        let sample = r#"[a-z"\]]+\d{1,10}""#;
        let sample_regex = B {
            c: vec![Regex::new(sample).unwrap()],
        };
        let sample_regex_se = sonic_rs::to_string(&sample_regex).unwrap();
        let sample_regex_de: B = sonic_rs::from_str(&sample_regex_se).unwrap();

        assert_eq!(sample_regex_de.c[0].as_str(), sample);
    }

    #[test]
    fn test_serde_regex_set() {
        let sample = r#"[a-z"\]]+\d{1,10}""#;
        let sample_regex = C {
            d: RegexSet::new([sample]).unwrap(),
        };
        let sample_regex_se = sonic_rs::to_string(&sample_regex).unwrap();
        let sample_regex_de: C = sonic_rs::from_str(&sample_regex_se).unwrap();

        assert_eq!(sample_regex_de.d.patterns()[0].as_str(), sample);
    }
}
