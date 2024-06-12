// Initializes and returns a matcher object from the provided match table map bytes.
void* init_matcher(char* match_table_map_bytes);

// Checks if the text matches using the given matcher object.
// Returns true if there is a match, otherwise false.
bool matcher_is_match(void* matcher, char* text);

// Returns the matched word in the text using the given matcher object,
// or NULL if there is no match.
char* matcher_word_match(void* matcher, char* text);

// Releases the resources held by the matcher object.
void drop_matcher(void* matcher);

// Initializes and returns a simple matcher object from the provided simple wordlist dictionary bytes.
void* init_simple_matcher(char* simple_wordlist_dict_bytes);

// Checks if the text matches using the given simple matcher object.
// Returns true if there is a match, otherwise false.
bool simple_matcher_is_match(void* simple_matcher, char* text);

// Processes the text using the given simple matcher object and returns a processed result,
// or NULL if there is no processing result.
char* simple_matcher_process(void* simple_matcher, char* text);

// Releases the resources held by the simple matcher object.
void drop_simple_matcher(void* simple_matcher);

// Releases the memory allocated for the string.
void drop_string(char* ptr);