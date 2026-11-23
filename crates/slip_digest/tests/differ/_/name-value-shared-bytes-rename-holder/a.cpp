// WP-115 fix round, F2 (digest §9): f(long) has its own use of "f", but p is f(int)'s "f"; under the rename of f(long) side B's p is h::q()'s "q": the bytes differ, so q(long) differs (writing p as the holder's own name would make it equal)
void take(const char *);
consteval const char *f(int) { return __builtin_FUNCTION(); }
void f(long) { constexpr const char *o = __builtin_FUNCTION(); take(o); constexpr const char *p = f(0); take(p); }
