#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "../matcher_c.h"

int main() {
    printf("Running matcher_c tests...\n");

    // 1. Test text_process
    const char* input_text = "This is a Test string with   Some extra   spaces! 測試";

    // Example: PROCESS_TYPE_NORMALIZE (8)
    // text_process only accepts a single bit.
    char* processed = text_process(PROCESS_TYPE_NORMALIZE, input_text);
    if (processed) {
        printf("text_process result: %s\n", processed);
        // We do a basic check. The actual behavior depends on Rust's `text_process_rs`.
        // NORMALIZE usually lowercases and does NFKC. DELETE usually removes symbols/spaces.
        if (strlen(processed) == 0) {
            fprintf(stderr, "Error: processed string is empty, expected content.\n");
            return 1;
        }
        drop_string(processed);
    } else {
        fprintf(stderr, "text_process failed or returned null.\n");
        return 1;
    }

    // 2. Test reduce_text_process
    // We expect an array of strings representing variants
    char** reduced = reduce_text_process(PROCESS_TYPE_FANJIAN_DELETE_NORMALIZE, input_text);
    if (reduced) {
        printf("reduce_text_process returned variants:\n");
        size_t i = 0;
        while (reduced[i] != NULL) {
            printf("  Variant %zu: %s\n", i, reduced[i]);
            i++;
        }
        drop_string_array(reduced);
    } else {
        fprintf(stderr, "reduce_text_process failed or returned no variants.\n");
        return 1;
    }

    // 3. Test simple_matcher
    const char* simple_table_json = "{\"1\": {\"1\": \"測試\"}, \"2\": {\"2\": \"你好\"}}";
    void* matcher = init_simple_matcher(simple_table_json);
    if (matcher) {
        printf("simple_matcher initialized successfully.\n");
        // Test match
        bool is_match = simple_matcher_is_match(matcher, "這是一個測試句子");
        printf("simple_matcher is_match (測試): %s\n", is_match ? "true" : "false");
        if (!is_match) {
            fprintf(stderr, "Error: expected simple_matcher to match '測試'.\n");
            return 1;
        }

        // Test process as string
        char* process_result = simple_matcher_process_as_string(matcher, "妳好，這是一個測試句子");
        if (process_result) {
            printf("simple_matcher_process_as_string result: %s\n", process_result);
            drop_string(process_result);
        } else {
            fprintf(stderr, "simple_matcher_process_as_string returned null.\n");
            return 1;
        }

        drop_simple_matcher(matcher);
    } else {
        fprintf(stderr, "init_simple_matcher failed.\n");
        return 1;
    }

    // If we reach here, no crashes occurred
    printf("Tests passed successfully.\n");
    return 0;
}
