package com.matcherjava;

import com.matcherjava.extensiontypes.ProcessType;
import org.junit.Test;
import static org.junit.Assert.*;

import java.nio.charset.StandardCharsets;

public class TestMatcherJava {

  @Test
  public void testTextProcess() {
    String text = "A B 測試 Ａ １";

    int fanjianType = ProcessType.MatchFanjian.getValue();
    String result1 = MatcherJava.textProcess(fanjianType, text.getBytes(StandardCharsets.UTF_8));
    assertEquals("A B 测试 Ａ １", result1);

    int combinedType = ProcessType.MatchFanjianDeleteNormalize.getValue();
    String jsonVariants = MatcherJava.reduceTextProcess(combinedType, text.getBytes(StandardCharsets.UTF_8));
    assertEquals("[\"A B 測試 Ａ １\",\"A B 测试 Ａ １\",\"AB测试Ａ１\",\"ab测试a1\"]", jsonVariants);
  }
}
