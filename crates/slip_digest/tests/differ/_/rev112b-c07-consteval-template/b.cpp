// rev-112b probe: a consteval function template's call in a default argument
#include "srcloc.h"
consteval unsigned len(const char *s) { unsigned n = 0; while (s[n]) ++n; return n; }
void take(unsigned);
template<class T> consteval unsigned tlen(T s) { return len(s); }
void log(unsigned = tlen(__builtin_FUNCTION()));
void start() { log(); }
