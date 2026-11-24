// rev-112b probe: an implicit conversion through a constructor whose second parameter's default consumes
#include "srcloc.h"
consteval unsigned len(const char *s) { unsigned n = 0; while (s[n]) ++n; return n; }
void take(unsigned);
struct C { C(int, unsigned = len(__builtin_FUNCTION())); };
void eat(C);
void start() { eat(1); }
