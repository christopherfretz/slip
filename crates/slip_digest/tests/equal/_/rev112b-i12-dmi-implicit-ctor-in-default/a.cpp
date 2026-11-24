// rev-112b probe: a default argument S() value-initialises an aggregate with a consuming default member initializer
#include "srcloc.h"
consteval unsigned len(const char *s) { unsigned n = 0; while (s[n]) ++n; return n; }
void take(unsigned);
struct S { unsigned n = len(__builtin_FUNCTION()); };
void log(S = S());
void f() { log(); }
