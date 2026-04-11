# Matcher Java JNI Bindings

![GitHub Actions Workflow Status](https://img.shields.io/github/actions/workflow/status/lips7/Matcher/test.yml)
![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)

Java JNI bindings for the [Matcher](https://github.com/Lips7/Matcher) library — a high-performance matcher designed to solve **LOGICAL** and **TEXT VARIATIONS** problems in word matching, implemented in Rust.

For detailed implementation, see the [Design Document](../DESIGN.md).

**Requirements:** Java 21+, Maven. Match results are returned as typed Java objects directly via JNI (no JSON serialization on the hot path).

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
<version>0.15.1</version>
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

The high-level `SimpleMatcher` class provides a safe, idiomatic API and handles native memory management via `AutoCloseable`. Use the builder to construct:

```java
import com.matcherjava.SimpleMatcher;
import com.matcherjava.SimpleMatcherBuilder;
import com.matcherjava.extensiontypes.ProcessType;
import com.matcherjava.extensiontypes.SimpleResult;

try (SimpleMatcher matcher = new SimpleMatcherBuilder()
    .add(ProcessType.NONE, 1, "hello&world")
    .add(ProcessType.DELETE, 2, "你好")
    .build()) {

    boolean matched = matcher.isMatch("hello,world");

    List<SimpleResult> results = matcher.process("hello,world");

    SimpleResult first = matcher.findMatch("hello,world"); // first match, or null
}
// matcher.close() is called automatically here
```

You can also construct from raw JSON bytes if you already have them:

```java
String json = """
    {"1":{"1":"hello&world"}}""";
try (SimpleMatcher matcher = new SimpleMatcher(json.getBytes())) {
    assert matcher.isMatch("hello,world");
}
```

### Batch Operations

Process multiple texts in a single native call to avoid per-text JNI overhead:

```java
try (SimpleMatcher matcher = new SimpleMatcherBuilder()
    .add(ProcessType.NONE, 1, "hello")
    .add(ProcessType.NONE, 2, "world")
    .build()) {
    List<String> texts = Arrays.asList("hello world", "no match", "hello");

    List<Boolean> matches = matcher.batchIsMatch(texts);
    // [true, false, true]

    List<List<SimpleResult>> results = matcher.batchProcess(texts);

    List<SimpleResult> firstMatches = matcher.batchFindMatch(texts);
}
```

### Spark / Serialization

`SimpleMatcher` implements `Serializable`. On deserialization, the native matcher is automatically reconstructed from stored config bytes. This enables Spark broadcast variables and closure capture:

```java
// Driver
SimpleMatcher matcher = new SimpleMatcherBuilder()
    .add(ProcessType.NONE, 1, "hello&world")
    .build();
Broadcast<SimpleMatcher> bc = sc.broadcast(matcher);

// Executors — native matcher is reconstructed per executor
rdd.map(text -> bc.value().isMatch(text));
```

`SimpleResult` is also `Serializable` (a Java record), so match results flow through Spark shuffle/collect without custom serializers.

### Composing ProcessTypes

Use `ProcessType.or()`, `ProcessType.combine()`, or raw bitwise OR to create composite flags:

```java
int deleteNormalize = ProcessType.or(ProcessType.DELETE, ProcessType.NORMALIZE);
int custom = ProcessType.combine(ProcessType.VARIANT_NORM, ProcessType.DELETE, ProcessType.NORMALIZE);
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

Use the static methods on `SimpleMatcher` to apply transformations without a matcher:

```java
import com.matcherjava.SimpleMatcher;
import com.matcherjava.extensiontypes.ProcessType;

// Apply a single transformation
String normalized = SimpleMatcher.textProcess(
    ProcessType.NORMALIZE.getValue(), "Ｈｅｌｌｏ");

// Get all intermediate transformation variants
String[] variants = SimpleMatcher.reduceTextProcess(
    ProcessType.VARIANT_NORM_DELETE_NORMALIZE.getValue(), "你好，世界！");
```

## Error Handling

- **Construction** (`new SimpleMatcher(bytes)` or `builder.build()`): throws a `RuntimeException` if the JSON config is malformed or contains invalid `ProcessType` values. This is the only operation that can fail.
- **Matching** (`isMatch`, `process`, `findMatch`, `batchIsMatch`, `batchProcess`, `batchFindMatch`): infallible once the matcher is built. These methods never throw.

## Contributing

Contributions to `matcher_java` are welcome! If you find a bug or have a feature request, please open an issue on the [GitHub repository](https://github.com/Lips7/Matcher). If you would like to contribute code, please fork the repository and submit a pull request.

## License

`matcher_java` is licensed under the MIT OR Apache-2.0 license.
