package com.matcher_java;

import com.sun.jna.Library;
import com.sun.jna.Native;
import com.sun.jna.Pointer;

interface MatcherJava extends Library {
    MatcherJava INSTANCE = (MatcherJava) Native.load(
            "matcher_c",
            MatcherJava.class);

    Pointer init_simple_matcher(byte[] simple_table_bytes);

    boolean simple_matcher_is_match(Pointer simple_matcher, byte[] text_bytes);

    Pointer simple_matcher_process_as_string(Pointer simple_matcher, byte[] text_bytes);

    void drop_simple_matcher(Pointer simple_matcher);

    void drop_string(Pointer ptr);
}