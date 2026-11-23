// WP-104 (digest §9), F3: block-scope constexpr pointer bound to __PRETTY_FUNCTION__
void take(const char *);
namespace legacy { void f() { constexpr const char *p = __PRETTY_FUNCTION__; take(p); } }
