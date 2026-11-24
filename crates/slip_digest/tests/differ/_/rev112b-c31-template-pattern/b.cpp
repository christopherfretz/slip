// rev-112b probe: a function template whose body's immediate invocation reads its name through a default; instantiated once
#include "srcloc.h"
consteval unsigned len(const char *s) { unsigned n = 0; while (s[n]) ++n; return n; }
void take(unsigned);
consteval unsigned lenf(const char *s = __builtin_FUNCTION()) { return len(s); }
template<class T> void start(T) { take(lenf()); }
void user() { start(1); }
