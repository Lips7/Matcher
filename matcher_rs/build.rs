use std::collections::{HashMap, HashSet};
use std::env;
use std::fs::File;
use std::io::{Result, Write};

/// Build script for `matcher_rs`.
///
/// Transforms raw text-map files in `process_map/` into pre-compiled binary structures
/// embedded at compile time by `constants.rs`.
///
/// ### Binary Generation Strategy:
/// 1. **Normalize (Single-Codepoint Replacements)**:
///    All entries in `NORM.txt` and `NUM-NORM.txt` are single-codepoint keys mapped to
///    replacement strings. Compiled into a **2-Stage Page Table** (same layout as Romanize):
///    - `L1`/`L2`: page-table mapping codepoints to packed `(offset << 8) | length`.
///    - A **Concatenated String Buffer**: stores all replacement strings as a single UTF-8 block.
///
/// 2. **VariantNorm (CJK Variant Normalization)**:
///    1-to-1 character mappings (Chinese Traditional→Simplified, Japanese Kyūjitai→Shinjitai,
///    half-width katakana→full-width, Korean Hanja→Hangul). Compiled into a **2-Stage Page Table**.
///    - `L1`: A page directory mapping character blocks to `L2` indices.
///    - `L2`: A data array containing the target character code points.
///
/// 3. **Romanize & RomanizeChar**:
///    Character-to-string mappings (Chinese Pinyin, Japanese kana Romaji, Korean RR) stored using:
///    - A **Concatenated String Buffer**: Stores all romanization strings as a single UTF-8 block.
///    - A **2-Stage Page Table**: Maps character code points to a packed `u32` containing
///      both the `offset` into the string buffer and the `length` of the string.
///      `RomanizeChar` trims boundary spaces after the table is decoded at runtime.
///
/// 4. **Text Delete (BitSet)**:
///    Delete-table codepoints are compiled into a **Global BitSet** (139 KB) covering the
///    Unicode range U+0000 to U+10FFFF. Each bit represents whether a character should be
///    discarded during processing.
fn main() -> Result<()> {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=process_map");

    const VARIANT_NORM: &str = include_str!("./process_map/VARIANT_NORM.txt");
    const NUM_NORM: &str = include_str!("./process_map/NUM-NORM.txt");
    const NORM: &str = include_str!("./process_map/NORM.txt");
    const ROMANIZE: &str = include_str!("./process_map/ROMANIZE.txt");
    const TEXT_DELETE: &str = include_str!("./process_map/TEXT-DELETE.txt");
    const UNICODE_BITSET_SIZE: usize = 0x110000 / 8;

    let out_dir = env::var("OUT_DIR").unwrap();

    // 1. Build Normalize 2-stage page table & string buffer
    let mut normalize_cp_map = HashMap::new();
    let mut normalize_str_buffer = String::new();

    for (source_name, process_map) in [("NORM.txt", NORM), ("NUM-NORM.txt", NUM_NORM)] {
        for (line_num, pair_str) in process_map.trim().lines().enumerate() {
            let mut split = pair_str.split('\t');
            let key = split.next().unwrap_or_else(|| {
                panic!(
                    "{source_name}:{}: missing key (line is empty)",
                    line_num + 1
                )
            });
            let value = split.next().unwrap_or_else(|| {
                panic!(
                    "{source_name}:{}: missing tab-separated value for key {key:?}",
                    line_num + 1
                )
            });
            if key == value {
                continue;
            }
            assert!(
                key.chars().count() == 1,
                "{source_name}:{}: key must be exactly one codepoint, got {key:?}",
                line_num + 1
            );
            let cp = key.chars().next().unwrap() as u32;
            let offset = normalize_str_buffer.len();
            normalize_str_buffer.push_str(value);
            let length = value.len();
            assert!(
                length < 256,
                "{source_name}:{}: replacement length {length} exceeds 8-bit packing limit for key U+{cp:04X}",
                line_num + 1
            );
            let packed = ((offset as u32) << 8) | (length as u32);
            normalize_cp_map.insert(cp, packed);
        }
    }

    File::create(format!("{out_dir}/normalize_str.bin"))?
        .write_all(normalize_str_buffer.as_bytes())?;
    build_2_stage_table(&normalize_cp_map, &format!("{out_dir}/normalize"))?;

    // 2. Build VariantNorm 2-stage flat array
    let mut variant_norm_map = HashMap::new();
    for (line_num, line) in VARIANT_NORM.trim().lines().enumerate() {
        let mut split = line.split('\t');
        let key = split.next().unwrap_or_else(|| {
            panic!(
                "VARIANT_NORM.txt:{}: missing key (line is empty)",
                line_num + 1
            )
        });
        let value = split.next().unwrap_or_else(|| {
            panic!(
                "VARIANT_NORM.txt:{}: missing tab-separated value for key {key:?}",
                line_num + 1
            )
        });
        assert!(
            key.chars().count() == 1,
            "VARIANT_NORM.txt:{}: key must be exactly one character, got {key:?}",
            line_num + 1
        );
        assert!(
            value.chars().count() == 1,
            "VARIANT_NORM.txt:{}: value must be exactly one character, got {value:?}",
            line_num + 1
        );
        let k = key.chars().next().unwrap() as u32;
        let v = value.chars().next().unwrap() as u32;
        if k != v {
            variant_norm_map.insert(k, v);
        }
    }
    build_2_stage_table(&variant_norm_map, &format!("{out_dir}/variant_norm"))?;

    // 3. Build Romanize 2-stage flat array & string buffer
    let mut romanize_map = HashMap::new();
    let mut romanize_str_buffer = String::new();

    for (line_num, line) in ROMANIZE.trim().lines().enumerate() {
        let mut split = line.split('\t');
        let key = split.next().unwrap_or_else(|| {
            panic!("ROMANIZE.txt:{}: missing key (line is empty)", line_num + 1)
        });
        assert!(
            key.chars().count() == 1,
            "ROMANIZE.txt:{}: key must be exactly one character, got {key:?}",
            line_num + 1
        );
        let k = key.chars().next().unwrap() as u32;
        let v = split.next().unwrap_or_else(|| {
            panic!(
                "ROMANIZE.txt:{}: missing tab-separated value for key {key:?}",
                line_num + 1
            )
        });
        assert!(
            !v.is_empty(),
            "ROMANIZE.txt:{}: value must not be empty for key U+{k:04X}",
            line_num + 1
        );

        let offset = romanize_str_buffer.len();
        romanize_str_buffer.push_str(v);
        let length = v.len();
        assert!(
            length < 256,
            "ROMANIZE.txt:{}: string length {length} exceeds 8-bit packing limit for key U+{k:04X}",
            line_num + 1
        );

        let packed = ((offset as u32) << 8) | (length as u32);
        romanize_map.insert(k, packed);
    }

    File::create(format!("{out_dir}/romanize_str.bin"))?
        .write_all(romanize_str_buffer.as_bytes())?;
    build_2_stage_table(&romanize_map, &format!("{out_dir}/romanize"))?;

    // 4. Build Text Delete BitSet
    let mut delete_bitset = vec![0u8; UNICODE_BITSET_SIZE];
    for (line_num, token) in TEXT_DELETE.trim().lines().enumerate() {
        let cp = parse_delete_codepoint(token, line_num + 1) as usize;
        delete_bitset[cp / 8] |= 1 << (cp % 8);
    }
    File::create(format!("{out_dir}/delete_bitset.bin"))?.write_all(&delete_bitset)?;

    Ok(())
}

fn parse_delete_codepoint(token: &str, line_num: usize) -> u32 {
    let hex = token.strip_prefix("U+").unwrap_or_else(|| {
        panic!("TEXT-DELETE.txt:{line_num}: expected U+XXXX format, got {token:?}")
    });
    u32::from_str_radix(hex, 16).unwrap_or_else(|e| {
        panic!("TEXT-DELETE.txt:{line_num}: invalid hex codepoint {token:?}: {e}")
    })
}

fn build_2_stage_table(map: &HashMap<u32, u32>, prefix: &str) -> std::io::Result<()> {
    let mut pages = HashSet::new();
    for &k in map.keys() {
        pages.insert(k >> 8);
    }

    let mut page_list: Vec<u32> = pages.into_iter().collect();
    page_list.sort_unstable();

    const L1_SIZE: usize = (0x10FFFF >> 8) + 1;
    let mut l1 = vec![0u16; L1_SIZE];
    let mut l2 = vec![0u32; (page_list.len() + 1) * 256];

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
    File::create(format!("{prefix}_l1.bin"))?.write_all(&l1_bytes)?;

    let mut l2_bytes = Vec::with_capacity(l2.len() * 4);
    for val in l2 {
        l2_bytes.extend_from_slice(&val.to_le_bytes());
    }
    File::create(format!("{prefix}_l2.bin"))?.write_all(&l2_bytes)?;

    Ok(())
}
