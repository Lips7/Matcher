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

Then you should find the `libmatcher_java.so`/`libmatcher_java.dylib`/`matcher_java.dll` in the `target/release` directory.

## Java Usage

We recommend using the high-level `SimpleMatcher` class, which provides a safe, idiomatic API and handles native memory management via `AutoCloseable`.

### SimpleMatcher Example

```java
import com.matcherjava.SimpleMatcher;
import com.matcherjava.extensiontypes.ProcessType;
import com.matcherjava.extensiontypes.ProcessTypeSerializer;
import com.matcherjava.extensiontypes.SimpleResult;
import com.alibaba.fastjson.JSON;
import com.alibaba.fastjson.serializer.SerializeConfig;
import java.util.*;

// Prepare your configuration
SerializeConfig config = new SerializeConfig();
config.put(ProcessType.class, new ProcessTypeSerializer());

Map<ProcessType, Map<String, String>> simpleTable = new HashMap<>();
Map<String, String> wordMap = new HashMap<>();
wordMap.put("1", "hello&world");
simpleTable.put(ProcessType.NONE, wordMap);

byte[] configBytes = JSON.toJSONBytes(simpleTable, config);

// Use try-with-resources for automatic cleanup
try (SimpleMatcher matcher = new SimpleMatcher(configBytes)) {
    String text = "hello,world";

    // Check for match
    boolean matched = matcher.isMatch(text);

    // Get detailed results
    List<SimpleResult> results = matcher.process(text);
}
// matcher.close() is called automatically here
```

## Low-Level API

For direct pointer access, use the static methods on `MatcherJava`. You **must** manually free the matcher via `dropSimpleMatcher` to avoid memory leaks.

```java
long ptr = MatcherJava.initSimpleMatcher(bytes);
boolean matched = MatcherJava.simpleMatcherIsMatch(ptr, text.getBytes(StandardCharsets.UTF_8));
String json = MatcherJava.simpleMatcherProcessAsString(ptr, text.getBytes(StandardCharsets.UTF_8));
MatcherJava.dropSimpleMatcher(ptr);
```