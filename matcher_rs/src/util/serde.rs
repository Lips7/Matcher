use std::borrow::Cow;

use fancy_regex::Regex;
use serde::{de::Error, Deserialize, Deserializer, Serialize, Serializer};

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
}
