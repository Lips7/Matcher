package com.matcher_java;

import com.alibaba.fastjson.JSON;
import com.matcher_java.extension_types.SimpleResult;
import java.lang.ref.Cleaner;
import java.nio.charset.StandardCharsets;
import java.util.List;

/**
 * A high-level wrapper for the SimpleMatcher native library.
 * Implements AutoCloseable to ensure native memory is freed.
 */
public class SimpleMatcher implements AutoCloseable {

    private static final Cleaner cleaner = Cleaner.create();

    private long matcherPtr;
    private boolean closed = false;
    private final Cleaner.Cleanable cleanable;

    public SimpleMatcher(byte[] simpleTableBytes) {
        this.matcherPtr = MatcherJava.initSimpleMatcher(simpleTableBytes);
        if (this.matcherPtr == 0) {
            throw new RuntimeException("Failed to initialize SimpleMatcher");
        }

        long ptr = this.matcherPtr;
        this.cleanable = cleaner.register(this, () -> {
            MatcherJava.dropSimpleMatcher(ptr);
        });
    }

    public boolean isMatch(String text) {
        checkClosed();
        return MatcherJava.simpleMatcherIsMatch(
            matcherPtr,
            text.getBytes(StandardCharsets.UTF_8)
        );
    }

    public List<SimpleResult> process(String text) {
        checkClosed();
        String json = MatcherJava.simpleMatcherProcessAsString(
                matcherPtr,
                text.getBytes(StandardCharsets.UTF_8)
        );
        
        if (json == null) return null;

        return JSON.parseArray(json, SimpleResult.class);
    }

    @Override
    public void close() {
        if (!closed) {
            cleanable.clean();
            matcherPtr = 0;
            closed = true;
        }
    }

    private void checkClosed() {
        if (closed) {
            throw new IllegalStateException("SimpleMatcher is already closed");
        }
    }
}
