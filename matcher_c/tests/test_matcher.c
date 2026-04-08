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
    char** reduced = reduce_text_process(PROCESS_TYPE_VARIANT_NORM_DELETE_NORMALIZE, input_text);
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

        // Test is_match
        bool is_match = simple_matcher_is_match(matcher, "這是一個測試句子");
        printf("simple_matcher is_match (測試): %s\n", is_match ? "true" : "false");
        if (!is_match) {
            fprintf(stderr, "Error: expected simple_matcher to match '測試'.\n");
            return 1;
        }

        // Test process (struct-based)
        SimpleResultList* results = simple_matcher_process(matcher, "妳好，這是一個測試句子");
        if (results) {
            printf("simple_matcher_process: %zu match(es)\n", results->len);
            for (size_t i = 0; i < results->len; i++) {
                printf("  [%zu] word_id=%u, word=%s\n", i,
                       results->items[i].word_id, results->items[i].word);
            }
            if (results->len == 0) {
                fprintf(stderr, "Error: expected at least one match.\n");
                return 1;
            }
            drop_simple_result_list(results);
        } else {
            fprintf(stderr, "simple_matcher_process returned null.\n");
            return 1;
        }

        // Test process with no matches
        SimpleResultList* no_results = simple_matcher_process(matcher, "nothing here");
        if (no_results) {
            printf("simple_matcher_process (no match): %zu match(es) (correct)\n", no_results->len);
            if (no_results->len != 0) {
                fprintf(stderr, "Error: expected 0 matches.\n");
                return 1;
            }
            drop_simple_result_list(no_results);
        } else {
            fprintf(stderr, "simple_matcher_process returned null unexpectedly.\n");
            return 1;
        }

        // Test find_match (struct-based)
        SimpleResult* found = simple_matcher_find_match(matcher, "這是一個測試句子");
        if (found) {
            printf("simple_matcher_find_match: word_id=%u, word=%s\n",
                   found->word_id, found->word);
            drop_simple_result(found);
        } else {
            fprintf(stderr, "Error: expected find_match to return a result.\n");
            return 1;
        }

        // Test find_match with no match
        SimpleResult* not_found = simple_matcher_find_match(matcher, "nothing here");
        if (not_found != NULL) {
            fprintf(stderr, "Error: expected find_match to return NULL for no match.\n");
            drop_simple_result(not_found);
            return 1;
        }
        printf("simple_matcher_find_match (no match): NULL (correct)\n");

        // Test find_match with empty text
        SimpleResult* empty = simple_matcher_find_match(matcher, "");
        if (empty != NULL) {
            fprintf(stderr, "Error: expected find_match to return NULL for empty text.\n");
            drop_simple_result(empty);
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
