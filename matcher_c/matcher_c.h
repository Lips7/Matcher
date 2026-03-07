void* init_simple_matcher(char* simple_table_bytes);
bool simple_matcher_is_match(void* simple_matcher, char* text);
char* simple_matcher_process_as_string(void* simple_matcher, char* text);
void drop_simple_matcher(void* simple_matcher);

void drop_string(char* ptr);
