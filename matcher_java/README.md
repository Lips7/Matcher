# Matcher Java JNI Bindings

![GitHub Actions Workflow Status](https://img.shields.io/github/actions/workflow/status/lips7/Matcher/test.yml)
![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)

Java JNI bindings for the [Matcher](https://github.com/Lips7/Matcher) library — a high-performance matcher designed to solve **LOGICAL** and **TEXT VARIATIONS** problems in word matching, implemented in Rust.

For detailed implementation, see the [Design Document](../DESIGN.md).

**Requirements:** Java 21+, Maven. Uses [fastjson](https://github.com/alibaba/fastjson) for JSON serialization.

## Installation

### Build from source

Requires the Rust **nightly** toolchain.

```shell
git clone https://github.com/Lips7/Matcher.git
cd Matcher
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- --default-toolchain nightly -y
cargo build --release
mkdir -p matcher_java/src/main/resources
cp target/release/libmatcher_java.dylib matcher_java/src/main/resources/  # .so on Linux, .dll on Windows
```

Or use `make build` from the repository root, which handles the copy step automatically.

### Maven coordinates

```xml
<groupId>com.matcher_java</groupId>
<artifactId>matcher_java</artifactId>
<version>0.11.0</version>
```

## Usage

### ProcessType

| Enum | Value | Description |
|------|-------|-------------|
| `NONE` | `0b00000001` | No transformation; match raw input |
| `FANJIAN` | `0b00000010` | Traditional Chinese to Simplified Chinese |
| `DELETE` | `0b00000100` | Remove symbols, punctuation, whitespace |
| `NORMALIZE` | `0b00001000` | Normalize character variants to basic forms |
| `DELETE_NORMALIZE` | `0b00001100` | Delete + Normalize combined |
| `FANJIAN_DELETE_NORMALIZE` | `0b00001110` | Fanjian + Delete + Normalize combined |
| `PINYIN` | `0b00010000` | Chinese characters to space-separated Pinyin |
| `PINYIN_CHAR` | `0b00100000` | Chinese characters to Pinyin without boundary spaces |

### SimpleMatcher (recommended)

The high-level `SimpleMatcher` class provides a safe, idiomatic API and handles native memory management via `AutoCloseable`.

```java
import com.matcherjava.SimpleMatcher;
import com.matcherjava.extensiontypes.ProcessType;
import com.matcherjava.extensiontypes.ProcessTypeSerializer;
import com.matcherjava.extensiontypes.SimpleResult;
import com.alibaba.fastjson.JSON;
import com.alibaba.fastjson.serializer.SerializeConfig;
import java.util.*;

// Prepare configuration
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

    boolean matched = matcher.isMatch(text);

    List<SimpleResult> results = matcher.process(text);
}
// matcher.close() is called automatically here
```

### Text Processing

Use `MatcherJava.textProcess` and `MatcherJava.reduceTextProcess` to apply transformations without a matcher:

```java
import com.matcherjava.MatcherJava;
import com.matcherjava.extensiontypes.ProcessType;
import java.nio.charset.StandardCharsets;

// Apply a single transformation
String normalized = MatcherJava.textProcess(
    ProcessType.NORMALIZE.getValue(),
    "Ｈｅｌｌｏ".getBytes(StandardCharsets.UTF_8)
);

// Get all intermediate transformation variants
String[] variants = MatcherJava.reduceTextProcess(
    ProcessType.FANJIAN_DELETE_NORMALIZE.getValue(),
    "你好，世界！".getBytes(StandardCharsets.UTF_8)
);
```

### Low-Level API

For direct pointer access, use the static methods on `MatcherJava`. You **must** manually free the matcher via `dropSimpleMatcher` to avoid memory leaks.

```java
long ptr = MatcherJava.initSimpleMatcher(bytes);
boolean matched = MatcherJava.simpleMatcherIsMatch(ptr, text.getBytes(StandardCharsets.UTF_8));
String json = MatcherJava.simpleMatcherProcessAsString(ptr, text.getBytes(StandardCharsets.UTF_8));
MatcherJava.dropSimpleMatcher(ptr);
```

## Contributing

Contributions to `matcher_java` are welcome! If you find a bug or have a feature request, please open an issue on the [GitHub repository](https://github.com/Lips7/Matcher). If you would like to contribute code, please fork the repository and submit a pull request.

## License

`matcher_java` is licensed under the MIT OR Apache-2.0 license.
