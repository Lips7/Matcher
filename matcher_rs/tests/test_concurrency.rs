use matcher_rs::{ProcessType, SimpleMatcherBuilder};
use std::sync::Arc;
use std::thread;

const _: () = {
    #[allow(dead_code)]
    fn assert_send_sync<T: Send + Sync>() {}
    #[allow(dead_code)]
    fn check() {
        assert_send_sync::<matcher_rs::SimpleMatcher>();
    }
};

#[test]
fn test_multithreaded_matching() {
    let matcher = Arc::new(
        SimpleMatcherBuilder::new()
            .add_word(ProcessType::None, 1, "apple")
            .add_word(ProcessType::None, 2, "banana")
            .add_word(ProcessType::Fanjian, 3, "你好")
            .build(),
    );

    let mut handles = Vec::new();

    for i in 0..10 {
        let matcher_clone = Arc::clone(&matcher);
        let handle = thread::spawn(move || {
            for _ in 0..100 {
                if i % 2 == 0 {
                    assert!(matcher_clone.is_match("I have an apple"));
                    assert!(matcher_clone.is_match("妳好")); // Traditional triggers Fanjian
                } else {
                    assert!(matcher_clone.is_match("banana split"));
                    let results = matcher_clone.process("apple banana 你好");
                    assert_eq!(results.len(), 3);
                }
            }
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.join().unwrap();
    }
}

#[test]
fn test_concurrent_same_state_matrix() {
    // Each thread should have its own state and not interfere with others.
    let matcher = Arc::new(
        SimpleMatcherBuilder::new()
            .add_word(ProcessType::None, 1, "a&b")
            .build(),
    );

    let h1 = thread::spawn({
        let matcher = Arc::clone(&matcher);
        move || {
            // This thread only sees "a", so it shouldn't match "a&b"
            for _ in 0..1000 {
                assert!(!matcher.is_match("a"));
            }
        }
    });

    let h2 = thread::spawn({
        let matcher = Arc::clone(&matcher);
        move || {
            // This thread sees both "a" and "b", so it SHOULD match "a&b"
            for _ in 0..1000 {
                assert!(matcher.is_match("a b"));
            }
        }
    });

    h1.join().unwrap();
    h2.join().unwrap();
}

#[test]
fn test_concurrent_different_matchers() {
    // Two matchers of very different sizes on different threads.
    let small = Arc::new(
        SimpleMatcherBuilder::new()
            .add_word(ProcessType::None, 1, "alpha")
            .add_word(ProcessType::None, 2, "beta")
            .build(),
    );

    let mut large_builder = SimpleMatcherBuilder::new();
    let mut large_words = Vec::new();
    for i in 0..500u32 {
        large_words.push(format!("pattern{i}"));
    }
    for (i, w) in large_words.iter().enumerate() {
        large_builder = large_builder.add_word(ProcessType::None, i as u32, w);
    }
    let large = Arc::new(large_builder.build());

    let h1 = thread::spawn({
        let m = Arc::clone(&small);
        move || {
            for _ in 0..500 {
                assert!(m.is_match("alpha"));
                assert!(!m.is_match("gamma"));
                assert_eq!(m.process("alpha beta").len(), 2);
            }
        }
    });

    let h2 = thread::spawn({
        let m = Arc::clone(&large);
        move || {
            for _ in 0..500 {
                assert!(m.is_match("pattern0"));
                assert!(m.is_match("pattern499"));
                assert!(!m.is_match("patternXXX"));
            }
        }
    });

    h1.join().unwrap();
    h2.join().unwrap();
}
