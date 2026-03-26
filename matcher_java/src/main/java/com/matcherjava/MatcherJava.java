package com.matcherjava;

/** Native entry points backed by the Rust matcher library. */
public final class MatcherJava {

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

  public static native String simpleMatcherProcessAsString(long matcherPtr, byte[] textBytes);

  public static native void dropSimpleMatcher(long matcherPtr);
}
