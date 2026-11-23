// WP-104 (digest §9), consumed: a constexpr function's result as a local constant of another function
constexpr unsigned long len(const char *p) { unsigned long n = 0; while (p[n]) ++n; return n; }
constexpr unsigned long start(int) { return len(__func__); }
unsigned long h() { constexpr unsigned long n = start(0); return n; }
