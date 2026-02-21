package com.matcher_java;

import com.sun.jna.Pointer;
import com.alibaba.fastjson.JSON;
import com.matcher_java.extension_types.SimpleResult;
import java.util.List;

/**
 * A high-level wrapper for the SimpleMatcher native library.
 * Implements AutoCloseable to ensure native memory is freed.
 */
public class SimpleMatcher implements AutoCloseable {
    private Pointer matcherPtr;
    private boolean closed = false;

    public SimpleMatcher(byte[] simpleTableBytes) {
        this.matcherPtr = MatcherJava.INSTANCE.init_simple_matcher(simpleTableBytes);
        if (this.matcherPtr == null) {
            throw new RuntimeException("Failed to initialize SimpleMatcher");
        }
    }

    public boolean isMatch(String text) {
        checkClosed();
        return MatcherJava.INSTANCE.simple_matcher_is_match(matcherPtr, text.getBytes());
    }

    public List<SimpleResult> process(String text) {
        checkClosed();
        Pointer resultPtr = MatcherJava.INSTANCE.simple_matcher_process_as_string(matcherPtr, text.getBytes());
        if (resultPtr == null) return null;

        try {
            String json = resultPtr.getString(0);
            return JSON.parseArray(json, SimpleResult.class);
        } finally {
            MatcherJava.INSTANCE.drop_string(resultPtr);
        }
    }

    @Override
    public void close() {
        if (!closed && matcherPtr != null) {
            MatcherJava.INSTANCE.drop_simple_matcher(matcherPtr);
            matcherPtr = null;
            closed = true;
        }
    }

    private void checkClosed() {
        if (closed) {
            throw new IllegalStateException("SimpleMatcher is already closed");
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
