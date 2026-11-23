// WP-104 (digest §9), consumed: a constexpr function's result in noexcept
constexpr unsigned long len(const char *p) { unsigned long n = 0; while (p[n]) ++n; return n; }
constexpr unsigned long start(int) { return len(__func__); }
void h() noexcept(start(0) > 3);
