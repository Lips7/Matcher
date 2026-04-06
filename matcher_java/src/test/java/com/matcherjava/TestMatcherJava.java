package com.matcherjava;

import com.matcherjava.extensiontypes.ProcessType;
import org.junit.Test;
import static org.junit.Assert.*;

import java.nio.charset.StandardCharsets;

public class TestMatcherJava {

  @Test
  public void testTextProcess() {
    String text = "A B 測試 Ａ １";

    int variantNormType = ProcessType.VARIANT_NORM.getValue();
    String result1 = MatcherJava.textProcess(variantNormType, text.getBytes(StandardCharsets.UTF_8));
    assertEquals("A B 测试 Ａ １", result1);

    int combinedType = ProcessType.VARIANT_NORM_DELETE_NORMALIZE.getValue();
    String[] variants = MatcherJava.reduceTextProcess(combinedType, text.getBytes(StandardCharsets.UTF_8));
    assertArrayEquals(new String[] { "A B 測試 Ａ １", "A B 测试 Ａ １", "AB测试Ａ１", "ab测试a1" }, variants);
    int deleteRomanizeCombo = ProcessType.DELETE.getValue() | ProcessType.ROMANIZE.getValue();
    String[] comboVariants = MatcherJava.reduceTextProcess(deleteRomanizeCombo, text.getBytes(StandardCharsets.UTF_8));
    assertNotNull("reduceTextProcess should not return null for valid composite flags", comboVariants);
    assertTrue("reduceTextProcess should return multiple intermediate variants", comboVariants.length > 0);
  }
}
