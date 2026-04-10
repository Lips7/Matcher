package com.matcherjava;

import com.matcherjava.extensiontypes.ProcessType;
import java.nio.charset.StandardCharsets;
import java.util.LinkedHashMap;
import java.util.Map;
import java.util.Objects;

/**
 * Fluent builder for constructing a {@link SimpleMatcher} without external JSON libraries.
 *
 * <pre>{@code
 * SimpleMatcher matcher = new SimpleMatcherBuilder()
 *     .add(ProcessType.NONE, 1, "hello&world")
 *     .add(ProcessType.DELETE, 2, "你好")
 *     .build();
 * }</pre>
 */
public final class SimpleMatcherBuilder {

  private final Map<Integer, Map<Integer, String>> table = new LinkedHashMap<>();

  /** Register a pattern under the given process type and word ID. */
  public SimpleMatcherBuilder add(ProcessType processType, int wordId, String word) {
    return add(processType.getValue(), wordId, word);
  }

  /** Register a pattern under a raw process type value and word ID. */
  public SimpleMatcherBuilder add(int processType, int wordId, String word) {
    Objects.requireNonNull(word, "word");
    table.computeIfAbsent(processType, k -> new LinkedHashMap<>()).put(wordId, word);
    return this;
  }

  /** Compile accumulated patterns into a {@link SimpleMatcher}. */
  public SimpleMatcher build() {
    if (table.isEmpty()) {
      throw new IllegalStateException("No entries added to builder");
    }
    return new SimpleMatcher(toJsonBytes());
  }

  /** Serializes the table to JSON bytes. Package-private for test access. */
  byte[] toJsonBytes() {
    StringBuilder sb = new StringBuilder("{");
    boolean firstOuter = true;
    for (var outer : table.entrySet()) {
      if (!firstOuter) {
        sb.append(',');
      }
      firstOuter = false;
      sb.append('"').append(outer.getKey()).append("\":{");
      boolean firstInner = true;
      for (var inner : outer.getValue().entrySet()) {
        if (!firstInner) {
          sb.append(',');
        }
        firstInner = false;
        sb.append('"').append(inner.getKey()).append("\":\"");
        escapeJson(sb, inner.getValue());
        sb.append('"');
      }
      sb.append('}');
    }
    sb.append('}');
    return sb.toString().getBytes(StandardCharsets.UTF_8);
  }

  private static void escapeJson(StringBuilder sb, String s) {
    for (int i = 0; i < s.length(); i++) {
      char c = s.charAt(i);
      switch (c) {
        case '"' -> sb.append("\\\"");
        case '\\' -> sb.append("\\\\");
        case '\n' -> sb.append("\\n");
        case '\r' -> sb.append("\\r");
        case '\t' -> sb.append("\\t");
        default -> {
          if (c < 0x20) {
            sb.append(String.format("\\u%04x", (int) c));
          } else {
            sb.append(c);
          }
        }
      }
    }
  }
}
