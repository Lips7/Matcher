package com.matcherjava.extensiontypes;

/**
 * Enumeration representing the different types of processing that can be
 * applied during matching.
 */
public enum ProcessType {
  NONE(0b00000001),
  VARIANT_NORM(0b00000010),
  DELETE(0b00000100),
  NORMALIZE(0b00001000),
  DELETE_NORMALIZE(0b00001100),
  VARIANT_NORM_DELETE_NORMALIZE(0b00001110),
  ROMANIZE(0b00010000),
  ROMANIZE_CHAR(0b00100000),
  EMOJI_NORM(0b01000000);

  private final int value;

  ProcessType(int value) {
    this.value = value;
  }

  public int getValue() {
    return value;
  }

  /** Combine two process types with bitwise OR. */
  public static int or(ProcessType first, ProcessType second) {
    return first.value | second.value;
  }

  /** Intersect two process types with bitwise AND. */
  public static int and(ProcessType first, ProcessType second) {
    return first.value & second.value;
  }

  /** Combine any number of process types with bitwise OR. */
  public static int combine(ProcessType... types) {
    int result = 0;
    for (ProcessType pt : types) {
      result |= pt.value;
    }
    return result;
  }

  @Override
  public String toString() {
    return String.valueOf(value);
  }
}
