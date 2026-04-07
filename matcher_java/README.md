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
just build
```

`just build` compiles all packages and copies the dynamic library to the right location automatically. You can also build manually:

```shell
cargo build --release
mkdir -p matcher_java/src/main/resources
cp target/release/libmatcher_java.dylib matcher_java/src/main/resources/  # .so on Linux, .dll on Windows
```

### Maven coordinates

```xml
<groupId>com.matcher_java</groupId>
<artifactId>matcher_java</artifactId>
<version>0.14.2</version>
```

## Usage

### ProcessType

| Enum | Value | Description |
|------|-------|-------------|
| `NONE` | `0b00000001` | No transformation; match raw input |
| `VARIANT_NORM` | `0b00000010` | CJK variant normalization |
| `DELETE` | `0b00000100` | Remove symbols, punctuation, whitespace |
| `NORMALIZE` | `0b00001000` | Normalize character variants to basic forms |
| `DELETE_NORMALIZE` | `0b00001100` | Delete + Normalize combined |
| `VARIANT_NORM_DELETE_NORMALIZE` | `0b00001110` | VariantNorm + Delete + Normalize combined |
| `ROMANIZE` | `0b00010000` | CJK characters to space-separated romanization (Pinyin, Romaji, RR) |
| `ROMANIZE_CHAR` | `0b00100000` | CJK characters to romanization without boundary spaces |
| `EMOJI_NORM` | `0b01000000` | Emoji → English words (CLDR short names), strips modifiers |

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

### OR, NOT, and Word Boundary

```java
import com.matcherjava.SimpleMatcher;

// ProcessType.NONE = 1
// Rules: OR (word 1), NOT (word 2), word boundary (word 3), combined (word 4)
String json = """
    {"1":{
        "1":"color|colour",
        "2":"banana~peel",
        "3":"\\\\bcat\\\\b",
        "4":"bright&color|colour~\\\\bdark\\\\b"
    }}""";
try (SimpleMatcher matcher = new SimpleMatcher(json.getBytes())) {
    assert matcher.isMatch("nice colour");            // OR
    assert matcher.isMatch("banana split");            // NOT (no veto)
    assert !matcher.isMatch("banana peel");            // NOT (vetoed)
    assert matcher.isMatch("the cat sat");             // word boundary
    assert !matcher.isMatch("concatenate");            // substring, not whole word
    assert matcher.isMatch("bright colour");           // combined: AND + OR
    assert !matcher.isMatch("bright dark color");      // combined: vetoed by \bdark\b
    assert matcher.isMatch("bright darken color");     // "darken" ≠ \bdark\b
}
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
    ProcessType.VARIANT_NORM_DELETE_NORMALIZE.getValue(),
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

## Error Handling

- **Construction** (`new SimpleMatcher(bytes)`): throws a `RuntimeException` if the JSON config is malformed or contains invalid `ProcessType` values. This is the only operation that can fail.
- **Matching** (`isMatch`, `process`, `batchIsMatch`, `batchProcess`): infallible once the matcher is built. These methods never throw.

## Contributing

Contributions to `matcher_java` are welcome! If you find a bug or have a feature request, please open an issue on the [GitHub repository](https://github.com/Lips7/Matcher). If you would like to contribute code, please fork the repository and submit a pull request.

## License

`matcher_java` is licensed under the MIT OR Apache-2.0 license.
