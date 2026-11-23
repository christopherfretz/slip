// WP-115 item 1 (digest §9): the shared __builtin_FUNCTION() string "f" has uses in two entities, so its bytes are written: a cost under the rename
void take(const char *);
void f(int) { constexpr const char *p = __builtin_FUNCTION(); take(p); }
void f(long) { constexpr const char *p = __builtin_FUNCTION(); take(p); }
