package com.matcherjava;

import com.alibaba.fastjson.JSON;
import com.matcherjava.extensiontypes.SimpleResult;
import java.lang.ref.Cleaner;
import java.nio.charset.StandardCharsets;
import java.util.List;

/**
 * A high-level wrapper for the SimpleMatcher native library.
 * Implements AutoCloseable to ensure native memory is freed.
 */
public class SimpleMatcher implements AutoCloseable {

  private static final Cleaner cleaner = Cleaner.create();

  private long matcherPtr;
  private boolean closed = false;
  private final Cleaner.Cleanable cleanable;

  /**
   * Constructs a SimpleMatcher using the provided compiled table bytes.
   *
   * @param simpleTableBytes The compiled table bytes representing the matcher
   *                         state.
   */
  public SimpleMatcher(byte[] simpleTableBytes) {
    this.matcherPtr = MatcherJava.initSimpleMatcher(simpleTableBytes);
    if (this.matcherPtr == 0) {
      throw new RuntimeException("Failed to initialize SimpleMatcher");
    }

    long ptr = this.matcherPtr;
    this.cleanable = cleaner.register(this, () -> {
      MatcherJava.dropSimpleMatcher(ptr);
    });
  }

  /**
   * Checks if the given text matches the matcher's patterns.
   *
   * @param text The text to evaluate.
   * @return true if a match is found, otherwise false.
   */
  public boolean isMatch(String text) {
    checkClosed();
    return MatcherJava.simpleMatcherIsMatch(
        matcherPtr,
        text.getBytes(StandardCharsets.UTF_8));
  }

  /**
   * Processes the given text and returns a list of matching results.
   *
   * @param text The text to process.
   * @return A list of {@link SimpleResult} containing matched information, or
   *         null if no match or error.
   */
  public List<SimpleResult> process(String text) {
    checkClosed();
    String json = MatcherJava.simpleMatcherProcessAsString(
        matcherPtr,
        text.getBytes(StandardCharsets.UTF_8));

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
}
