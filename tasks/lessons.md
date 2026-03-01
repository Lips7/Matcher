# Lessons Learned

## Vec vs HashMap for sparse per-call data
**Context**: Replaced `HashMap<u32, Vec<Vec<i32>>>` with `Vec<Option<Vec<Vec<i32>>>>` pre-allocated to word_count in SimpleMatcher's inner loop.

**Problem**: For 10,000 patterns, `vec![None; 10000]` allocates 240KB per call (24 bytes × 10,000 entries). When called millions of times (once per text line × 200 iterations), the `memset` zeroing cost outweighs the hashing savings.

**Rule**: Use pre-allocated Vec only when the hit rate is high (most slots will be filled). For sparse data (few matches per call out of many possible), HashMap is better despite hashing overhead. For persistent lookup structures (built once, read many times), Vec is always superior.

## Profiling on macOS
- Use `samply` with a custom `[profile.profiling]` (inherits release, `debug = true`, `strip = "none"`)
- Don't use `cargo flamegraph` — it has macOS compatibility issues
- The `pprof` crate has symbolication issues on macOS ARM
- Always verify wall-clock timing with `time` alongside profiler %ages — profiler overhead can skew results
