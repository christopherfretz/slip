// WP-104 (digest §9), consumed: a constexpr function over the characters as a local constant
constexpr unsigned long len(const char *p) { unsigned long n = 0; while (p[n]) ++n; return n; }
namespace acme {
unsigned long f() { constexpr unsigned long n = len(__PRETTY_FUNCTION__); return n; }
}
