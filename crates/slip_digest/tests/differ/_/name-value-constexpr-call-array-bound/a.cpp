// WP-104 (digest §9), consumed: a constexpr function's result as an array bound
constexpr unsigned long len(const char *p) { unsigned long n = 0; while (p[n]) ++n; return n; }
constexpr unsigned long f(int) { return len(__func__); }
unsigned long h() { char a[f(0)]; return sizeof a; }
