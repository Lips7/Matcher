use std::io::Result;

/// The `main` function serves as the build script for a Rust project, responsible for
/// generating binary data files used in text conversion and matching tasks.
/// Depending on the features enabled, it reads specific conversion mappings from
/// text files, processes them, and writes them to binary files.
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

        let out_dir = env::var("OUT_DIR").unwrap();

        // 1. Build Normalize (uses Daachorse due to multi-char overlaps)
        let process_str_map = HashMap::from([("normalize", vec![NORM, NUM_NORM])]);

        for process_type_bit_str in ["normalize"] {
            let mut process_dict = HashMap::new();
            for process_map in process_str_map.get(process_type_bit_str).unwrap() {
                process_dict.extend(process_map.trim().lines().map(|pair_str| {
                    let mut pair_str_split = pair_str.split('\t');
                    (
                        pair_str_split.next().unwrap(),
                        pair_str_split.next().unwrap(),
                    )
                }))
            }

            process_dict.retain(|&key, &mut value| key != value);
            let process_list = process_dict
                .iter()
                .map(|(&key, _)| key)
                .collect::<Vec<&str>>();

            let mut process_list_bin =
                File::create(format!("{out_dir}/{process_type_bit_str}_process_list.bin"))?;
            process_list_bin.write_all(process_list.join("\n").as_bytes())?;

            let process_replace_list = process_dict
                .iter()
                .map(|(_, &val)| val)
                .collect::<Vec<&str>>();
            let mut process_replace_list_bin = File::create(format!(
                "{out_dir}/{process_type_bit_str}_process_replace_list.bin"
            ))?;
            process_replace_list_bin.write_all(process_replace_list.join("\n").as_bytes())?;

            #[cfg(not(feature = "dfa"))]
            if process_type_bit_str == "normalize" {
                let matcher: CharwiseDoubleArrayAhoCorasick<u32> =
                    CharwiseDoubleArrayAhoCorasickBuilder::new()
                        .match_kind(DoubleArrayAhoCorasickMatchKind::LeftmostLongest)
                        .build(&process_list)
                        .unwrap();
                let matcher_bytes = matcher.serialize();
                let mut matcher_bin = File::create(format!(
                    "{out_dir}/{process_type_bit_str}_daachorse_charwise_u32_matcher.bin"
                ))?;
                matcher_bin.write_all(&matcher_bytes)?;
            }
        }

        // 2. Build Fanjian 2-stage flat array
        let mut fanjian_map = HashMap::new();
        for line in FANJIAN.trim().lines() {
            let mut split = line.split('\t');
            let k = split.next().unwrap().chars().next().unwrap() as u32;
            let v = split.next().unwrap().chars().next().unwrap() as u32;
            if k != v {
                fanjian_map.insert(k, v);
            }
        }
        build_2_stage_table(&fanjian_map, &format!("{out_dir}/fanjian"));

        // 3. Build Pinyin 2-stage flat array & string buffer
        let mut pinyin_map = HashMap::new();
        let mut pinyin_str_buffer = String::new();

        for line in PINYIN.trim().lines() {
            let mut split = line.split('\t');
            let k = split.next().unwrap().chars().next().unwrap() as u32;
            let v = split.next().unwrap();

            let offset = pinyin_str_buffer.len();
            pinyin_str_buffer.push_str(v);
            let length = v.len();

            // store offset << 8 | length
            let packed = ((offset as u32) << 8) | (length as u32);
            pinyin_map.insert(k, packed);
        }

        File::create(format!("{out_dir}/pinyin_str.bin"))?
            .write_all(pinyin_str_buffer.as_bytes())?;
        build_2_stage_table(&pinyin_map, &format!("{out_dir}/pinyin"));

        // 4. Build Text Delete BitSet
        let mut delete_bitset = vec![0u8; 139264]; // 0x10FFFF / 8 + 1
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

#[cfg(not(feature = "runtime_build"))]
fn build_2_stage_table(map: &std::collections::HashMap<u32, u32>, prefix: &str) {
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
    File::create(format!("{}_l1.bin", prefix))
        .unwrap()
        .write_all(&l1_bytes)
        .unwrap();

    let mut l2_bytes = Vec::with_capacity(l2.len() * 4);
    for val in l2 {
        l2_bytes.extend_from_slice(&val.to_le_bytes());
    }
    File::create(format!("{}_l2.bin", prefix))
        .unwrap()
        .write_all(&l2_bytes)
        .unwrap();
}
