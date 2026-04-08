package com.matcherjava;

import com.alibaba.fastjson.JSON;
import com.alibaba.fastjson.TypeReference;
import com.matcherjava.extensiontypes.SimpleResult;
import java.lang.ref.Cleaner;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
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

  /** Returns the first match for the input text, or null if nothing matches. */
  public SimpleResult findMatch(String text) {
    checkClosed();
    String json = MatcherJava.simpleMatcherFindMatchAsString(matcherPtr, utf8Bytes(text));
    if (json == null) {
      return null;
    }
    return JSON.parseObject(json, SimpleResult.class);
  }

  /** Check multiple texts in a single native call. */
  public List<Boolean> batchIsMatch(List<String> texts) {
    checkClosed();
    byte[][] bytesArray = new byte[texts.size()][];
    for (int i = 0; i < texts.size(); i++) {
      bytesArray[i] = utf8Bytes(texts.get(i));
    }
    boolean[] results = MatcherJava.simpleMatcherBatchIsMatch(matcherPtr, bytesArray);
    List<Boolean> list = new ArrayList<>(results.length);
    for (boolean b : results) {
      list.add(b);
    }
    return list;
  }

  /** Process multiple texts in a single native call. */
  public List<List<SimpleResult>> batchProcess(List<String> texts) {
    checkClosed();
    byte[][] bytesArray = new byte[texts.size()][];
    for (int i = 0; i < texts.size(); i++) {
      bytesArray[i] = utf8Bytes(texts.get(i));
    }
    String json = MatcherJava.simpleMatcherBatchProcessAsString(matcherPtr, bytesArray);
    if (json == null) {
      return null;
    }
    return JSON.parseObject(json, new TypeReference<List<List<SimpleResult>>>() {});
  }

  /** Find the first match for each text. Elements may be null when no match is found. */
  public List<SimpleResult> batchFindMatch(List<String> texts) {
    checkClosed();
    byte[][] bytesArray = new byte[texts.size()][];
    for (int i = 0; i < texts.size(); i++) {
      bytesArray[i] = utf8Bytes(texts.get(i));
    }
    String json = MatcherJava.simpleMatcherBatchFindMatchAsString(matcherPtr, bytesArray);
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
