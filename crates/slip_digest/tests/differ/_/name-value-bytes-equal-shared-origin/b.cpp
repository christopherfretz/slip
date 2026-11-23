// WP-115 fix round, F2 (digest §9): "f" has uses in f(int) and f(long) on side A, in h::f() and f(long) on side B, so its bytes are written on both sides: f(long) passes "f" either way, an equal value under the rename (only g(int), whose body changed, differs)
void take(const char *);
namespace h { consteval const char *f() { return __builtin_FUNCTION(); } }
consteval const char *g(int) { return h::f(); }
void f(long) { (void)__builtin_FUNCTION(); constexpr const char *p = g(0); take(p); }
