import os

import ormsgpack

from cffi import FFI

absolute_path = os.path.dirname(__file__)

ffi = FFI()
ffi.cdef(
    open(os.path.join(absolute_path, "./matcher_c.h"), "r", encoding="utf-8").read()
)

lib = ffi.dlopen(os.path.join(absolute_path, "./matcher_c.so"))


if __name__ == "__main__":
    simple_matcher = lib.init_simple_matcher(
        ormsgpack.packb({15: [{"word_id": 1, "word": "你好"}]})
    )

    res = lib.simple_matcher_process(simple_matcher, "你好".encode("utf-8"))
    print(ffi.string(res).decode("utf-8"))
    lib.drop_string(res)

    lib.drop_simple_matcher(simple_matcher)
