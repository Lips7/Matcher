package com.matcherjava;

/**
 * Access to native Rust library.
 */
public class MatcherJava {

  static {
    try {
      // First try loading from java.library.path
      System.loadLibrary("matcher_java");
    } catch (UnsatisfiedLinkError e) {
      System.err.println("Could not load matcher_java lib. Check java.library.path.");
      throw e;
    }
  }

  public static native String textProcess(int processType, byte[] textBytes);

  public static native String reduceTextProcess(int processType, byte[] textBytes);

  public static native long initSimpleMatcher(byte[] simpleTableBytes);

  public static native boolean simpleMatcherIsMatch(long matcherPtr, byte[] textBytes);

  public static native String simpleMatcherProcessAsString(long matcherPtr, byte[] textBytes);

  public static native void dropSimpleMatcher(long matcherPtr);
}
