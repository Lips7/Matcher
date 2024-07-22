package com.matcher_java.extension_types;

import java.util.Map;
import java.util.HashMap;

public class MatchTableType {
    public static Map<String, Simple> Simple(ProcessType processType) {
        Map<String, Simple> map = new HashMap<>();
        map.put("simple", new Simple(processType));
        return map;
    }

    public static Map<String, Regex> Regex(ProcessType processType, RegexMatchType regexMatchType) {
        Map<String, Regex> map = new HashMap<>();
        map.put("regex", new Regex(processType, regexMatchType));
        return map;
    }

    public static Map<String, Similar> Similar(ProcessType processType, SimMatchType simMatchType,
            float threshold) {
        Map<String, Similar> map = new HashMap<>();
        map.put("similar", new Similar(processType, simMatchType, threshold));
        return map;
    }
}
