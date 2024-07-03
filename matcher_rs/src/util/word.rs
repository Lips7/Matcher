use std::borrow::Cow;

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

    pub fn from_string(word: String) -> Self {
        SimpleWord(word)
    }

    pub fn from_str(word: &str) -> Self {
        SimpleWord(word.to_owned())
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

    pub fn to_string(self) -> String {
        self.0
    }

    pub fn as_str(&self) -> &str {
        &self.0
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

impl Into<String> for SimpleWord {
    fn into(self) -> String {
        self.0
    }
}

impl AsRef<str> for SimpleWord {
    fn as_ref(&self) -> &str {
        &self.0
    }
}
