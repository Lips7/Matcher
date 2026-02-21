package com.matcher_java;

import com.alibaba.fastjson.JSON;
import com.alibaba.fastjson.serializer.SerializeConfig;
import com.matcher_java.extension_types.*;
import java.io.IOException;
import java.util.*;

public class MatcherJavaExample {
    public static void main(String[] args) throws IOException {
        System.out.println("--- Simple Matcher High-Level API Test ---");
        simpleMatcherHighLevelDemo();

        System.out.println("\n--- Matcher High-Level API Test ---");
        matcherHighLevelDemo();
    }

    public static void simpleMatcherHighLevelDemo() throws IOException {
        SerializeConfig config = new SerializeConfig();
        config.put(ProcessType.class, new ProcessTypeSerializer());

        Map<ProcessType, Map<String, String>> simpleTable = new HashMap<>();
        Map<String, String> wordMap = new HashMap<>();
        wordMap.put("1", "hello&world");
        simpleTable.put(ProcessType.MatchNone, wordMap);

        byte[] configBytes = JSON.toJSONBytes(simpleTable, config);

        try (SimpleMatcher matcher = new SimpleMatcher(configBytes)) {
            String text = "hello,world";

            boolean isMatch = matcher.isMatch(text);
            System.out.printf("isMatch: %s\n", isMatch);

            List<SimpleResult> results = matcher.process(text);
            System.out.printf("Results: %s\n", JSON.toJSONString(results));
        }
    }

    public static void matcherHighLevelDemo() throws IOException {
        SerializeConfig config = new SerializeConfig();
        config.put(ProcessType.class, new ProcessTypeSerializer());

        Map<String, List<MatchTable>> matchTableMap = new HashMap<>();
        List<MatchTable> tables = List.of(
            new MatchTable(1, MatchTableType.Simple(ProcessType.MatchNone), List.of("hello&world"), ProcessType.MatchNone, List.of())
        );
        matchTableMap.put("1", tables);

        byte[] configBytes = JSON.toJSONBytes(matchTableMap, config);

        try (Matcher matcher = new Matcher(configBytes)) {
            String text = "hello,world";

            boolean isMatch = matcher.isMatch(text);
            System.out.printf("isMatch: %s\n", isMatch);

            List<MatchResult> results = matcher.process(text);
            System.out.printf("Process Results: %s\n", JSON.toJSONString(results));

            Map<Integer, List<MatchResult>> wordResults = matcher.wordMatch(text);
            System.out.printf("Word Match Results: %s\n", JSON.toJSONString(wordResults));
        }
    }
}