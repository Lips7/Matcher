package com.matcher_java.extension_types;

public enum ProcessType {
    MatchNone(0b00000001),
    MatchFanjian(0b00000010),
    MatchDelete(0b00000100),
    MatchNormalize(0b00001000),
    MatchDeleteNormalize(0b00001100),
    MatchFanjianDeleteNormalize(0b00001110),
    MatchPinYin(0b00010000),
    MatchPinYinChar(0b00100000);

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