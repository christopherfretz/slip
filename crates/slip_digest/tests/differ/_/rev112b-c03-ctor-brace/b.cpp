// rev-112b probe: consteval constructor called with braces in a default argument
#include "srcloc.h"
consteval unsigned len(const char *s) { unsigned n = 0; while (s[n]) ++n; return n; }
void take(unsigned);
struct L { unsigned n; consteval L(const char *s) : n(len(s)) {} };
void log(L = L{__builtin_FUNCTION()});
void start() { log(); }
