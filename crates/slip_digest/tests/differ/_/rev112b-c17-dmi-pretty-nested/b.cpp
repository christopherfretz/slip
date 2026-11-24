// rev-112b probe: a nested default through a default member initializer, consumed
#include "srcloc.h"
consteval unsigned len(const char *s) { unsigned n = 0; while (s[n]) ++n; return n; }
void take(unsigned);
consteval unsigned plen(const char *p) { return len(p); }
constexpr unsigned g(unsigned n = plen(std::source_location::current().function_name())) { return n; }
struct S { unsigned n = g(); };
void start() { S s{}; take(s.n); }
