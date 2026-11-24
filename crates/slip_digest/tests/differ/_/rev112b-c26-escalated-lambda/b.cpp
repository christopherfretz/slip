// rev-112b probe: an immediate-escalating lambda (constexpr, calls len on its parameter: P2564) called in a default argument is an immediate invocation that consumes the caller's name
#include "srcloc.h"
consteval unsigned len(const char *s) { unsigned n = 0; while (s[n]) ++n; return n; }
void take(unsigned);
void log(unsigned = [](const char *s) { return len(s); }(__builtin_FUNCTION()));
void start() { log(); }
