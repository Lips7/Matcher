package com.matcher_java;

import com.sun.jna.Pointer;
import com.alibaba.fastjson.JSON;
import com.matcher_java.extension_types.MatchResult;
import java.util.List;
import java.util.Map;

/**
 * A high-level wrapper for the Matcher native library.
 * Implements AutoCloseable to ensure native memory is freed.
 */
public class Matcher implements AutoCloseable {
    private Pointer matcherPtr;
    private boolean closed = false;

    public Matcher(byte[] matchTableMapBytes) {
        this.matcherPtr = MatcherJava.INSTANCE.init_matcher(matchTableMapBytes);
        if (this.matcherPtr == null) {
            throw new RuntimeException("Failed to initialize Matcher");
        }
    }

    public boolean isMatch(String text) {
        checkClosed();
        return MatcherJava.INSTANCE.matcher_is_match(matcherPtr, text.getBytes());
    }

    public List<MatchResult> process(String text) {
        checkClosed();
        Pointer resultPtr = MatcherJava.INSTANCE.matcher_process_as_string(matcherPtr, text.getBytes());
        if (resultPtr == null) return null;

        try {
            String json = resultPtr.getString(0);
            return JSON.parseArray(json, MatchResult.class);
        } finally {
            MatcherJava.INSTANCE.drop_string(resultPtr);
        }
    }

    public Map<Integer, List<MatchResult>> wordMatch(String text) {
        checkClosed();
        Pointer resultPtr = MatcherJava.INSTANCE.matcher_word_match_as_string(matcherPtr, text.getBytes());
        if (resultPtr == null) return null;

        try {
            String json = resultPtr.getString(0);
            // Using FastJSON to parse the map
            return (Map<Integer, List<MatchResult>>) (Object) JSON.parseObject(json, Map.class);
        } finally {
            MatcherJava.INSTANCE.drop_string(resultPtr);
        }
    }

    @Override
    public void close() {
        if (!closed && matcherPtr != null) {
            MatcherJava.INSTANCE.drop_matcher(matcherPtr);
            matcherPtr = null;
            closed = true;
        }
    }

    private void checkClosed() {
        if (closed) {
            throw new IllegalStateException("Matcher is already closed");
        }
    }

    @Override
    protected void finalize() throws Throwable {
        try {
            close();
        } finally {
            super.finalize();
        }
    }
}
