package com.matcherjava;

import com.matcherjava.extensiontypes.ProcessType;
import org.junit.Test;
import static org.junit.Assert.*;

public class TestMatcherJava {

  @Test
  public void testTextProcessVariantNorm() {
    String result = SimpleMatcher.textProcess(ProcessType.VARIANT_NORM.getValue(), "A B 測試 Ａ １");
    assertEquals("A B 测试 Ａ １", result);
  }

  @Test
  public void testTextProcessNone() {
    String result = SimpleMatcher.textProcess(ProcessType.NONE.getValue(), "Hello World");
    assertEquals("Hello World", result);
  }

  @Test
  public void testTextProcessDelete() {
    String result = SimpleMatcher.textProcess(ProcessType.DELETE.getValue(), "你！好");
    assertEquals("你好", result);
  }

  @Test
  public void testTextProcessNormalize() {
    String result = SimpleMatcher.textProcess(ProcessType.NORMALIZE.getValue(), "ＡＢ");
    assertEquals("ab", result);
  }

  @Test
  public void testReduceTextProcess() {
    int combinedType = ProcessType.VARIANT_NORM_DELETE_NORMALIZE.getValue();
    String[] variants = SimpleMatcher.reduceTextProcess(combinedType, "A B 測試 Ａ １");
    assertArrayEquals(
        new String[] {"A B 測試 Ａ １", "A B 测试 Ａ １", "AB测试Ａ１", "ab测试a1"},
        variants);
  }

  @Test
  public void testReduceTextProcessComposite() {
    int combo = ProcessType.or(ProcessType.DELETE, ProcessType.ROMANIZE);
    String[] variants = SimpleMatcher.reduceTextProcess(combo, "Hello World");
    assertNotNull(variants);
    assertTrue(variants.length > 0);
  }

  @Test
  public void testReduceTextProcessEmpty() {
    String[] variants = SimpleMatcher.reduceTextProcess(ProcessType.NONE.getValue(), "");
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
