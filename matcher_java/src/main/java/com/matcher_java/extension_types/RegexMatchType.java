package com.matcher_java.extension_types;

import com.alibaba.fastjson.annotation.JSONField;

public enum RegexMatchType {
    MatchSimilarChar("similar_char"),
    MatchAcrostic("acrostic"),
    MatchRegex("regex");

    private final String value;

    RegexMatchType(String value) {
        this.value = value;
    }

    @JSONField
    public String getValue() {
        return value;
    }
}