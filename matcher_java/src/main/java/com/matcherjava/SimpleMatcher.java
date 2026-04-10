package com.matcherjava;

import com.matcherjava.extensiontypes.SimpleResult;
import java.io.IOException;
import java.io.Serializable;
import java.lang.ref.Cleaner;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;
import java.util.Objects;

/**
 * High-level Java wrapper around the native simple matcher.
 *
 * <p>Implements {@link Serializable} so instances can be broadcast across Spark executors. On
 * deserialization the native matcher is reconstructed from the stored config bytes.
 */
public final class SimpleMatcher implements AutoCloseable, Serializable {

  private static final long serialVersionUID = 1L;
  private static final Cleaner CLEANER = Cleaner.create();

  private final byte[] configBytes;

  private transient long matcherPtr;
  private transient boolean closed;
  private transient Cleaner.Cleanable cleanable;

  /** Creates a matcher from serialized table bytes. */
  public SimpleMatcher(byte[] configBytes) {
    this.configBytes = Objects.requireNonNull(configBytes, "configBytes").clone();
    initNative();
  }

  private void initNative() {
    matcherPtr = MatcherJava.initSimpleMatcher(configBytes);
    if (matcherPtr == 0) {
      throw new RuntimeException("Failed to initialize SimpleMatcher");
    }
    long ptr = matcherPtr;
    cleanable = CLEANER.register(this, () -> MatcherJava.dropSimpleMatcher(ptr));
    closed = false;
  }

  // --- Static text processing utilities ---

  /** Applies the text transformation pipeline and returns the result. */
  public static String textProcess(int processType, String text) {
    return MatcherJava.textProcess(processType, utf8Bytes(text));
  }

  /** Applies the transformation pipeline, returning all intermediate variants. */
  public static String[] reduceTextProcess(int processType, String text) {
    return MatcherJava.reduceTextProcess(processType, utf8Bytes(text));
  }

  // --- Query methods ---

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

  // --- Batch methods ---

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

  // --- Lifecycle ---

  @Override
  public void close() {
    if (!closed) {
      cleanable.clean();
      matcherPtr = 0;
      closed = true;
    }
  }

  // --- Serialization ---

  @java.io.Serial
  private void writeObject(java.io.ObjectOutputStream out) throws IOException {
    out.defaultWriteObject();
  }

  @java.io.Serial
  private void readObject(java.io.ObjectInputStream in)
      throws IOException, ClassNotFoundException {
    in.defaultReadObject();
    initNative();
  }

  // --- Internal helpers ---

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
