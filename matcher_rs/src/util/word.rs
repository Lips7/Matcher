use std::{borrow::Cow, fmt::Display};

use sonic_rs::{Deserialize, Serialize};

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SimpleWord(String);

impl SimpleWord {
    pub fn new<I>(word: I) -> Self
    where
        I: AsRef<str>,
    {
        SimpleWord(word.as_ref().to_owned())
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn and<I>(mut self, word: I) -> Self
    where
        I: AsRef<str>,
    {
        self.0.push('&');
        self.0.push_str(word.as_ref());
        self
    }

    pub fn not<I>(mut self, word: I) -> Self
    where
        I: AsRef<str>,
    {
        self.0.push('~');
        self.0.push_str(word.as_ref());
        self
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Display for SimpleWord {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<String> for SimpleWord {
    fn from(value: String) -> Self {
        SimpleWord(value)
    }
}

impl From<&str> for SimpleWord {
    fn from(value: &str) -> Self {
        SimpleWord(value.to_owned())
    }
}

impl<'a> From<Cow<'a, str>> for SimpleWord {
    fn from(value: Cow<'a, str>) -> Self {
        SimpleWord(value.into_owned())
    }
}

impl From<SimpleWord> for String {
    fn from(value: SimpleWord) -> Self {
        value.0
    }
}

impl AsRef<str> for SimpleWord {
    fn as_ref(&self) -> &str {
        &self.0
    }
}
