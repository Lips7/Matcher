package com.matcher_java.extension_types;

public class MatchResult {
    private int match_id;
    private int table_id;
    private int word_id;
    private String word;
    private float similarity;

    public MatchResult(int match_id, int table_id, int word_id, String word, float similarity) {
        this.match_id = match_id;
        this.table_id = table_id;
        this.word_id = word_id;
        this.word = word;
        this.similarity = similarity;
    }

    public int getMatchId() {
        return match_id;
    }

    public void setMatchId(int match_id) {
        this.match_id = match_id;
    }

    public int getTableId() {
        return table_id;
    }

    public void setTableId(int table_id) {
        this.table_id = table_id;
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

    public float getSimilarity() {
        return similarity;
    }

    public void setSimilarity(float similarity) {
        this.similarity = similarity;
    }
}