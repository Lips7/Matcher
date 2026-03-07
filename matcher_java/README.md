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

## Java Usage

We recommend using the high-level `SimpleMatcher` class, which provides a safe, idiomatic API and handles native memory management via `AutoCloseable`.

### SimpleMatcher Example

```java
import com.matcher_java.SimpleMatcher;
import com.matcher_java.extension_types.ProcessType;
import com.matcher_java.extension_types.SimpleResult;
import com.alibaba.fastjson.JSON;
import java.util.*;

// Prepare your configuration
Map<ProcessType, Map<String, String>> simpleTable = new HashMap<>();
Map<String, String> wordMap = new HashMap<>();
wordMap.put("1", "hello&world");
simpleTable.put(ProcessType.MatchNone, wordMap);

byte[] configBytes = JSON.toJSONBytes(simpleTable);

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

If you need direct access to the native pointers or specialized functions, you can still use `MatcherJava.INSTANCE`. However, you **must** manually free resources using `drop_simple_matcher` or `drop_string` to avoid memory leaks.

```java
MatcherJava instance = MatcherJava.INSTANCE;
Pointer ptr = instance.init_simple_matcher(bytes);
// ...
instance.drop_simple_matcher(ptr);
```