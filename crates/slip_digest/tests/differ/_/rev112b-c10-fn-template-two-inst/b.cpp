// rev-112b probe: a function template's default used by two specialisations
#include "srcloc.h"
consteval unsigned len(const char *s) { unsigned n = 0; while (s[n]) ++n; return n; }
void take(unsigned);
template<class T> void log(T, unsigned = len(__builtin_FUNCTION()));
void start() { log(1); log(1.0); }
