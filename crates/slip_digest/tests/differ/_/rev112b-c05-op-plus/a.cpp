// rev-112b probe: consteval overloaded operator in a default argument
#include "srcloc.h"
consteval unsigned len(const char *s) { unsigned n = 0; while (s[n]) ++n; return n; }
void take(unsigned);
struct Z {};
consteval unsigned operator+(Z, const char *s) { return len(s); }
void log(unsigned = Z{} + __builtin_FUNCTION());
void f() { log(); }
