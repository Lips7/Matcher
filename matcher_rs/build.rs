use std::io::Result;

/// The `main` function serves as the build script for a Rust project, responsible for
/// generating binary data files used in text conversion and matching tasks.
/// Depending on the features enabled, it reads specific conversion mappings from
/// text files, processes them, and writes them to binary files.
///
/// It comprises several key steps:
///
/// 1. Print instructions to re-run build script if specific files change.
/// 2. Conditionally process text conversion data only if 'runtime_build' feature is not enabled.
/// 3. Load text content from files in the 'process_map' directory into constants like FANJIAN, NUM_NORM, NORM, and PINYIN.
/// 4. For each mapping type ('fanjian', 'normalize', 'pinyin'):
///     - Aggregate conversion mappings from loaded constants into a HashMap.
///     - Clean the HashMap by removing identity mappings.
///     - Create binary files containing the list of strings to match and the list of corresponding replacements.
///     - For 'pinyin':
///         - Also create a binary file with trimmed replacements.
///     - For specified mappings ('fanjian', 'pinyin'):
///         - Use the `daachorse` crate to build and serialize a CharwiseDoubleArrayAhoCorasick matcher, and write it to a binary file.
///     - For 'normalize', when DFA feature is not enabled:
///         - Similarly, build a matcher with a different match kind and serialize it.
/// 5. Additionally, if 'dfa' feature is not enabled:
///     - Load delete and whitespace character patterns from TEXT_DELETE constant and WHITE_SPACE array respectively.
///     - Aggregate these patterns into a HashSet to remove duplicates.
///     - Write these patterns to a binary file.
///     - Build a matcher for these patterns, serialize it, and write it to a binary file.
///
/// The function completes by returning `Ok(())` to indicate successful completion of the build script.
fn main() -> Result<()> {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=process_map");

    #[cfg(not(feature = "runtime_build"))]
    {
        use std::collections::HashMap;
        use std::env;
        use std::fs::File;
        use std::io::Write;

        use daachorse::{
            CharwiseDoubleArrayAhoCorasick, CharwiseDoubleArrayAhoCorasickBuilder,
            MatchKind as DoubleArrayAhoCorasickMatchKind,
        };

        /// These constants include the contents of their respective text files
        /// from the `process_map` directory. Each constant refers to a specific
        /// text conversion mapping used within the project. The text files
        /// contain tab-separated values, where each line represents a pair of
        /// strings that define a specific conversion.
        ///
        /// - `FANJIAN` includes simplified and traditional Chinese character mappings.
        /// - `NUM_NORM` includes mappings for normalizing numbers.
        /// - `NORM` includes mappings for various normalization forms.
        /// - `PINYIN` includes mappings for converting characters to Pinyin.
        const FANJIAN: &str = include_str!("./process_map/FANJIAN.txt");
        const NUM_NORM: &str = include_str!("./process_map/NUM-NORM.txt");
        const NORM: &str = include_str!("./process_map/NORM.txt");
        const PINYIN: &str = include_str!("./process_map/PINYIN.txt");

        let out_dir = env::var("OUT_DIR").unwrap();
        let process_str_map = HashMap::from([
            ("fanjian", vec![FANJIAN]),
            ("normalize", vec![NORM, NUM_NORM]),
            ("pinyin", vec![PINYIN]),
        ]);

        for process_type_bit_str in ["fanjian", "normalize", "pinyin"] {
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

            if process_type_bit_str == "pinyin" {
                let process_replace_list = process_dict
                    .iter()
                    .map(|(_, &val)| val.trim_matches(' '))
                    .collect::<Vec<&str>>();
                let mut process_replace_list_bin =
                    File::create(format!("{out_dir}/pinyinchar_process_replace_list.bin"))?;
                process_replace_list_bin.write_all(process_replace_list.join("\n").as_bytes())?;
            }

            if ["fanjian", "pinyin"].contains(&process_type_bit_str) {
                let matcher: CharwiseDoubleArrayAhoCorasick<u32> =
                    CharwiseDoubleArrayAhoCorasickBuilder::new()
                        .match_kind(DoubleArrayAhoCorasickMatchKind::Standard)
                        .build(&process_list)
                        .unwrap();
                let matcher_bytes = matcher.serialize();
                let mut matcher_bin = File::create(format!(
                    "{out_dir}/{process_type_bit_str}_daachorse_charwise_u32_matcher.bin"
                ))?;
                matcher_bin.write_all(&matcher_bytes)?;
            }

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

        #[cfg(not(feature = "dfa"))]
        {
            use std::collections::HashSet;

            /// These constants define deletion and whitespace character mappings
            /// that are used within the project. The `TEXT_DELETE` constant
            /// includes contents from the `TEXT-DELETE.txt` file in the `process_map`
            /// directory, which contains textual patterns to be deleted.
            /// The `WHITE_SPACE` constant includes various Unicode whitespace
            /// characters that are treated as whitespace in the project's text
            /// processing logic.
            ///
            /// - `TEXT_DELETE` includes patterns of text identified for deletion.
            /// - `WHITE_SPACE` includes numerous Unicode representations of whitespace.
            const TEXT_DELETE: &str = include_str!("./process_map/TEXT-DELETE.txt");
            const WHITE_SPACE: &[&str] = &[
                "\u{0009}", "\u{000A}", "\u{000B}", "\u{000C}", "\u{000D}", "\u{0020}", "\u{0085}",
                "\u{00A0}", "\u{1680}", "\u{2000}", "\u{2001}", "\u{2002}", "\u{2003}", "\u{2004}",
                "\u{2005}", "\u{2006}", "\u{2007}", "\u{2008}", "\u{2009}", "\u{200A}", "\u{200D}",
                "\u{200F}", "\u{2028}", "\u{2029}", "\u{202F}", "\u{205F}", "\u{3000}",
            ];

            let mut process_set = HashSet::new();

            process_set.extend(TEXT_DELETE.trim().lines().map(|line| line));
            process_set.extend(WHITE_SPACE);

            let process_list = process_set.iter().map(|&s| s).collect::<Vec<&str>>();

            let mut process_list_bin = File::create(format!("{out_dir}/delete_process_list.bin"))?;
            process_list_bin.write_all(process_list.join("\n").as_bytes())?;

            let matcher: CharwiseDoubleArrayAhoCorasick<u32> =
                CharwiseDoubleArrayAhoCorasickBuilder::new()
                    .match_kind(DoubleArrayAhoCorasickMatchKind::LeftmostLongest)
                    .build(&process_list)
                    .unwrap();
            let matcher_bytes = matcher.serialize();
            let mut matcher_bin = File::create(format!(
                "{out_dir}/delete_daachorse_charwise_u32_matcher.bin"
            ))?;
            matcher_bin.write_all(&matcher_bytes)?;
        }
    }

    Ok(())
}
