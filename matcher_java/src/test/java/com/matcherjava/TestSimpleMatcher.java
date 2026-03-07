package com.matcherjava;

import com.alibaba.fastjson.JSON;
import com.alibaba.fastjson.serializer.SerializeConfig;
import com.matcherjava.extensiontypes.ProcessType;
import com.matcherjava.extensiontypes.ProcessTypeSerializer;
import com.matcherjava.extensiontypes.SimpleResult;

import org.junit.Test;
import static org.junit.Assert.*;

import java.io.IOException;
import java.util.HashMap;
import java.util.List;
import java.util.Map;

public class TestSimpleMatcher {

  @Test
  public void testSimpleMatcher() throws IOException {
    SerializeConfig config = new SerializeConfig();
    config.put(ProcessType.class, new ProcessTypeSerializer());

    Map<ProcessType, Map<String, String>> simpleTable = new HashMap<>();
    Map<String, String> wordMap = new HashMap<>();
    wordMap.put("1", "hello&world");
    simpleTable.put(ProcessType.MatchNone, wordMap);

    byte[] configBytes = JSON.toJSONBytes(simpleTable, config);

    try (SimpleMatcher matcher = new SimpleMatcher(configBytes)) {
      String text = "hello,world";

      boolean isMatch = matcher.isMatch(text);
      assertTrue(isMatch);

      List<SimpleResult> results = matcher.process(text);
      assertNotNull(results);
      assertEquals(1, results.size());
      assertEquals("hello&world", results.get(0).getWord());
      assertEquals(1, results.get(0).getWordId());
    }
  }
}
