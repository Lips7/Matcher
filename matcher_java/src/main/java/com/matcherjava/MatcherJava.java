package com.matcherjava;

import com.matcherjava.extensiontypes.SimpleResult;

/** Native entry points backed by the Rust matcher library. */
final class MatcherJava {

  static {
    try {
      System.loadLibrary("matcher_java");
    } catch (UnsatisfiedLinkError e) {
      throw new ExceptionInInitializerError(e);
    }
  }

  private MatcherJava() {}

  public static native String textProcess(int processType, byte[] textBytes);

  public static native String[] reduceTextProcess(int processType, byte[] textBytes);

  public static native long initSimpleMatcher(byte[] simpleTableBytes);

  public static native boolean simpleMatcherIsMatch(long matcherPtr, byte[] textBytes);

  public static native SimpleResult[] simpleMatcherProcess(long matcherPtr, byte[] textBytes);

  public static native SimpleResult simpleMatcherFindMatch(long matcherPtr, byte[] textBytes);

  public static native boolean[] simpleMatcherBatchIsMatch(
      long matcherPtr, byte[][] textsBytes);

  public static native SimpleResult[][] simpleMatcherBatchProcess(
      long matcherPtr, byte[][] textsBytes);

  public static native SimpleResult[] simpleMatcherBatchFindMatch(
      long matcherPtr, byte[][] textsBytes);

  public static native void dropSimpleMatcher(long matcherPtr);
}
