// WP-104 (digest §9), consumed: a constexpr function's result in alignas
constexpr unsigned long len(const char *p) { unsigned long n = 0; while (p[n]) ++n; return n; }
constexpr unsigned long f(int) { return len(__func__); }
struct alignas(f(0) > 3 ? 8 : 4) S {};
