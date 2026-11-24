// rev-112b probe: defaults added across redeclarations
#include "srcloc.h"
consteval unsigned len(const char *s) { unsigned n = 0; while (s[n]) ++n; return n; }
void take(unsigned);
void log(unsigned, unsigned = 0);
void log(unsigned = len(__builtin_FUNCTION()), unsigned);
void f() { log(); }
