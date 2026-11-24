// rev-112b probe: a consteval lambda's call in a default argument
#include "srcloc.h"
consteval unsigned len(const char *s) { unsigned n = 0; while (s[n]) ++n; return n; }
void take(unsigned);
void log(unsigned = [](const char *s) consteval { return len(s); }(__builtin_FUNCTION()));
void start() { log(); }
