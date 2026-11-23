// WP-104 (digest §9), consumed: a constexpr function's result as a case label
constexpr unsigned long len(const char *p) { unsigned long n = 0; while (p[n]) ++n; return n; }
constexpr unsigned long f(int) { return len(__func__); }
int h(int x) { switch (x) { case f(0): return 1; default: return 2; } }
