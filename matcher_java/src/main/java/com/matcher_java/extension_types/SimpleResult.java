package com.matcher_java.extension_types;

public class SimpleResult {
    private int word_id;
    private String word;

    public SimpleResult(int word_id, String word) {
        this.word_id = word_id;
        this.word = word;
    }

    public int getWordId() {
        return word_id;
    }

    public void setWordId(int word_id) {
        this.word_id = word_id;
    }

    public String getWord() {
        return word;
    }

    public void setWord(String word) {
        this.word = word;
    }
}
