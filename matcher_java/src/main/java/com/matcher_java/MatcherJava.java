package com.matcher_java;

import com.sun.jna.Library;
import com.sun.jna.Native;
import com.sun.jna.Pointer;

interface MatcherJava extends Library {
    MatcherJava INSTANCE = (MatcherJava) Native.load(
            MatcherJava.class.getResource("/matcher_c.so").getPath(),
            MatcherJava.class);

    Pointer init_matcher(byte[] match_table_map_bytes);

    boolean matcher_is_match(Pointer matcher, byte[] text_bytes);

    Pointer matcher_process(Pointer matcher, byte[] text_bytes);

    Pointer matcher_word_match(Pointer matcher, byte[] text_bytes);

    void drop_matcher(Pointer matcher);

    Pointer init_simple_matcher(byte[] smt_word_map_bytes);

    boolean simple_matcher_is_match(Pointer simple_matcher, byte[] text_bytes);

    Pointer simple_matcher_process(Pointer simple_matcher, byte[] text_bytes);

    void drop_simple_matcher(Pointer simple_matcher);

    void drop_string(Pointer ptr);
}