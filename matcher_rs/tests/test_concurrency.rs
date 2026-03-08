use matcher_rs::{ProcessType, SimpleMatcherBuilder};
use std::sync::Arc;
use std::thread;

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
    // This test targets the thread_local state matrix.
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
