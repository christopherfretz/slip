// WP-104 (digest §9), consumed: a constexpr function over the characters as a constant
constexpr unsigned long len(const char *p) { unsigned long n = 0; while (p[n]) ++n; return n; }
template <unsigned long N> struct B {};
namespace acme {
void f() { B<len(__PRETTY_FUNCTION__)> b; (void)b; }
}
