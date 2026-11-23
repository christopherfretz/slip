// WP-115 item 1 (digest §9), a cost: "f" has uses in f(int) and f(long) on side A, so its bytes are written; under the rename of f(int) side B's "q" is q(int)'s alone: f(int) and f(long) differ
void take(const char *);
consteval const char *f(int) { return __builtin_FUNCTION(); }
void f(long) { constexpr const char *o = __builtin_FUNCTION(); take(o); constexpr const char *p = f(0); take(p); }
