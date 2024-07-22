package com.matcher_java.extension_types;

import com.alibaba.fastjson.PropertyNamingStrategy;
import com.alibaba.fastjson.annotation.JSONType;

@JSONType(naming = PropertyNamingStrategy.SnakeCase)
public class Regex {
    private ProcessType process_type;
    private RegexMatchType regex_match_type;

    public Regex(ProcessType process_type, RegexMatchType regexMatchType) {
        this.process_type = process_type;
        this.regex_match_type = regexMatchType;
    }

    public ProcessType getProcessType() {
        return process_type;
    }

    public RegexMatchType getRegexMatchType() {
        return regex_match_type;
    }
}