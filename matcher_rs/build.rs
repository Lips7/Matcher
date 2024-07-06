use std::collections::HashMap;
use std::env;
use std::fs::File;
use std::io::{Result, Write};

use daachorse::{
    CharwiseDoubleArrayAhoCorasick, CharwiseDoubleArrayAhoCorasickBuilder,
    MatchKind as DoubleArrayAhoCorasickMatchKind,
};

const FANJIAN: &str = include_str!("./str_conv/FANJIAN.txt");
const SYMBOL_NORM: &str = include_str!("./str_conv/SYMBOL-NORM.txt");
const NUM_NORM: &str = include_str!("./str_conv/NUM-NORM.txt");
const NORM: &str = include_str!("./str_conv/NORM.txt");
const PINYIN: &str = include_str!("./str_conv/PINYIN.txt");
const PINYIN_CHAR: &str = include_str!("./str_conv/PINYIN-CHAR.txt");

fn main() -> Result<()> {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=str_conv");

    #[cfg(feature = "prebuilt")]
    {
        let out_dir = env::var("OUT_DIR").unwrap();
        let process_str_conv_map = HashMap::from([
            ("fanjian", vec![FANJIAN]),
            ("normalize", vec![SYMBOL_NORM, NORM, NUM_NORM]),
            ("pinyin", vec![PINYIN]),
            ("pinyinchar", vec![PINYIN_CHAR]),
        ]);

        for simple_match_type_bit_str in ["fanjian", "normalize", "pinyin", "pinyinchar"] {
            let mut process_dict = HashMap::new();

            for str_conv_map in process_str_conv_map.get(simple_match_type_bit_str).unwrap() {
                process_dict.extend(str_conv_map.trim().lines().map(|pair_str| {
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

            let mut process_list_bin = File::create(format!(
                "{out_dir}/{simple_match_type_bit_str}_process_list.bin"
            ))?;
            process_list_bin.write_all(process_list.join("\n").as_bytes())?;

            let process_replace_list = process_dict
                .iter()
                .map(|(_, &val)| val)
                .collect::<Vec<&str>>();
            let mut process_replace_list_bin = File::create(format!(
                "{out_dir}/{simple_match_type_bit_str}_process_replace_list.bin"
            ))?;
            process_replace_list_bin.write_all(process_replace_list.join("\n").as_bytes())?;

            if ["fanjian", "pinyin", "pinyinchar"].contains(&simple_match_type_bit_str) {
                let matcher: CharwiseDoubleArrayAhoCorasick<u32> =
                    CharwiseDoubleArrayAhoCorasickBuilder::new()
                        .match_kind(DoubleArrayAhoCorasickMatchKind::Standard)
                        .build(&process_list)
                        .unwrap();
                let matcher_bytes = matcher.serialize();
                let mut matcher_bin = File::create(format!(
                    "{out_dir}/{simple_match_type_bit_str}_daachorse_charwise_u32_matcher.bin"
                ))?;
                matcher_bin.write_all(&matcher_bytes)?;
            }
        }
    }

    Ok(())
}
