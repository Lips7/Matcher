#pragma once

#include <stdint.h>
#include <stdbool.h>
#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

void* init_simple_matcher(const char* simple_table_bytes);
bool simple_matcher_is_match(void* simple_matcher, const char* text);
char* simple_matcher_process_as_string(void* simple_matcher, const char* text);
void drop_simple_matcher(void* simple_matcher);

void drop_string(char* ptr);

// ProcessType enum definitions
typedef uint8_t ProcessType;

#define PROCESS_TYPE_NONE 1
#define PROCESS_TYPE_FANJIAN 2
#define PROCESS_TYPE_DELETE 4
#define PROCESS_TYPE_NORMALIZE 8
#define PROCESS_TYPE_DELETE_NORMALIZE 12
#define PROCESS_TYPE_FANJIAN_DELETE_NORMALIZE 14
#define PROCESS_TYPE_PINYIN 16
#define PROCESS_TYPE_PINYIN_CHAR 32

char* text_process(ProcessType process_type, const char* text);
char** reduce_text_process(ProcessType process_type, const char* text);
void drop_string_array(char** array);

#ifdef __cplusplus
}
#endif
