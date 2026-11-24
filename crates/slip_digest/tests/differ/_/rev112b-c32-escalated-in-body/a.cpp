// rev-112b probe: an immediate-escalating lambda called in f's body with the builtin as its argument
#include "srcloc.h"
consteval unsigned len(const char *s) { unsigned n = 0; while (s[n]) ++n; return n; }
void take(unsigned);
void f() { take([](const char *s) { return len(s); }(__builtin_FUNCTION())); }
