// rev-112b probe: a use after a lambda in the same constant: still under it
#include "srcloc.h"
consteval unsigned len(const char *s) { unsigned n = 0; while (s[n]) ++n; return n; }
void take(unsigned);
consteval unsigned plen(const char *p = std::source_location::current().function_name()) { return len(p); }
constexpr unsigned lg(unsigned n = plen()) { return n; }
consteval auto pass(auto fn) { return fn; }
consteval unsigned two(auto fn, unsigned n) { return n; }
void start() { take(two([] { take(lg()); }, plen())); }
