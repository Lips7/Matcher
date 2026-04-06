#pragma once

#include <stdint.h>
#include <stdbool.h>

#ifdef __cplusplus
extern "C" {
#endif

// Initialize a SimpleMatcher from JSON bytes. Returns null on error.
// Caller must free with drop_simple_matcher.
void* init_simple_matcher(const char* simple_table_bytes);

// Returns true if text matches any pattern. Returns false on null input or error.
bool simple_matcher_is_match(const void* simple_matcher, const char* text);

// Returns JSON string of match results, or null on error.
// Caller must free the returned string with drop_string.
char* simple_matcher_process_as_string(const void* simple_matcher, const char* text);

// Frees a SimpleMatcher created by init_simple_matcher.
void drop_simple_matcher(void* simple_matcher);

// Frees a string returned by simple_matcher_process_as_string or text_process.
void drop_string(char* ptr);

// ProcessType bit flags
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
