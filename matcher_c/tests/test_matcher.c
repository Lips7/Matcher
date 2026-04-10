#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "../matcher_c.h"

int main() {
    printf("Running matcher_c tests...\n");

    // 1. Test text_process
    const char* input_text = "This is a Test string with   Some extra   spaces! 測試";

    char* processed = text_process(PROCESS_TYPE_NORMALIZE, input_text);
    if (processed) {
        printf("text_process result: %s\n", processed);
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

    // 3. Test simple_matcher (JSON constructor)
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

        // Test process
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

        // Test find_match
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

        // Test heap_bytes
        size_t heap = simple_matcher_heap_bytes(matcher);
        printf("simple_matcher_heap_bytes: %zu\n", heap);
        if (heap == 0) {
            fprintf(stderr, "Error: expected heap_bytes > 0.\n");
            return 1;
        }

        drop_simple_matcher(matcher);
    } else {
        fprintf(stderr, "init_simple_matcher failed.\n");
        return 1;
    }

    // 4. Test builder
    printf("\n--- Builder tests ---\n");

    void* builder = init_simple_matcher_builder();
    if (!builder) {
        fprintf(stderr, "Error: init_simple_matcher_builder returned NULL.\n");
        return 1;
    }

    bool ok = simple_matcher_builder_add_word(builder, PROCESS_TYPE_NONE, 1, "測試");
    if (!ok) {
        fprintf(stderr, "Error: add_word failed.\n");
        return 1;
    }
    ok = simple_matcher_builder_add_word(builder, PROCESS_TYPE_DELETE, 2, "你好");
    if (!ok) {
        fprintf(stderr, "Error: add_word failed.\n");
        return 1;
    }

    void* built_matcher = simple_matcher_builder_build(builder);
    // builder is consumed — do NOT use or free it after this
    if (!built_matcher) {
        fprintf(stderr, "Error: builder_build returned NULL.\n");
        return 1;
    }

    // Verify builder-constructed matcher works like JSON-constructed one
    if (!simple_matcher_is_match(built_matcher, "這是一個測試句子")) {
        fprintf(stderr, "Error: builder matcher should match '測試'.\n");
        return 1;
    }
    if (!simple_matcher_is_match(built_matcher, "你！好")) {
        fprintf(stderr, "Error: builder matcher should match '你好' via DELETE.\n");
        return 1;
    }
    if (simple_matcher_is_match(built_matcher, "nothing")) {
        fprintf(stderr, "Error: builder matcher should not match 'nothing'.\n");
        return 1;
    }
    printf("Builder matcher: is_match tests passed.\n");

    // heap_bytes on builder-constructed matcher
    size_t builder_heap = simple_matcher_heap_bytes(built_matcher);
    printf("Builder matcher heap_bytes: %zu\n", builder_heap);
    if (builder_heap == 0) {
        fprintf(stderr, "Error: expected heap_bytes > 0.\n");
        return 1;
    }

    drop_simple_matcher(built_matcher);

    // 5. Test builder cleanup without build
    void* abandoned = init_simple_matcher_builder();
    simple_matcher_builder_add_word(abandoned, PROCESS_TYPE_NONE, 1, "test");
    drop_simple_matcher_builder(abandoned);
    printf("Builder drop (without build): no crash.\n");

    // 6. Edge cases
    if (simple_matcher_builder_add_word(NULL, PROCESS_TYPE_NONE, 1, "test")) {
        fprintf(stderr, "Error: add_word with NULL builder should return false.\n");
        return 1;
    }
    if (simple_matcher_builder_build(NULL) != NULL) {
        fprintf(stderr, "Error: build(NULL) should return NULL.\n");
        return 1;
    }
    if (simple_matcher_heap_bytes(NULL) != 0) {
        fprintf(stderr, "Error: heap_bytes(NULL) should return 0.\n");
        return 1;
    }
    printf("Edge cases: passed.\n");

    printf("\nAll tests passed successfully.\n");
    return 0;
}
