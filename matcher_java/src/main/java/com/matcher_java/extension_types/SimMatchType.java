package com.matcher_java.extension_types;

import com.alibaba.fastjson.annotation.JSONField;

public enum SimMatchType {
    MatchLevenshtein("levenshtein");

    private final String value;

    SimMatchType(String value) {
        this.value = value;
    }

    @JSONField
    public String getValue() {
        return value;
    }
}