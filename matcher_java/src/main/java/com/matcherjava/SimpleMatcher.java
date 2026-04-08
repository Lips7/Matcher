package com.matcherjava;

import com.matcherjava.extensiontypes.SimpleResult;
import java.lang.ref.Cleaner;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.Arrays;
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
    SimpleResult[] results = MatcherJava.simpleMatcherProcess(matcherPtr, utf8Bytes(text));
    return results == null ? null : Arrays.asList(results);
  }

  /** Returns the first match for the input text, or null if nothing matches. */
  public SimpleResult findMatch(String text) {
    checkClosed();
    return MatcherJava.simpleMatcherFindMatch(matcherPtr, utf8Bytes(text));
  }

  /** Check multiple texts in a single native call. */
  public List<Boolean> batchIsMatch(List<String> texts) {
    checkClosed();
    boolean[] results = MatcherJava.simpleMatcherBatchIsMatch(matcherPtr, toBytesArray(texts));
    List<Boolean> list = new ArrayList<>(results.length);
    for (boolean b : results) {
      list.add(b);
    }
    return list;
  }

  /** Process multiple texts in a single native call. */
  public List<List<SimpleResult>> batchProcess(List<String> texts) {
    checkClosed();
    byte[][] bytes = toBytesArray(texts);
    SimpleResult[][] results = MatcherJava.simpleMatcherBatchProcess(matcherPtr, bytes);
    if (results == null) {
      return null;
    }
    List<List<SimpleResult>> out = new ArrayList<>(results.length);
    for (SimpleResult[] inner : results) {
      out.add(Arrays.asList(inner));
    }
    return out;
  }

  /** Find the first match for each text. Elements may be null when no match is found. */
  public List<SimpleResult> batchFindMatch(List<String> texts) {
    checkClosed();
    byte[][] bytes = toBytesArray(texts);
    SimpleResult[] results = MatcherJava.simpleMatcherBatchFindMatch(matcherPtr, bytes);
    return results == null ? null : Arrays.asList(results);
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

  private static byte[][] toBytesArray(List<String> texts) {
    byte[][] bytesArray = new byte[texts.size()][];
    for (int i = 0; i < texts.size(); i++) {
      bytesArray[i] = utf8Bytes(texts.get(i));
    }
    return bytesArray;
  }
}
