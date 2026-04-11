#pragma once

#include <stdint.h>
#include <stdbool.h>
#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

// Returns the library version as a static string (do NOT free).
const char* matcher_version(void);

// --- Result types ---

// A single match result. word is an owned null-terminated UTF-8 string.
typedef struct {
    uint32_t word_id;
    char* word;
} SimpleResult;

// A list of match results.
typedef struct {
    size_t len;
    SimpleResult* items;
} SimpleResultList;

// --- SimpleMatcherBuilder ---

// Create a new builder. Caller must either:
//   (a) call simple_matcher_builder_build() which consumes the builder, OR
//   (b) call drop_simple_matcher_builder() to free it without building.
void* init_simple_matcher_builder(void);

// Add a word pattern. The word string is copied; caller retains ownership.
// Returns true on success, false on null builder/word or internal error.
bool simple_matcher_builder_add_word(void* builder, uint8_t process_type,
                                     uint32_t word_id, const char* word);

// Consume the builder and produce a SimpleMatcher. Returns NULL on error.
// The builder is ALWAYS freed by this call (even on error). Do NOT call
// drop_simple_matcher_builder or use the builder pointer after this.
void* simple_matcher_builder_build(void* builder);

// Free a builder NOT consumed by build(). No-op on NULL.
void drop_simple_matcher_builder(void* builder);

// --- SimpleMatcher lifecycle ---

// Initialize a SimpleMatcher from JSON bytes. Returns null on error.
// Caller must free with drop_simple_matcher.
void* init_simple_matcher(const char* simple_table_bytes);

// Frees a SimpleMatcher created by init_simple_matcher or builder_build.
void drop_simple_matcher(void* simple_matcher);

// --- Matching ---

// Returns true if text matches any pattern. Returns false on null input or error.
bool simple_matcher_is_match(const void* simple_matcher, const char* text);

// Returns all matches for the input text, or NULL on error.
// Caller must free with drop_simple_result_list.
SimpleResultList* simple_matcher_process(const void* simple_matcher, const char* text);

// Returns the first match, or NULL if no match.
// Caller must free a non-NULL return with drop_simple_result.
SimpleResult* simple_matcher_find_match(const void* simple_matcher, const char* text);

// Approximate heap memory in bytes used by the matcher. Returns 0 on NULL.
size_t simple_matcher_heap_bytes(const void* simple_matcher);

// --- Batch matching (parallel via rayon) ---

// Batch is_match: tests each text in parallel, returns bool array of `count`.
// Caller must free with drop_bool_array(ptr, count).
bool* simple_matcher_batch_is_match(const void* simple_matcher,
                                     const char** texts, size_t count);

// Batch process: matches each text in parallel, returns SimpleResultList array of `count`.
// Caller must free with drop_simple_result_list_array(ptr, count).
SimpleResultList* simple_matcher_batch_process(const void* simple_matcher,
                                                const char** texts, size_t count);

// Batch find_match: finds first match per text in parallel, returns SimpleResult* array.
// Each element is either a valid pointer (match found) or NULL (no match).
// Caller must free with drop_simple_result_ptr_array(ptr, count).
SimpleResult** simple_matcher_batch_find_match(const void* simple_matcher,
                                                const char** texts, size_t count);

// --- Result deallocation ---

// Frees a single SimpleResult returned by simple_matcher_find_match.
void drop_simple_result(SimpleResult* result);

// Frees a SimpleResultList returned by simple_matcher_process.
void drop_simple_result_list(SimpleResultList* list);

// Frees a bool array returned by simple_matcher_batch_is_match.
void drop_bool_array(bool* ptr, size_t count);

// Frees a SimpleResultList array returned by simple_matcher_batch_process.
void drop_simple_result_list_array(SimpleResultList* ptr, size_t count);

// Frees a SimpleResult* array returned by simple_matcher_batch_find_match.
void drop_simple_result_ptr_array(SimpleResult** ptr, size_t count);

// Frees a string returned by text_process.
void drop_string(char* ptr);

// --- ProcessType ---

typedef uint8_t ProcessType;

#define PROCESS_TYPE_NONE 1
#define PROCESS_TYPE_VARIANT_NORM 2
#define PROCESS_TYPE_DELETE 4
#define PROCESS_TYPE_NORMALIZE 8
#define PROCESS_TYPE_DELETE_NORMALIZE 12
#define PROCESS_TYPE_VARIANT_NORM_DELETE_NORMALIZE 14
#define PROCESS_TYPE_ROMANIZE 16
#define PROCESS_TYPE_ROMANIZE_CHAR 32
#define PROCESS_TYPE_EMOJI_NORM 64

// --- Text processing ---

// Apply a single ProcessType transform to text. Caller must free result with drop_string.
char* text_process(ProcessType process_type, const char* text);

// Apply all ProcessType transforms up to process_type and return null-terminated array of variants.
// Caller must free the result with drop_string_array.
char** reduce_text_process(ProcessType process_type, const char* text);

// Frees a null-terminated char** array returned by reduce_text_process.
void drop_string_array(char** array);

#ifdef __cplusplus
}
#endif
