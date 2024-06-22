# Matcher Rust Implement C FFI bindings

## Overview

Matcher is a high-performance matching library implemented in Rust, providing C FFI bindings for seamless integration with other programming languages. This library is designed for various matching tasks, including simple and complex match types with normalization and deletion capabilities.

## Installation

### Build from source

```shell
git clone https://github.com/Lips7/Matcher.git
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- --default-toolchain nightly -y
cargo build --release
```

Then you should find the `libmatcher_c.so`/`libmatcher_c.dylib`/`matcher_c.dll` in the `target/release` directory.

### Install pre-built binary

Visit the [release page](https://github.com/Lips7/Matcher/releases) to download the pre-built binary.

## Python usage example

```Python
import msgspec

from cffi import FFI

from extension_types import MatchTableType, SimpleMatchType, MatchTable

## define ffi
ffi = FFI()
ffi.cdef(open("./matcher_c.h", "r", encoding="utf-8").read())
lib = ffi.dlopen("./matcher_c.so")

# init matcher
matcher = lib.init_matcher(
    msgspec.msgpack.encode({
        1: [
            MatchTable(
                table_id=1,
                match_table_type=MatchTableType.Simple(simple_match_type=SimpleMatchType.MatchNone),
                word_list=["hello,world", "hello", "world"],
                exemption_simple_match_type=SimpleMatchType.MatchNone,
                exemption_word_list=[],
            )
        ]
    })
)

# check is match
lib.matcher_is_match(matcher, "hello".encode("utf-8")) # True

# match word, output json string
res = lib.matcher_word_match(matcher, "hello,world".encode("utf-8")) # {1:[{"table_id":1,"word":"hello"},{"table_id":1,"word":"hello,world"},{"table_id":1,"word":"world"}]"}
print(ffi.string(res).decode("utf-8")) #
lib.drop_string(res)

# drop matcher
lib.drop_matcher(matcher)

# init simple matcher
simple_matcher = lib.init_simple_matcher(
    msgspec.msgpack.encode(({
        SimpleMatchType.MatchFanjianDeleteNormalize | SimpleMatchType.MatchPinYinChar: {
            1: "妳好,世界",
            2: "hello"
        }
    }))
)

# check is match
lib.simple_matcher_is_match(simple_matcher, "你好世界".encode("utf-8")) # True

# match word, output json string
res = lib.simple_matcher_process(simple_matcher, "nihaoshijie!hello!world!".encode("utf-8")) # [{"word_id":1,"word":"妳好,世界"},{"word_id":2,"word":"hello"}]
print(ffi.string(res).decode("utf-8"))
lib.drop_string(res)

# drop simple matcher
lib.drop_simple_matcher(simple_matcher)
```

## Important Notes

1. The [extension_types.py](./extension_types.py) is not required, you can use the dynamic library directly.
2. Always call `drop_matcher`, `drop_simple_matcher`, and `drop_string` after initializing and processing to avoid memory leaks.