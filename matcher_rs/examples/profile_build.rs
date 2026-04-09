//! Profiling target: SimpleMatcher::new() construction hot loop.
//!
//! Attach Instruments / perf to this binary for flame graphs of the build
//! phase.
//!
//! ```sh
//! # Default: 10K English literal rules, 10s
//! cargo run --profile profiling --example profile_build -p matcher_rs
//!
//! # Custom:
//! cargo run --profile profiling --example profile_build -p matcher_rs -- \
//!     --dict cn --rules 50000 --pt variant_norm --seconds 15
//! ```

#[path = "../benches/common/mod.rs"]
mod common;

use std::{
    collections::HashMap,
    env,
    hint::black_box,
    time::{Duration, Instant},
};

use common::{build_literal_map, parse_process_type};
use matcher_rs::SimpleMatcher;

fn main() {
    let args: Vec<String> = env::args().collect();

    let mut dict = env::var("DICT").unwrap_or_else(|_| "en".into());
    let mut rules: usize = env::var("RULES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(10_000);
    let mut pt_str = env::var("PT").unwrap_or_else(|_| "none".into());
    let mut seconds: u64 = env::var("SECONDS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(10);

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--dict" => {
                dict = args[i + 1].clone();
                i += 2;
            }
            "--rules" => {
                rules = args[i + 1].parse().unwrap();
                i += 2;
            }
            "--pt" => {
                pt_str = args[i + 1].clone();
                i += 2;
            }
            "--seconds" => {
                seconds = args[i + 1].parse().unwrap();
                i += 2;
            }
            other => panic!("Unknown arg: {other}. Use: --dict, --rules, --pt, --seconds"),
        }
    }

    let pt = parse_process_type(&pt_str);

    println!("profile_build: rules={rules}, dict={dict}, pt={pt}, seconds={seconds}");

    let map = build_literal_map(&dict, rules, true);
    let table = HashMap::from([(pt, map)]);

    println!("  table ready, starting build loop...");

    let mut iterations: u64 = 0;
    let start = Instant::now();
    let deadline = start + Duration::from_secs(seconds);

    while Instant::now() < deadline {
        let matcher = black_box(SimpleMatcher::new(&table).unwrap());
        black_box(&matcher);
        drop(matcher);
        iterations += 1;
    }

    let elapsed = start.elapsed();
    let per_build_ms = elapsed.as_secs_f64() * 1000.0 / iterations as f64;

    println!("  iterations: {iterations}");
    println!("  per-build: {per_build_ms:.2} ms");
    println!(
        "  throughput: {:.1} builds/s",
        iterations as f64 / elapsed.as_secs_f64()
    );
}
