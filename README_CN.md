# Matcher

![Rust](https://img.shields.io/badge/rust-%23000000.svg?style=for-the-badge&logo=rust&logoColor=white)![Python](https://img.shields.io/badge/python-3670A0?style=for-the-badge&logo=python&logoColor=ffdd54)![Java](https://img.shields.io/badge/java-%23ED8B00.svg?style=for-the-badge&logo=openjdk&logoColor=white)![C](https://img.shields.io/badge/c-%2300599C.svg?style=for-the-badge&logo=c&logoColor=white)

![PyPI - License](https://img.shields.io/pypi/l/matcher_py)

![Crates.io Version](https://img.shields.io/crates/v/matcher_rs)![GitHub Actions Workflow Status](https://img.shields.io/github/actions/workflow/status/lips7/Matcher/test.yml)![docs.rs](https://img.shields.io/docsrs/matcher_rs)![Crates.io Total Downloads](https://img.shields.io/crates/d/matcher_rs)

![PyPI - Version](https://img.shields.io/pypi/v/matcher_py)![PyPI - Python Version](https://img.shields.io/pypi/pyversions/matcher_py)![PyPI - Downloads](https://img.shields.io/pypi/dm/matcher_py)

一个高性能文本匹配器，旨在解决**逻辑**和**文本变体**的词匹配问题。

它对以下方面非常有帮助：
- **内容过滤**：检测和攻击性或敏感词语。
- **搜索引擎**：通过识别相关关键词来改进搜索结果。
- **文本分析**：从大量文本中提取特定信息。
- **垃圾邮件检测**：识别电子邮件或消息中的垃圾内容。
- ···

## 特性

有关详细的实现，请参见[Design Document](./DESIGN.md)。

- **多种匹配方法**：
	- 简单词匹配
	- 基于正则表达式的匹配
	- 基于相似度的匹配
- **文本转换**：
	- **繁简转换**：将繁体字转换为简体字。例如：`蟲艸` -> `虫草`
	- **删除特定字符**：移除特定字符。例如：`*Fu&*iii&^%%*&kkkk` -> `Fuiiikkkk`
	- **规范化**：将特殊字符规范化为可识别字符。例如：`𝜢𝕰𝕃𝙻𝝧 𝙒ⓞᵣℒ𝒟!` -> `hello world!`
	- **拼音转换**：将汉字转换为拼音以进行模糊匹配。例如：`西安` -> ` xi  an `, 匹配 `洗按` -> ` xi  an `, 但不匹配 `先` -> ` xian `
  - **拼音字符转换**：将汉字转换为拼音。例如：`西安` -> `xian`, 匹配 `洗按` 和 `先` -> `xian`
- **与或非词匹配**：
	- 考虑单词的重复次数。
	- 例如：`hello&world` 匹配 `hello world` 和 `world,hello`
	- 例如：`无&法&无&天` 匹配 `无无法天`（因为 `无` 重复两次），但不匹配 `无法天`
	- 例如：`hello~helloo~hhello` 匹配 `hello` 但不匹配 `helloo` 和 `hhello`
- **可定制的豁免列表**：排除特定单词的匹配。
- **高效处理大型词列表**：针对性能进行了优化。

### Rust 用户

请参阅 [Rust README](./matcher_rs/README.md)。

### Python 用户

请参阅 [Python README](./matcher_py/README.md)。

### C, Java 和其他用户

我们提供动态链接库，请参阅 [C README](./matcher_c/README.md) 和 [Java README](./matcher_java/README.md)。

#### 或从源构建

```shell
git clone https://github.com/Lips7/Matcher.git
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- --default-toolchain nightly -y
cargo build --release
```

在 `target/release` 文件夹底下找到 `libmatcher_c.so`/`libmatcher_c.dylib`/`matcher_c.dll`。

#### 预构建的包

访问 [release page](https://github.com/Lips7/Matcher/releases) 来下载预构建的动态链接库.

## 性能测试

请参阅 [benchmarks](./matcher_rs/README.md#benchmarks) 查看更多细节。