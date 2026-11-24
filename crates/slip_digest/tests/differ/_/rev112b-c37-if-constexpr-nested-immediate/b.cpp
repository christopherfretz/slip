// rev-112b probe: an if constexpr condition g() == 1 whose default holds an immediate invocation of the builtin
#include "srcloc.h"
consteval unsigned len(const char *s) { unsigned n = 0; while (s[n]) ++n; return n; }
void take(unsigned);
constexpr unsigned g(unsigned n = len(__builtin_FUNCTION())) { return n; }
void start() { if constexpr (g() == 1) take(1); else take(2); }
