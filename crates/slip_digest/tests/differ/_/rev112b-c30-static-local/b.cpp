// rev-112b probe: a constant-initialised static local whose immediate invocation's default consumes f's name
#include "srcloc.h"
consteval unsigned len(const char *s) { unsigned n = 0; while (s[n]) ++n; return n; }
void take(unsigned);
consteval unsigned lenf(const char *s = __builtin_FUNCTION()) { return len(s); }
void start() { static unsigned n = lenf(); take(n); }
