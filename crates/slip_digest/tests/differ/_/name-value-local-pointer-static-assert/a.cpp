// WP-104 (digest §9), F3: a block-scope constexpr pointer to __PRETTY_FUNCTION__ read by a static_assert: the condition reads the variable's value by value
void take(const char *);
namespace legacy { void f() { constexpr const char *p = __PRETTY_FUNCTION__; static_assert(p[0] == 'v'); take(p); } }
