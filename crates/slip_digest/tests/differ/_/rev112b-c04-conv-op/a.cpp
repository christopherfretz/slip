// rev-112b probe: consteval conversion function applied implicitly in a default argument
#include "srcloc.h"
consteval unsigned len(const char *s) { unsigned n = 0; while (s[n]) ++n; return n; }
void take(unsigned);
struct N { const char *s; consteval operator unsigned() const { return len(s); } };
void log(unsigned = N{__builtin_FUNCTION()});
void f() { log(); }
