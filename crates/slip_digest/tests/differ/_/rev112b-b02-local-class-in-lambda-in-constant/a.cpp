// rev-112b probe: a lambda in a local class member in a constexpr lambda, in an immediate invocation
#include "srcloc.h"
consteval unsigned len(const char *s) { unsigned n = 0; while (s[n]) ++n; return n; }
void take(unsigned);
consteval unsigned plen(const char *p = std::source_location::current().function_name()) { return len(p); }
constexpr unsigned lg(unsigned n = plen()) { return n; }
consteval auto pass(auto fn) { return fn; }
void f() { auto l = pass([] { struct L { static void m() { auto k = [] { take(lg()); }; k(); } }; L::m(); }); l(); }
