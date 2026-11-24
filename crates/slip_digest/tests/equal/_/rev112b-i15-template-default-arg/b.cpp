// rev-112b probe: a default template argument holding the builtin under an immediate invocation, used by f
#include "srcloc.h"
consteval unsigned len(const char *s) { unsigned n = 0; while (s[n]) ++n; return n; }
void take(unsigned);
template<unsigned N = len(__builtin_FUNCTION())> void log();
void start() { log<>(); }
