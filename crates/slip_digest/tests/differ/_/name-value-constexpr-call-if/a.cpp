// WP-104 (digest §9), consumed: a constexpr function's result in if constexpr
constexpr unsigned long len(const char *p) { unsigned long n = 0; while (p[n]) ++n; return n; }
constexpr unsigned long f(int) { return len(__func__); }
int h() { if constexpr (f(0) > 3) return 1; else return 2; }
