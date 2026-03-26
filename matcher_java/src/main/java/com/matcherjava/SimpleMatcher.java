package com.matcherjava;

import com.alibaba.fastjson.JSON;
import com.matcherjava.extensiontypes.SimpleResult;
import java.lang.ref.Cleaner;
import java.nio.charset.StandardCharsets;
import java.util.List;
import java.util.Objects;

/** High-level Java wrapper around the native simple matcher. */
public final class SimpleMatcher implements AutoCloseable {

  private static final Cleaner CLEANER = Cleaner.create();

  private long matcherPtr;
  private boolean closed;
  private final Cleaner.Cleanable cleanable;

  /** Creates a matcher from serialized table bytes. */
  public SimpleMatcher(byte[] simpleTableBytes) {
    matcherPtr = MatcherJava.initSimpleMatcher(
        Objects.requireNonNull(simpleTableBytes, "simpleTableBytes"));
    if (matcherPtr == 0) {
      throw new RuntimeException("Failed to initialize SimpleMatcher");
    }

    long ptr = matcherPtr;
    cleanable = CLEANER.register(this, () -> MatcherJava.dropSimpleMatcher(ptr));
  }

  /** Returns whether the input text matches any configured rule. */
  public boolean isMatch(String text) {
    checkClosed();
    return MatcherJava.simpleMatcherIsMatch(matcherPtr, utf8Bytes(text));
  }

  /** Returns all matches for the input text. */
  public List<SimpleResult> process(String text) {
    checkClosed();
    String json = MatcherJava.simpleMatcherProcessAsString(matcherPtr, utf8Bytes(text));

    if (json == null) {
      return null;
    }

    return JSON.parseArray(json, SimpleResult.class);
  }

  @Override
  public void close() {
    if (!closed) {
      cleanable.clean();
      matcherPtr = 0;
      closed = true;
    }
  }

  private void checkClosed() {
    if (closed) {
      throw new IllegalStateException("SimpleMatcher is already closed");
    }
  }

  private static byte[] utf8Bytes(String text) {
    return Objects.requireNonNull(text, "text").getBytes(StandardCharsets.UTF_8);
  }
}
