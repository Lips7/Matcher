package com.matcher_java;

import com.alibaba.fastjson.JSON;
import com.alibaba.fastjson.TypeReference;
import com.matcher_java.extension_types.MatchResult;
import com.sun.jna.Pointer;
import java.lang.ref.Cleaner;
import java.util.List;
import java.util.Map;

/**
 * A high-level wrapper for the Matcher native library.
 * Implements AutoCloseable to ensure native memory is freed.
 */
public class Matcher implements AutoCloseable {

    private static final Cleaner cleaner = Cleaner.create();

    private Pointer matcherPtr;
    private boolean closed = false;
    private final Cleaner.Cleanable cleanable;

    public Matcher(byte[] matchTableMapBytes) {
        this.matcherPtr = MatcherJava.INSTANCE.init_matcher(matchTableMapBytes);
        if (this.matcherPtr == null) {
            throw new RuntimeException("Failed to initialize Matcher");
        }

        Pointer ptr = this.matcherPtr;
        this.cleanable = cleaner.register(this, () -> {
            MatcherJava.INSTANCE.drop_matcher(ptr);
        });
    }

    public boolean isMatch(String text) {
        checkClosed();
        return MatcherJava.INSTANCE.matcher_is_match(
            matcherPtr,
            text.getBytes()
        );
    }

    public List<MatchResult> process(String text) {
        checkClosed();
        Pointer resultPtr = MatcherJava.INSTANCE.matcher_process_as_string(
            matcherPtr,
            text.getBytes()
        );
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
        Pointer resultPtr = MatcherJava.INSTANCE.matcher_word_match_as_string(
            matcherPtr,
            text.getBytes()
        );
        if (resultPtr == null) return null;

        try {
            String json = resultPtr.getString(0);
            // Using FastJSON to parse the map
            return JSON.parseObject(
                json,
                new TypeReference<Map<Integer, List<MatchResult>>>() {}
            );
        } finally {
            MatcherJava.INSTANCE.drop_string(resultPtr);
        }
    }

    @Override
    public void close() {
        if (!closed) {
            cleanable.clean();
            matcherPtr = null;
            closed = true;
        }
    }

    private void checkClosed() {
        if (closed) {
            throw new IllegalStateException("Matcher is already closed");
        }
    }
}
