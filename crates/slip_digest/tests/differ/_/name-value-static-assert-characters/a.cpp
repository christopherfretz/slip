// WP-104 (digest §9), consumed: a constexpr function over the characters in a static_assert
constexpr unsigned long len(const char *p) { unsigned long n = 0; while (p[n]) ++n; return n; }
namespace legacy {
void f() { static_assert(len(__PRETTY_FUNCTION__) > 3); }
}
