package com.matcherjava;

import com.alibaba.fastjson.JSON;
import com.alibaba.fastjson.serializer.SerializeConfig;
import com.matcherjava.extensiontypes.ProcessType;
import com.matcherjava.extensiontypes.ProcessTypeSerializer;
import com.matcherjava.extensiontypes.SimpleResult;

import org.junit.Test;
import static org.junit.Assert.*;

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
    SerializeConfig config = new SerializeConfig();
    config.put(ProcessType.class, new ProcessTypeSerializer());
    Map<ProcessType, Map<String, String>> table = new HashMap<>();
    table.put(pt, wordMap);
    return JSON.toJSONBytes(table, config);
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
      assertEquals("hello&world", results.get(0).getWord());
      assertEquals(1, results.get(0).getWordId());
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
      assertEquals("测试", results.get(0).getWord());
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
    new SimpleMatcher(null);
  }

  @Test(expected = RuntimeException.class)
  public void testEmptyBytes() {
    new SimpleMatcher(new byte[0]);
  }

  @Test(expected = RuntimeException.class)
  public void testInvalidJson() {
    new SimpleMatcher("not json".getBytes());
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
      assertEquals("hello", results.get(2).get(0).getWord());
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
}
