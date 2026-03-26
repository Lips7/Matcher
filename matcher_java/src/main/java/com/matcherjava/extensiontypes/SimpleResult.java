package com.matcherjava.extensiontypes;

import com.alibaba.fastjson.annotation.JSONField;

/** Java view of a single native match result. */
public class SimpleResult {
  @JSONField(name = "word_id")
  private int wordId;
  private String word;

  public SimpleResult() {}

  public SimpleResult(int wordId, String word) {
    this.wordId = wordId;
    this.word = word;
  }

  public int getWordId() {
    return wordId;
  }

  public void setWordId(int wordId) {
    this.wordId = wordId;
  }

  public String getWord() {
    return word;
  }

  public void setWord(String word) {
    this.word = word;
  }
}
