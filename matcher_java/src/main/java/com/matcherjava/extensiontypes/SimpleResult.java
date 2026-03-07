package com.matcherjava.extensiontypes;

/**
 * Represents a match result returned by the SimpleMatcher.
 */
public class SimpleResult {
  private int wordId;
  private String word;

  /**
   * Constructs a SimpleResult.
   *
   * @param wordId The ID of the matched word.
   * @param word   The matched word string.
   */
  public SimpleResult(int wordId, String word) {
    this.wordId = wordId;
    this.word = word;
  }

  /**
   * Gets the word ID.
   *
   * @return the word ID.
   */
  public int getWordId() {
    return wordId;
  }

  /**
   * Sets the word ID.
   *
   * @param wordId the new word ID.
   */
  public void setWordId(int wordId) {
    this.wordId = wordId;
  }

  /**
   * Gets the matched word.
   *
   * @return the matched word.
   */
  public String getWord() {
    return word;
  }

  /**
   * Sets the matched word.
   *
   * @param word the new word.
   */
  public void setWord(String word) {
    this.word = word;
  }
}
