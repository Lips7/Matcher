void* init_matcher(char* match_table_map_bytes);
bool matcher_is_match(void* matcher, char* text);
char* matcher_process_as_string(void* matcher, char* text);
char* matcher_word_match_as_string(void* matcher, char* text);
void drop_matcher(void* matcher);

void* init_simple_matcher(char* simple_table_bytes);
bool simple_matcher_is_match(void* simple_matcher, char* text);
char* simple_matcher_process_as_string(void* simple_matcher, char* text);
void drop_simple_matcher(void* simple_matcher);

void drop_string(char* ptr);