// rev-112b probe: consteval converting constructor reached by an implicit conversion in a default argument consumes the caller's name
#include "srcloc.h"
consteval unsigned len(const char *s) { unsigned n = 0; while (s[n]) ++n; return n; }
void take(unsigned);
struct L { unsigned n; consteval L(const char *s) : n(len(s)) {} };
void log(L = __builtin_FUNCTION());
void f() { log(); }
