use std::io::Result;

/// The `main` function serves as the build script for the `matcher_rs` project.
/// Its primary responsibility is to transform raw text transformation rules (from the `process_map` directory)
/// into highly optimized, high-performance binary structures for text processing.
///
/// ### Binary Generation Strategy:
/// 1. **Normalize (Complex Rules)**:
///    Rules in `NORM.txt` and `NUM-NORM.txt` contain multi-character sequences and overlapping patterns
///    (e.g., Unicode combining marks). These are compiled into a `daachorse` Double-Array Aho-Corasick
///    state machine, which supports aggressive leftmost-longest matching.
///
/// 2. **Fanjian (Traditional to Simplified Chinese)**:
///    Since these are 1-to-1 character mappings, they are compiled into a **2-Stage Page Table**.
///    - `L1`: A page directory mapping character blocks to `L2` indices.
///    - `L2`: A data array containing the target character code points.
///      This allows $O(1)$ character conversion via direct memory indexing.
///
/// 3. **Pinyin & PinyinChar**:
///    Character-to-string mappings are stored using a hybrid structure:
///    - A **Concatenated String Buffer**: Stores all Pinyin strings as a single UTF-8 block.
///    - A **2-Stage Page Table**: Maps character code points to a packed `u32` containing
///      both the `offset` into the string buffer and the `length` of the Pinyin string.
///
/// 4. **Text Delete (BitSet)**:
///    Deletion rules and whitespace are compiled into a **Global BitSet** (139KB) covering the
///    entire Unicode spectrum (`0` to `U+10FFFF`). Each bit represents whether a character
///    should be discarded during processing, enabling extremely fast, branchless filtering.
fn main() -> Result<()> {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=process_map");

    #[cfg(not(feature = "runtime_build"))]
    {
        use std::collections::{HashMap, HashSet};
        use std::env;
        use std::fs::File;
        use std::io::Write;

        #[cfg(not(feature = "dfa"))]
        use daachorse::{
            CharwiseDoubleArrayAhoCorasick, CharwiseDoubleArrayAhoCorasickBuilder,
            MatchKind as DoubleArrayAhoCorasickMatchKind,
        };

        const FANJIAN: &str = include_str!("./process_map/FANJIAN.txt");
        const NUM_NORM: &str = include_str!("./process_map/NUM-NORM.txt");
        const NORM: &str = include_str!("./process_map/NORM.txt");
        const PINYIN: &str = include_str!("./process_map/PINYIN.txt");
        const TEXT_DELETE: &str = include_str!("./process_map/TEXT-DELETE.txt");
        const WHITE_SPACE: &[&str] = &[
            "\u{0009}", "\u{000A}", "\u{000B}", "\u{000C}", "\u{000D}", "\u{0020}", "\u{0085}",
            "\u{00A0}", "\u{1680}", "\u{2000}", "\u{2001}", "\u{2002}", "\u{2003}", "\u{2004}",
            "\u{2005}", "\u{2006}", "\u{2007}", "\u{2008}", "\u{2009}", "\u{200A}", "\u{200D}",
            "\u{200F}", "\u{2028}", "\u{2029}", "\u{202F}", "\u{205F}", "\u{3000}",
        ];
        const UNICODE_BITSET_SIZE: usize = 0x110000 / 8;

        let out_dir = env::var("OUT_DIR").unwrap();

        // 1. Build Normalize (uses Daachorse due to multi-char overlaps)
        let mut normalize_map = HashMap::new();
        for process_map in [NORM, NUM_NORM] {
            normalize_map.extend(process_map.trim().lines().map(|pair_str| {
                let mut split = pair_str.split('\t');
                (
                    split.next().expect("missing key in normalization source"),
                    split.next().expect("missing value in normalization source"),
                )
            }));
        }
        normalize_map.retain(|&key, &mut value| key != value);

        let mut normalize_pairs: Vec<(&str, &str)> = normalize_map.into_iter().collect();
        normalize_pairs.sort_unstable_by_key(|&(k, _)| k);
        let normalize_patterns: Vec<&str> = normalize_pairs.iter().map(|&(k, _)| k).collect();
        let normalize_replacements: Vec<&str> = normalize_pairs.iter().map(|&(_, v)| v).collect();

        let mut pattern_file = File::create(format!("{out_dir}/normalize_process_list.bin"))?;
        pattern_file.write_all(normalize_patterns.join("\n").as_bytes())?;

        let mut replacement_file =
            File::create(format!("{out_dir}/normalize_process_replace_list.bin"))?;
        replacement_file.write_all(normalize_replacements.join("\n").as_bytes())?;

        #[cfg(not(feature = "dfa"))]
        {
            let matcher: CharwiseDoubleArrayAhoCorasick<u32> =
                CharwiseDoubleArrayAhoCorasickBuilder::new()
                    .match_kind(DoubleArrayAhoCorasickMatchKind::LeftmostLongest)
                    .build(&normalize_patterns)
                    .unwrap();
            let matcher_bytes = matcher.serialize();
            let mut matcher_bin = File::create(format!(
                "{out_dir}/normalize_daachorse_charwise_u32_matcher.bin"
            ))?;
            matcher_bin.write_all(&matcher_bytes)?;
        }

        // 2. Build Fanjian 2-stage flat array
        let mut fanjian_map = HashMap::new();
        for line in FANJIAN.trim().lines() {
            let mut split = line.split('\t');
            let k = split
                .next()
                .expect("missing key in FANJIAN.txt")
                .chars()
                .next()
                .unwrap() as u32;
            let v = split
                .next()
                .expect("missing value in FANJIAN.txt")
                .chars()
                .next()
                .unwrap() as u32;
            if k != v {
                fanjian_map.insert(k, v);
            }
        }
        build_2_stage_table(&fanjian_map, &format!("{out_dir}/fanjian"))?;

        // 3. Build Pinyin 2-stage flat array & string buffer
        let mut pinyin_map = HashMap::new();
        let mut pinyin_str_buffer = String::new();

        for line in PINYIN.trim().lines() {
            let mut split = line.split('\t');
            let k = split
                .next()
                .expect("missing key in PINYIN.txt")
                .chars()
                .next()
                .unwrap() as u32;
            let v = split.next().expect("missing value in PINYIN.txt");

            let offset = pinyin_str_buffer.len();
            pinyin_str_buffer.push_str(v);
            let length = v.len();
            assert!(
                length < 256,
                "pinyin string length {length} exceeds 8-bit packing limit for key U+{k:04X}"
            );

            // store offset << 8 | length
            let packed = ((offset as u32) << 8) | (length as u32);
            pinyin_map.insert(k, packed);
        }

        File::create(format!("{out_dir}/pinyin_str.bin"))?
            .write_all(pinyin_str_buffer.as_bytes())?;
        build_2_stage_table(&pinyin_map, &format!("{out_dir}/pinyin"))?;

        // 4. Build Text Delete BitSet
        let mut delete_bitset = vec![0u8; UNICODE_BITSET_SIZE];
        let mut process_set = HashSet::new();
        process_set.extend(TEXT_DELETE.trim().lines());
        process_set.extend(WHITE_SPACE);

        for &val in process_set.iter() {
            for c in val.chars() {
                let cp = c as usize;
                delete_bitset[cp / 8] |= 1 << (cp % 8);
            }
        }
        File::create(format!("{out_dir}/delete_bitset.bin"))?.write_all(&delete_bitset)?;
    }

    Ok(())
}

/// Generates a compact, 2-stage flat-array structure for character-based lookups.
///
/// This function constructs a page-based directory system for sparse Unicode mappings:
/// - **L1 Page Table**: 4352 elements (`0x10FFFF >> 8`) mapping character blocks to L2 segments.
/// - **L2 Data Table**: Dense arrays containing the actual mapping data (e.g., replacement code points).
///
/// This structure provides $O(1)$ lookup performance with a very small memory footprint,
/// making it ideal for large-scale character transformations like Pinyin or Fanjian.
#[cfg(not(feature = "runtime_build"))]
fn build_2_stage_table(
    map: &std::collections::HashMap<u32, u32>,
    prefix: &str,
) -> std::io::Result<()> {
    use std::fs::File;
    use std::io::Write;

    let mut pages = std::collections::HashSet::new();
    for &k in map.keys() {
        pages.insert(k >> 8);
    }

    let mut page_list: Vec<u32> = pages.into_iter().collect();
    page_list.sort_unstable();

    let mut l1 = vec![0u16; 4352]; // up to 0x10FFFF >> 8 = 0x10FF = 4351
    let mut l2 = vec![0u32; (page_list.len() + 1) * 256]; // page 0 is empty fallback

    for (i, &page) in page_list.iter().enumerate() {
        let l2_page_idx = (i + 1) as u16;
        l1[page as usize] = l2_page_idx;

        for char_idx in 0..256 {
            let cp = (page << 8) | char_idx;
            if let Some(&val) = map.get(&cp) {
                l2[(l2_page_idx as usize * 256) + char_idx as usize] = val;
            }
        }
    }

    let mut l1_bytes = Vec::with_capacity(l1.len() * 2);
    for val in l1 {
        l1_bytes.extend_from_slice(&val.to_le_bytes());
    }
    File::create(format!("{}_l1.bin", prefix))?.write_all(&l1_bytes)?;

    let mut l2_bytes = Vec::with_capacity(l2.len() * 4);
    for val in l2 {
        l2_bytes.extend_from_slice(&val.to_le_bytes());
    }
    File::create(format!("{}_l2.bin", prefix))?.write_all(&l2_bytes)?;

    Ok(())
}
