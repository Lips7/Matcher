#[global_allocator]
static GLOBAL: mimalloc_rust::GlobalMiMalloc = mimalloc_rust::GlobalMiMalloc;

use hyperscan::{pattern, BlockDatabase, Builder, Matching};

fn main() {
    let pattern = pattern! {"test"; CASELESS | SOM_LEFTMOST};
    let db: BlockDatabase = pattern.build().unwrap();
    let scratch = db.alloc_scratch().unwrap();
    let mut matches = vec![];

    db.scan("some test data", &scratch, |id, from, to, _| {
        println!("found pattern #{} @ [{}, {})", id, from, to);

        matches.push(from..to);

        Matching::Continue
    })
    .unwrap();

    assert_eq!(matches, vec![5..9]);
}
