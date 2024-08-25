# Matcher Rust Implement JAVA FFI bindings

## Overview

A high-performance matcher designed to solve **LOGICAL** and **TEXT VARIATIONS** problems in word matching, implemented in Rust.

## Installation

### Build from source

```shell
git clone https://github.com/Lips7/Matcher.git
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- --default-toolchain nightly -y
cargo build --release
```

Then you should find the `libmatcher_c.so`/`libmatcher_c.dylib`/`matcher_c.dll` in the `target/release` directory.

### Install pre-built binary

Visit the [release page](https://github.com/Lips7/Matcher/releases) to download the pre-built binary.

## Java usage example

Put the `matcher_c` dynamic library under the `src/main/resources` directory.

Copy the code below or refer to [MatcherJavaExample.java](./src/test/java/com/matcher_java/MatcherJavaExample.java).

```java
package com.matcher_java;

import com.alibaba.fastjson.JSON;
import com.alibaba.fastjson.serializer.SerializeConfig;
import com.matcher_java.extension_types.MatchTable;
import com.matcher_java.extension_types.MatchTableType;
import com.matcher_java.extension_types.ProcessType;
import com.matcher_java.extension_types.ProcessTypeSerializer;
import com.sun.jna.Pointer;

import java.io.IOException;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;

public class MatcherJavaExample {
    public static void main(String[] args) throws IOException {
        System.out.println("Simple Matcher Test");
        simple_matcher_process_demo();

        System.out.println("\n");

        System.out.println("Matcher Test");
        matcher_process_demo();
    }

    public static void simple_matcher_process_demo() throws IOException {
        SerializeConfig serializeConfig = new SerializeConfig();
        serializeConfig.put(ProcessType.class, new ProcessTypeSerializer());

        Map<ProcessType, Map<String, String>> simpleTable = new HashMap<>();
        Map<String, String> wordMap = new HashMap<>();
        wordMap.put("1", "hello&world");
        simpleTable.put(ProcessType.MatchNone, wordMap);

        String simpleTableStr = JSON.toJSONString(simpleTable, serializeConfig);
        System.out.printf("simple_table: %s\n", simpleTableStr);

        byte[] simpleTableBytes = JSON.toJSONBytes(simpleTable, serializeConfig);

        MatcherJava instance = MatcherJava.INSTANCE;

        Pointer simpleMatcher = instance.init_simple_matcher(simpleTableBytes);

        byte[] strBytes = "hello,world".getBytes("utf-8");
        byte[] cStrBytes = new byte[strBytes.length + 1];
        System.arraycopy(strBytes, 0, cStrBytes, 0, strBytes.length);

        boolean isMatch = instance.simple_matcher_is_match(simpleMatcher, cStrBytes);
        System.out.printf("isMatch: %s\n", isMatch);

        Pointer matchResPtr = instance.simple_matcher_process_as_string(simpleMatcher, cStrBytes);
        String matchRes = matchResPtr.getString(0, "utf-8");
        System.out.printf("matchRes: %s\n", matchRes);
        instance.drop_string(matchResPtr);

        instance.drop_simple_matcher(simpleMatcher);
    }

    public static void matcher_process_demo() throws IOException {
        SerializeConfig serializeConfig = new SerializeConfig();
        serializeConfig.put(ProcessType.class, new ProcessTypeSerializer());

        Map<String, List<MatchTable>> matchTableMap = new HashMap<>();
        List<MatchTable> matchTableList = new ArrayList<>();
        MatchTable matchTable = new MatchTable(1, MatchTableType.Simple(ProcessType.MatchNone), List.of("hello&world"), ProcessType.MatchNone, List.of());
        matchTableList.add(matchTable);
        matchTableMap.put("1", matchTableList);

        String matchTableMapStr = JSON.toJSONString(matchTableMap, serializeConfig);
        System.out.printf("match_table_map: %s\n", matchTableMapStr);

        byte[] matchTableMapBytes = JSON.toJSONBytes(matchTableMap, serializeConfig);

        MatcherJava instance = MatcherJava.INSTANCE;

        Pointer matcher = instance.init_matcher(matchTableMapBytes);

        byte[] strBytes = "hello,world".getBytes("utf-8");
        byte[] cStrBytes = new byte[strBytes.length + 1];
        System.arraycopy(strBytes, 0, cStrBytes, 0, strBytes.length);

        boolean isMatch = instance.matcher_is_match(matcher, cStrBytes);
        System.out.printf("isMatch: %s\n", isMatch);

        Pointer matchResPtr1 = instance.matcher_process_as_string(matcher, cStrBytes);
        String matchRes1 = matchResPtr1.getString(0, "utf-8");
        System.out.printf("matchRes: %s\n", matchRes1);
        instance.drop_string(matchResPtr1);

        Pointer matchResPtr2 = instance.matcher_word_match_as_string(matcher, cStrBytes);
        String matchRes2 = matchResPtr2.getString(0, "utf-8");
        System.out.printf("matchRes: %s\n", matchRes2);
        instance.drop_string(matchResPtr2);

        instance.drop_matcher(matcher);
    }
}
```

## Important Notes

Always call `drop_matcher`, `drop_simple_matcher`, and `drop_string` after initializing and processing to avoid memory leaks.