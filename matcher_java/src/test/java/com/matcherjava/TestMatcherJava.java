package com.matcherjava;

import com.matcherjava.extensiontypes.ProcessType;
import org.junit.Test;
import static org.junit.Assert.*;

import java.nio.charset.StandardCharsets;

public class TestMatcherJava {

  @Test
  public void testTextProcessVariantNorm() {
    String text = "A B 測試 Ａ １";
    int variantNormType = ProcessType.VARIANT_NORM.getValue();
    String result = MatcherJava.textProcess(variantNormType, text.getBytes(StandardCharsets.UTF_8));
    assertEquals("A B 测试 Ａ １", result);
  }

  @Test
  public void testTextProcessNone() {
    String text = "Hello World";
    String result = MatcherJava.textProcess(
        ProcessType.NONE.getValue(), text.getBytes(StandardCharsets.UTF_8));
    assertEquals("Hello World", result);
  }

  @Test
  public void testTextProcessDelete() {
    String text = "你！好";
    String result = MatcherJava.textProcess(
        ProcessType.DELETE.getValue(), text.getBytes(StandardCharsets.UTF_8));
    assertEquals("你好", result);
  }

  @Test
  public void testTextProcessNormalize() {
    String text = "ＡＢ";
    String result = MatcherJava.textProcess(
        ProcessType.NORMALIZE.getValue(), text.getBytes(StandardCharsets.UTF_8));
    assertEquals("ab", result);
  }

  @Test
  public void testReduceTextProcess() {
    String text = "A B 測試 Ａ １";
    int combinedType = ProcessType.VARIANT_NORM_DELETE_NORMALIZE.getValue();
    String[] variants = MatcherJava.reduceTextProcess(
        combinedType, text.getBytes(StandardCharsets.UTF_8));
    assertArrayEquals(
        new String[] {"A B 測試 Ａ １", "A B 测试 Ａ １", "AB测试Ａ１", "ab测试a1"},
        variants);
  }

  @Test
  public void testReduceTextProcessComposite() {
    String text = "Hello World";
    int combo = ProcessType.or(ProcessType.DELETE, ProcessType.ROMANIZE);
    String[] variants = MatcherJava.reduceTextProcess(
        combo, text.getBytes(StandardCharsets.UTF_8));
    assertNotNull(variants);
    assertTrue(variants.length > 0);
  }

  @Test
  public void testReduceTextProcessEmpty() {
    String[] variants = MatcherJava.reduceTextProcess(
        ProcessType.NONE.getValue(), "".getBytes(StandardCharsets.UTF_8));
    assertNotNull(variants);
  }

  @Test
  public void testProcessTypeCombine() {
    int combo = ProcessType.combine(ProcessType.DELETE, ProcessType.NORMALIZE);
    assertEquals(ProcessType.DELETE_NORMALIZE.getValue(), combo);
  }

  @Test
  public void testProcessTypeOr() {
    int result = ProcessType.or(ProcessType.DELETE, ProcessType.NORMALIZE);
    assertEquals(ProcessType.DELETE_NORMALIZE.getValue(), result);
  }

  @Test
  public void testProcessTypeAnd() {
    int result = ProcessType.and(ProcessType.DELETE_NORMALIZE, ProcessType.DELETE);
    assertEquals(ProcessType.DELETE.getValue(), result);
  }
}
