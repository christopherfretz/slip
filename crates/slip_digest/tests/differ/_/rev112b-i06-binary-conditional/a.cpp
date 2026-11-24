// rev-112b probe: GNU ?: in a default argument: the consumed operand is the common expression
#include "srcloc.h"
consteval unsigned len(const char *s) { unsigned n = 0; while (s[n]) ++n; return n; }
void take(unsigned);
void log(unsigned = len(__builtin_FUNCTION()) ?: 7u);
void f() { log(); }
