package com.matcherjava;

import com.matcherjava.extensiontypes.ProcessType;
import org.junit.Test;
import static org.junit.Assert.*;

import java.nio.charset.StandardCharsets;

public class TestMatcherJava {

  @Test
  public void testTextProcess() {
    String text = "A B 測試 Ａ １";

    int fanjianType = ProcessType.FANJIAN.getValue();
    String result1 = MatcherJava.textProcess(fanjianType, text.getBytes(StandardCharsets.UTF_8));
    assertEquals("A B 测试 Ａ １", result1);

    int combinedType = ProcessType.FANJIAN_DELETE_NORMALIZE.getValue();
    String[] variants = MatcherJava.reduceTextProcess(combinedType, text.getBytes(StandardCharsets.UTF_8));
    assertArrayEquals(new String[] { "A B 測試 Ａ １", "A B 测试 Ａ １", "AB测试Ａ１", "ab测试a1" }, variants);
    int deletePinyinCombo = ProcessType.DELETE.getValue() | ProcessType.PINYIN.getValue();
    String[] comboVariants = MatcherJava.reduceTextProcess(deletePinyinCombo, text.getBytes(StandardCharsets.UTF_8));
    assertNotNull("reduceTextProcess should not return null for valid composite flags", comboVariants);
    assertTrue("reduceTextProcess should return multiple intermediate variants", comboVariants.length > 0);
  }
}
