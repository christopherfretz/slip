// WP-145 item 1 (__builtin_FILE against __builtin_FUNCTION): #line names the file as f's plain name, so one string object is both; a pointer to __builtin_FUNCTION() passed on is written by its bytes and the rename is seen
void take(const char *);
#line 10 "f"
void f() { constexpr const char *p = __builtin_FUNCTION(); take(p); take(__builtin_FILE()); }
