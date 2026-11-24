// rev-112b probe: an immediate-escalating generic lambda called in a default argument
#include "srcloc.h"
consteval unsigned len(const char *s) { unsigned n = 0; while (s[n]) ++n; return n; }
void take(unsigned);
void log(unsigned = [](auto s) { return len(s); }(__builtin_FUNCTION()));
void start() { log(); }
