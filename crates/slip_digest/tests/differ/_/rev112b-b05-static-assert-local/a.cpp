// rev-112b probe: a local class member read in a static_assert in f
#include "srcloc.h"
consteval unsigned len(const char *s) { unsigned n = 0; while (s[n]) ++n; return n; }
void take(unsigned);
consteval unsigned plen(const char *p = std::source_location::current().function_name()) { return len(p); }
constexpr unsigned lg(unsigned n = plen()) { return n; }
consteval auto pass(auto fn) { return fn; }
void f() { struct L { static constexpr unsigned m(unsigned n = plen()) { return n; } }; static_assert(L::m() > 0); take(L::m()); }
