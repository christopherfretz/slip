// WP-115 named hole (digest §9): f(long) holds its own use of the shared __builtin_FUNCTION() string "f", but p's value comes from f(int); under a rename of f(int) side B's p is another function's "f", and f(long) is written alike
void take(const char *);
namespace h { consteval const char *f() { return __builtin_FUNCTION(); } }
consteval const char *g(int) { return h::f(); }
void f(long) { (void)__builtin_FUNCTION(); constexpr const char *p = g(0); take(p); }
