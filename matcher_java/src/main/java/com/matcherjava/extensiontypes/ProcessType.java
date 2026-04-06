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

  @Override
  public String toString() {
    return String.valueOf(value);
  }
}
