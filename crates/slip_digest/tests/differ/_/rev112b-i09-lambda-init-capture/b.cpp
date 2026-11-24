// rev-112b probe: a lambda's init-capture in a default argument consumes the caller's name
#include "srcloc.h"
consteval unsigned len(const char *s) { unsigned n = 0; while (s[n]) ++n; return n; }
void take(unsigned);
void log(unsigned = [n = len(__builtin_FUNCTION())] { return n; }());
void start() { log(); }
