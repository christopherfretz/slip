// rev-112b probe: a constructor's default argument used by a declaration C c;
#include "srcloc.h"
consteval unsigned len(const char *s) { unsigned n = 0; while (s[n]) ++n; return n; }
void take(unsigned);
struct C { C(unsigned = len(__builtin_FUNCTION())); };
void f() { C c; }
