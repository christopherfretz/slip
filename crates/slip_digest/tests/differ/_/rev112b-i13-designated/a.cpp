// rev-112b probe: a designated initializer in a default argument leaves a consuming default member initializer
#include "srcloc.h"
consteval unsigned len(const char *s) { unsigned n = 0; while (s[n]) ++n; return n; }
void take(unsigned);
struct S { int x; unsigned n = len(__builtin_FUNCTION()); };
void log(S = {.x = 1});
void f() { log(); }
