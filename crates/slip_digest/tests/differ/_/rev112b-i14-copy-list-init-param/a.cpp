// rev-112b probe: a braced default argument {} for an aggregate parameter
#include "srcloc.h"
consteval unsigned len(const char *s) { unsigned n = 0; while (s[n]) ++n; return n; }
void take(unsigned);
struct S { unsigned n = len(__builtin_FUNCTION()); };
void log(S = {});
void f() { log(); }
