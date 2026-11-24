// rev-112b probe: c08 in the other order of first use: the outer default log() is met before g() alone
#include "srcloc.h"
consteval unsigned len(const char *s) { unsigned n = 0; while (s[n]) ++n; return n; }
void take(unsigned);
constexpr unsigned g(unsigned n = len(__builtin_FUNCTION())) { return n; }
void log(unsigned = g());
void other() { log(); take(g()); }
void f() { log(); take(g()); }
