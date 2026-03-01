# SimpleMatcher Optimization — Round 2

## Tasks
- [ ] Replace `not_flags: Vec<bool>` with `HashSet<usize>` in `_word_match`
- [ ] Flatten `Vec<Vec<i32>>` to `Vec<i32>` with stride-based access
- [ ] Update `is_match_preprocessed` for flat matrix
- [ ] Update `process_preprocessed` for flat matrix
- [ ] Create `profiling/` workspace member with benchmark binary
- [ ] Add `profiling` to workspace members in root `Cargo.toml`
- [ ] Run `cargo test -p matcher_rs` to verify
- [ ] Benchmark before/after
