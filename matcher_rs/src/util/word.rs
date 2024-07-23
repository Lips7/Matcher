use std::borrow::Cow;
use std::fmt::Display;

use serde::{Deserialize, Serialize};

/// A struct representing a simple word.
///
/// This struct holds a single `String` and provides various methods for
/// manipulating and querying the contents of the string. It supports the
/// `Debug`, `Default`, `Clone`, `PartialEq`, `Eq`, `Serialize`, and
/// `Deserialize` traits, making it versatile for different use cases such
/// as debugging, serialization, and comparison.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SimpleWord(String);

impl SimpleWord {
    /// Creates a new `SimpleWord` instance from any type that can be referenced as a string.
    ///
    /// # Arguments
    ///
    /// * `word` - An input that implements the `AsRef<str>` trait. This allows for a wide range
    ///            of input types, including `String`, `&str`, and `Cow<str>`.
    ///
    /// # Returns
    ///
    /// A `SimpleWord` instance containing the provided word.
    ///
    /// # Examples
    ///
    /// ```
    /// use matcher_rs::SimpleWord;
    ///
    /// let word = SimpleWord::new("hello");
    /// assert_eq!(word.as_str(), "hello");
    /// ```
    pub fn new<I>(word: I) -> Self
    where
        I: AsRef<str>,
    {
        SimpleWord(word.as_ref().to_owned())
    }

    /// Returns the length of the string contained within the `SimpleWord`.
    ///
    /// This method returns the number of characters in the underlying string.
    ///
    /// # Returns
    ///
    /// The length of the string as a `usize`.
    ///
    /// # Examples
    ///
    /// ```
    /// use matcher_rs::SimpleWord;
    ///
    /// let word = SimpleWord::new("hello");
    /// assert_eq!(word.len(), 5);
    /// ```
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Checks if the string contained within the `SimpleWord` is empty.
    ///
    /// This method returns true if the underlying string has a length of zero.
    ///
    /// # Returns
    ///
    /// `true` if the string is empty, `false` otherwise.
    ///
    /// # Examples
    ///
    /// ```
    /// use matcher_rs::SimpleWord;
    ///
    /// let empty_word = SimpleWord::new("");
    /// assert!(empty_word.is_empty());
    ///
    /// let non_empty_word = SimpleWord::new("hello");
    /// assert!(!non_empty_word.is_empty());
    /// ```
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Appends a given word to the current `SimpleWord` with an `&`.
    ///
    /// This method takes an input that implements the `AsRef<str>` trait and appends
    /// it to the current `SimpleWord` instance, preceded by the `&` character.
    ///
    /// # Arguments
    ///
    /// * `word` - An input that implements the `AsRef<str>` trait. This could be a
    ///            `String`, `&str`, or `Cow<str>`.
    ///
    /// # Returns
    ///
    /// A new `SimpleWord` instance with the appended word.
    ///
    /// # Examples
    ///
    /// ```
    /// use matcher_rs::SimpleWord;
    ///
    /// let word1 = SimpleWord::new("hello");
    /// let word2 = word1.and("world");
    /// assert_eq!(word2.as_str(), "hello&world");
    /// ```
    pub fn and<I>(mut self, word: I) -> Self
    where
        I: AsRef<str>,
    {
        self.0.push('&');
        self.0.push_str(word.as_ref());
        self
    }

    /// Prepends a given word to the current `SimpleWord` with a `~`.
    ///
    /// This method takes an input that implements the `AsRef<str>` trait and prepends
    /// it to the current `SimpleWord` instance, preceded by the `~` character.
    ///
    /// # Arguments
    ///
    /// * `word` - An input that implements the `AsRef<str>` trait. This could be a
    ///            `String`, `&str`, or `Cow<str>`.
    ///
    /// # Returns
    ///
    /// A new `SimpleWord` instance with the prepended word.
    ///
    /// # Examples
    ///
    /// ```
    /// use matcher_rs::SimpleWord;
    ///
    /// let word1 = SimpleWord::new("world");
    /// let word2 = word1.not("hello");
    /// assert_eq!(word2.as_str(), "world~hello");
    /// ```
    pub fn not<I>(mut self, word: I) -> Self
    where
        I: AsRef<str>,
    {
        self.0.push('~');
        self.0.push_str(word.as_ref());
        self
    }

    /// Returns a string slice of the contents of the `SimpleWord`.
    ///
    /// This method allows for borrowing the underlying string without taking ownership.
    ///
    /// # Returns
    ///
    /// A string slice (`&str`) of the contents.
    ///
    /// # Examples
    ///
    /// ```
    /// use matcher_rs::SimpleWord;
    ///
    /// let word = SimpleWord::new("hello");
    /// assert_eq!(word.as_str(), "hello");
    /// ```
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
