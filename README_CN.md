# Matcher

![Rust](https://img.shields.io/badge/rust-%23000000.svg?style=for-the-badge&logo=rust&logoColor=white)![Python](https://img.shields.io/badge/python-3670A0?style=for-the-badge&logo=python&logoColor=ffdd54)![Java](https://img.shields.io/badge/java-%23ED8B00.svg?style=for-the-badge&logo=openjdk&logoColor=white)![C](https://img.shields.io/badge/c-%2300599C.svg?style=for-the-badge&logo=c&logoColor=white)

![PyPI - License](https://img.shields.io/pypi/l/matcher_py)

![Crates.io Version](https://img.shields.io/crates/v/matcher_rs)![GitHub Actions Workflow Status](https://img.shields.io/github/actions/workflow/status/lips7/Matcher/test.yml)![docs.rs](https://img.shields.io/docsrs/matcher_rs)![Crates.io Total Downloads](https://img.shields.io/crates/d/matcher_rs)

![PyPI - Version](https://img.shields.io/pypi/v/matcher_py)![PyPI - Python Version](https://img.shields.io/pypi/pyversions/matcher_py)![PyPI - Downloads](https://img.shields.io/pypi/dm/matcher_py)

高性能文本匹配器，旨在解决词匹配中的**逻辑**和**文本变体**问题，采用 Rust 实现。

它在以下场景非常有帮助：
- **精确率和召回率**：词匹配是一个检索过程。逻辑匹配提高精确率，文本变体匹配提高召回率。
- **内容过滤**：检测并过滤冒犯性或敏感词汇。
- **搜索引擎**：通过识别相关关键词来改进搜索结果。
- **文本分析**：从大容量文本中提取特定信息。
- **垃圾邮件检测**：识别电子邮件或消息中的垃圾内容。
- ···

## 特性

详细实现请参考 [设计文档](./DESIGN.md)。

- **文本转换 (Text Transformation)**：
  - **繁简转换 (Fanjian)**：将繁体中文转换为简体。
    示例：`蟲艸` -> `虫草`
  - **删除 (Delete)**：移除特定字符（如标点、特殊符号）。
    示例：`*Fu&*iii&^%%*&kkkk` -> `Fuiiikkkk`
  - **规范化 (Normalize)**：将特殊字符规范化为标准字符。
    示例：`𝜢𝕰𝕃𝙻𝝧 𝙒ⓞᵣℒ𝒟!` -> `hello world!`
  - **拼音 (PinYin)**：将汉字转换为带空格的拼音，用于模糊匹配。
    示例：`西安` -> ` xi  an `，匹配 `洗按` -> ` xi  an `，但不匹配 `先` -> ` xian `
  - **拼音简写 (PinYinChar)**：将汉字转换为紧凑拼音。
    示例：`西安` -> `xian`，匹配 `洗按` 和 `先` -> `xian`
- **与 (AND) 或 (OR) 非 (NOT) 逻辑匹配**：
  - 支持考虑单词重复次数。
  - 示例：`hello&world` 匹配 `hello world` 和 `world,hello`
  - 示例：`无&法&无&天` 匹配 `无无法天`（因为 `无` 重复了两次），但不匹配 `无法天`
  - 示例：`hello~helloo~hhello` 匹配 `hello`，但不匹配 `helloo` 和 `hhello`
- **高效处理大规模词表**：针对高性能运行进行了端到端优化。

### Rust 用户

请参阅 [Rust README](./matcher_rs/README.md)。

### Python 用户

请参阅 [Python README](./matcher_py/README.md)。

### C, Java 以及其他语言用户

我们提供了动态链接库用于链接集成。请参阅 [C README](./matcher_c/README.md) 和 [Java README](./matcher_java/README.md)。

#### 从源码编译

```shell
git clone https://github.com/Lips7/Matcher.git
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- --default-toolchain nightly -y
cargo build --release
```

编译完成后，你可以在 `target/release` 目录下找到 `libmatcher_c.so` / `libmatcher_c.dylib` / `matcher_c.dll`。

#### 预编译二进制文件

访问 [Release 页面](https://github.com/Lips7/Matcher/releases) 下载预编译好的二进制文件。

## 基准测试 (Benchmarks)

详情请参考 [基准测试](./matcher_rs/README.md#benchmarks)。