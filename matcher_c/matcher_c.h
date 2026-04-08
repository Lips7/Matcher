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

// --- SimpleMatcher lifecycle ---

// Initialize a SimpleMatcher from JSON bytes. Returns null on error.
// Caller must free with drop_simple_matcher.
void* init_simple_matcher(const char* simple_table_bytes);

// Frees a SimpleMatcher created by init_simple_matcher.
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

// --- Result deallocation ---

// Frees a single SimpleResult returned by simple_matcher_find_match.
void drop_simple_result(SimpleResult* result);

// Frees a SimpleResultList returned by simple_matcher_process.
void drop_simple_result_list(SimpleResultList* list);

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
