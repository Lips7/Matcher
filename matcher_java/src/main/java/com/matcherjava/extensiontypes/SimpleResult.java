package com.matcherjava.extensiontypes;

import java.io.Serializable;

/** A single match result containing the rule identifier and the matched pattern. */
public record SimpleResult(int wordId, String word) implements Serializable {
  private static final long serialVersionUID = 1L;
}
