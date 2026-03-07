#include <iostream>
#include <string>
#include <vector>
#include "../matcher.hpp"

int main() {
    std::cout << "Testing matcher_c with C++ Wrapper...\n";
    
    // Test 1: text_process
    std::cout << "\n--- Testing text_process ---\n";
    std::string text1 = "測試文本"; // Traditional Chinese
    std::cout << "Input: '" << text1 << "'\n";
    
    std::string result1 = matcher::text_process_cpp(PROCESS_TYPE_FANJIAN, text1);
    if (!result1.empty()) {
        std::cout << "Fanjian Output: '" << result1 << "'\n";
    } else {
        std::cout << "Failed to process text.\n";
    }

    // Test 2: reduce_text_process
    std::cout << "\n--- Testing reduce_text_process ---\n";
    // Combine FanJian (2), Delete (4), Normalize (8) = 14
    std::string text2 = "A B 測試 Ａ １";
    std::cout << "Input: '" << text2 << "'\n";
    
    // using std::vector<std::string> directly instead of CStringArray!
    std::vector<std::string> variants = matcher::reduce_text_process_cpp(PROCESS_TYPE_FANJIAN_DELETE_NORMALIZE, text2);
    
    if (!variants.empty()) {
        std::cout << "Output has " << variants.size() << " variants:\n";
        for (size_t i = 0; i < variants.size(); i++) {
            std::cout << "  Variant " << i << ": '" << variants[i] << "'\n";
        }
    } else {
        std::cout << "Failed to reduce process text.\n";
    }

    // Test 3: simple matcher
    std::cout << "\n--- Testing simple_matcher ---\n";
    std::string config = R"({"1":{"1":"hello&world", "2": "test"}})";
    std::cout << "Config: " << config << "\n";
    
    matcher::SimpleMatcher simple_matcher(config);
    if (simple_matcher.is_valid()) {
        std::string test_text = "hello test world";
        std::cout << "Input: '" << test_text << "'\n";
        
        bool is_match = simple_matcher.is_match(test_text);
        std::cout << "Is match: " << (is_match ? "true" : "false") << "\n";
        
        std::string result = simple_matcher.process_as_string(test_text);
        if (!result.empty()) {
            std::cout << "Result: " << result << "\n";
        } else {
            std::cout << "Failed to get match result.\n";
        }
    } else {
        std::cout << "Failed to initialize simple_matcher.\n";
    }

    std::cout << "\nAll tests finished.\n";
    return 0;
}
