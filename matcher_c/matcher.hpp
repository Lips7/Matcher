#pragma once

#include <string>
#include <vector>
#include "matcher_c.h"

namespace matcher {

// Wrapper for text_process
inline std::string text_process_cpp(ProcessType process_type, const std::string& text) {
    char* c_str = text_process(process_type, text.c_str());
    if (c_str) {
        std::string result(c_str);
        drop_string(c_str);
        return result;
    }
    return "";
}

// Wrapper for reduce_text_process to use std::vector<std::string>
inline std::vector<std::string> reduce_text_process_cpp(ProcessType process_type, const std::string& text) {
    CStringArray arr = reduce_text_process(process_type, text.c_str());
    std::vector<std::string> result;
    
    if (arr.strings) {
        result.reserve(arr.len);
        for (size_t i = 0; i < arr.len; i++) {
            result.emplace_back(arr.strings[i]);
        }
    }
    
    // Automatically manage Rust memory using the FFI free function
    drop_string_array(arr);
    
    return result;
}

// RAII Wrapper for SimpleMatcher
class SimpleMatcher {
private:
    void* matcher;

public:
    SimpleMatcher(const std::string& config) {
        matcher = init_simple_matcher(const_cast<char*>(config.c_str()));
    }

    ~SimpleMatcher() {
        if (matcher) {
            drop_simple_matcher(matcher);
        }
    }

    // Disable copy to prevent double-free
    SimpleMatcher(const SimpleMatcher&) = delete;
    SimpleMatcher& operator=(const SimpleMatcher&) = delete;

    // Allow move semantics
    SimpleMatcher(SimpleMatcher&& other) noexcept : matcher(other.matcher) {
        other.matcher = nullptr;
    }
    SimpleMatcher& operator=(SimpleMatcher&& other) noexcept {
        if (this != &other) {
            if (matcher) drop_simple_matcher(matcher);
            matcher = other.matcher;
            other.matcher = nullptr;
        }
        return *this;
    }

    bool is_valid() const {
        return matcher != nullptr;
    }

    bool is_match(const std::string& text) {
        if (!matcher) return false;
        return simple_matcher_is_match(matcher, const_cast<char*>(text.c_str()));
    }

    std::string process_as_string(const std::string& text) {
        if (!matcher) return "";
        char* res = simple_matcher_process_as_string(matcher, const_cast<char*>(text.c_str()));
        if (res) {
            std::string result(res);
            drop_string(res);
            return result;
        }
        return "";
    }
};

} // namespace matcher
