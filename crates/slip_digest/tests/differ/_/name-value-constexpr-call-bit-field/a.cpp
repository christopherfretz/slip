// WP-104 (digest §9), consumed: a constexpr function's result as a bit-field width
constexpr unsigned long len(const char *p) { unsigned long n = 0; while (p[n]) ++n; return n; }
constexpr unsigned long f(int) { return len(__func__); }
struct S { int b : f(0); };
