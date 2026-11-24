// rev-112b probe: a local class in a lambda in an array bound: the consumed length is the bound
#include "srcloc.h"
consteval unsigned len(const char *s) { unsigned n = 0; while (s[n]) ++n; return n; }
void take(unsigned);
consteval unsigned plen(const char *p = std::source_location::current().function_name()) { return len(p); }
constexpr unsigned lg(unsigned n = plen()) { return n; }
consteval auto pass(auto fn) { return fn; }
void f() { int a[([] { struct L { static constexpr unsigned m() { return lg(); } }; return L::m(); }())]; take(sizeof a); }
