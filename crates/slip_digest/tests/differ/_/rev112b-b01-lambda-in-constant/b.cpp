// rev-112b probe: a lambda body in an immediate invocation uses a default that consumes the pretty name (which holds f)
#include "srcloc.h"
consteval unsigned len(const char *s) { unsigned n = 0; while (s[n]) ++n; return n; }
void take(unsigned);
consteval unsigned plen(const char *p = std::source_location::current().function_name()) { return len(p); }
constexpr unsigned lg(unsigned n = plen()) { return n; }
consteval auto pass(auto fn) { return fn; }
void start() { auto l = pass([] { take(lg()); }); l(); }
