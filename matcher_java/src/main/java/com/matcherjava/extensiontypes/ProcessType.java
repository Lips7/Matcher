package com.matcherjava.extensiontypes;

/**
 * Enumeration representing the different types of processing that can be
 * applied during matching.
 */
public enum ProcessType {
  NONE(0b00000001),
  FANJIAN(0b00000010),
  DELETE(0b00000100),
  NORMALIZE(0b00001000),
  DELETE_NORMALIZE(0b00001100),
  FANJIAN_DELETE_NORMALIZE(0b00001110),
  PINYIN(0b00010000),
  PINYIN_CHAR(0b00100000);

  private final int value;

  ProcessType(int value) {
    this.value = value;
  }

  public int getValue() {
    return value;
  }

  public String toString() {
    return String.valueOf(value);
  }
}