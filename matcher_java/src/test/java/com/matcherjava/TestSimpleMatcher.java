package com.matcherjava;

import com.matcherjava.extensiontypes.ProcessType;
import com.matcherjava.extensiontypes.SimpleResult;

import org.junit.Test;
import static org.junit.Assert.*;

import java.io.ByteArrayInputStream;
import java.io.ByteArrayOutputStream;
import java.io.ObjectInputStream;
import java.io.ObjectOutputStream;
import java.util.Arrays;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import java.util.concurrent.Future;
import java.util.ArrayList;

public class TestSimpleMatcher {

  private static byte[] buildTable(ProcessType pt, Map<String, String> wordMap) {
    SimpleMatcherBuilder builder = new SimpleMatcherBuilder();
    for (Map.Entry<String, String> entry : wordMap.entrySet()) {
      builder.add(pt, Integer.parseInt(entry.getKey()), entry.getValue());
    }
    return builder.toJsonBytes();
  }

  private static Map<String, String> words(String... pairs) {
    Map<String, String> map = new HashMap<>();
    for (int i = 0; i < pairs.length; i += 2) {
      map.put(pairs[i], pairs[i + 1]);
    }
    return map;
  }

  @Test
  public void testBasicMatchAndProcess() {
    byte[] table = buildTable(ProcessType.NONE, words("1", "hello&world"));
    try (SimpleMatcher matcher = new SimpleMatcher(table)) {
      assertTrue(matcher.isMatch("hello,world"));

      List<SimpleResult> results = matcher.process("hello,world");
      assertNotNull(results);
      assertEquals(1, results.size());
      assertEquals("hello&world", results.get(0).word());
      assertEquals(1, results.get(0).wordId());
    }
  }

  @Test
  public void testOrOperator() {
    byte[] table = buildTable(ProcessType.NONE, words("1", "cat|dog"));
    try (SimpleMatcher matcher = new SimpleMatcher(table)) {
      assertTrue(matcher.isMatch("cat"));
      assertTrue(matcher.isMatch("dog"));
      assertFalse(matcher.isMatch("bird"));
    }
  }

  @Test
  public void testNotOperator() {
    byte[] table = buildTable(ProcessType.NONE, words("1", "hello~world"));
    try (SimpleMatcher matcher = new SimpleMatcher(table)) {
      assertTrue(matcher.isMatch("hello"));
      assertFalse(matcher.isMatch("hello world"));
    }
  }

  @Test
  public void testEmptyText() {
    byte[] table = buildTable(ProcessType.NONE, words("1", "hello"));
    try (SimpleMatcher matcher = new SimpleMatcher(table)) {
      assertFalse(matcher.isMatch(""));
      List<SimpleResult> results = matcher.process("");
      assertNotNull(results);
      assertTrue(results.isEmpty());
    }
  }

  @Test
  public void testUnicodePatterns() {
    byte[] table = buildTable(ProcessType.VARIANT_NORM, words("1", "测试"));
    try (SimpleMatcher matcher = new SimpleMatcher(table)) {
      assertTrue(matcher.isMatch("測試"));
      assertTrue(matcher.isMatch("测试"));

      List<SimpleResult> results = matcher.process("测试");
      assertEquals(1, results.size());
      assertEquals("测试", results.get(0).word());
    }
  }

  @Test
  public void testCloseIdempotent() {
    byte[] table = buildTable(ProcessType.NONE, words("1", "hello"));
    SimpleMatcher matcher = new SimpleMatcher(table);
    matcher.close();
    matcher.close();
  }

  @Test(expected = IllegalStateException.class)
  public void testUseAfterClose() {
    byte[] table = buildTable(ProcessType.NONE, words("1", "hello"));
    SimpleMatcher matcher = new SimpleMatcher(table);
    matcher.close();
    matcher.isMatch("hello");
  }

  @Test(expected = NullPointerException.class)
  public void testNullBytes() {
    SimpleMatcher matcher = new SimpleMatcher(null);
    matcher.close();
  }

  @Test(expected = RuntimeException.class)
  public void testEmptyBytes() {
    SimpleMatcher matcher = new SimpleMatcher(new byte[0]);
    matcher.close();
  }

  @Test(expected = RuntimeException.class)
  public void testInvalidJson() {
    SimpleMatcher matcher = new SimpleMatcher("not json".getBytes());
    matcher.close();
  }

  @Test
  public void testBatchIsMatch() {
    byte[] table = buildTable(ProcessType.NONE, words("1", "hello", "2", "world"));
    try (SimpleMatcher matcher = new SimpleMatcher(table)) {
      List<Boolean> results = matcher.batchIsMatch(Arrays.asList("hello", "miss", "world", ""));
      assertEquals(Arrays.asList(true, false, true, false), results);
    }
  }

  @Test
  public void testBatchIsMatchEmpty() {
    byte[] table = buildTable(ProcessType.NONE, words("1", "hello"));
    try (SimpleMatcher matcher = new SimpleMatcher(table)) {
      List<Boolean> results = matcher.batchIsMatch(Arrays.asList());
      assertTrue(results.isEmpty());
    }
  }

  @Test
  public void testBatchProcess() {
    byte[] table = buildTable(ProcessType.NONE, words("1", "hello", "2", "world"));
    try (SimpleMatcher matcher = new SimpleMatcher(table)) {
      List<List<SimpleResult>> results =
          matcher.batchProcess(Arrays.asList("hello world", "miss", "hello"));
      assertEquals(3, results.size());
      assertEquals(2, results.get(0).size());
      assertTrue(results.get(1).isEmpty());
      assertEquals(1, results.get(2).size());
      assertEquals("hello", results.get(2).get(0).word());
    }
  }

  @Test
  public void testFindMatch() {
    byte[] table = buildTable(ProcessType.NONE, words("1", "hello", "2", "world"));
    try (SimpleMatcher matcher = new SimpleMatcher(table)) {
      SimpleResult result = matcher.findMatch("hello world");
      assertNotNull(result);
      assertTrue(result.wordId() == 1 || result.wordId() == 2);
    }
  }

  @Test
  public void testFindMatchNone() {
    byte[] table = buildTable(ProcessType.NONE, words("1", "hello"));
    try (SimpleMatcher matcher = new SimpleMatcher(table)) {
      assertNull(matcher.findMatch("goodbye"));
      assertNull(matcher.findMatch(""));
    }
  }

  @Test
  public void testFindMatchGeneral() {
    byte[] table = buildTable(ProcessType.NONE, words("1", "a&b"));
    try (SimpleMatcher matcher = new SimpleMatcher(table)) {
      SimpleResult result = matcher.findMatch("a and b");
      assertNotNull(result);
      assertEquals(1, result.wordId());
      assertEquals("a&b", result.word());
      assertNull(matcher.findMatch("a only"));
    }
  }

  @Test
  public void testBatchFindMatch() {
    byte[] table = buildTable(ProcessType.NONE, words("1", "hello", "2", "world"));
    try (SimpleMatcher matcher = new SimpleMatcher(table)) {
      List<SimpleResult> results =
          matcher.batchFindMatch(Arrays.asList("hello", "miss", "world", ""));
      assertEquals(4, results.size());
      assertNotNull(results.get(0));
      assertEquals(1, results.get(0).wordId());
      assertNull(results.get(1));
      assertNotNull(results.get(2));
      assertEquals(2, results.get(2).wordId());
      assertNull(results.get(3));
    }
  }

  @Test
  public void testBatchFindMatchEmpty() {
    byte[] table = buildTable(ProcessType.NONE, words("1", "hello"));
    try (SimpleMatcher matcher = new SimpleMatcher(table)) {
      List<SimpleResult> results = matcher.batchFindMatch(Arrays.asList());
      assertNotNull(results);
      assertTrue(results.isEmpty());
    }
  }

  @Test
  public void testConcurrentAccess() throws Exception {
    byte[] table = buildTable(ProcessType.VARIANT_NORM, words("1", "测试"));
    try (SimpleMatcher matcher = new SimpleMatcher(table)) {
      ExecutorService pool = Executors.newFixedThreadPool(4);
      List<Future<Boolean>> futures = new ArrayList<>();
      for (int i = 0; i < 100; i++) {
        futures.add(pool.submit(() -> matcher.isMatch("測試测试文本")));
      }
      pool.shutdown();
      for (Future<Boolean> f : futures) {
        assertTrue(f.get());
      }
    }
  }

  // --- Serialization tests ---

  @Test
  public void testSerializationRoundTrip() throws Exception {
    byte[] table = buildTable(ProcessType.NONE, words("1", "hello&world"));

    ByteArrayOutputStream baos = new ByteArrayOutputStream();
    try (SimpleMatcher original = new SimpleMatcher(table);
        ObjectOutputStream oos = new ObjectOutputStream(baos)) {
      oos.writeObject(original);
    }

    ByteArrayInputStream bais = new ByteArrayInputStream(baos.toByteArray());
    try (ObjectInputStream ois = new ObjectInputStream(bais);
        SimpleMatcher restored = (SimpleMatcher) ois.readObject()) {
      assertTrue(restored.isMatch("hello,world"));
      assertFalse(restored.isMatch("goodbye"));
      List<SimpleResult> results = restored.process("hello,world");
      assertEquals(1, results.size());
      assertEquals("hello&world", results.get(0).word());
    }
  }

  // --- Builder tests ---

  @Test
  public void testBuilderBasic() {
    try (SimpleMatcher matcher = new SimpleMatcherBuilder()
        .add(ProcessType.NONE, 1, "hello")
        .add(ProcessType.NONE, 2, "world")
        .build()) {
      assertTrue(matcher.isMatch("hello"));
      assertTrue(matcher.isMatch("world"));
      assertFalse(matcher.isMatch("miss"));
    }
  }

  @Test(expected = IllegalStateException.class)
  public void testBuilderEmpty() {
    new SimpleMatcherBuilder().build();
  }

  @Test
  public void testBuilderWithProcessTypes() {
    try (SimpleMatcher matcher = new SimpleMatcherBuilder()
        .add(ProcessType.VARIANT_NORM, 1, "测试")
        .build()) {
      assertTrue(matcher.isMatch("測試"));
    }
  }

  @Test
  public void testBuilderBackslashPattern() {
    try (SimpleMatcher matcher = new SimpleMatcherBuilder()
        .add(ProcessType.NONE, 1, "\\bcat\\b")
        .build()) {
      assertTrue(matcher.isMatch("the cat sat"));
      assertFalse(matcher.isMatch("concatenate"));
    }
  }

  @Test
  public void testBuilderSerializationRoundTrip() throws Exception {
    ByteArrayOutputStream baos = new ByteArrayOutputStream();
    try (SimpleMatcher original = new SimpleMatcherBuilder()
        .add(ProcessType.NONE, 1, "hello")
        .add(ProcessType.DELETE, 2, "你好")
        .build();
        ObjectOutputStream oos = new ObjectOutputStream(baos)) {
      oos.writeObject(original);
    }

    ByteArrayInputStream bais = new ByteArrayInputStream(baos.toByteArray());
    try (ObjectInputStream ois = new ObjectInputStream(bais);
        SimpleMatcher restored = (SimpleMatcher) ois.readObject()) {
      assertTrue(restored.isMatch("hello"));
      assertTrue(restored.isMatch("你！好"));
    }
  }
}
