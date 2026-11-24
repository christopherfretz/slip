// rev-112b probe: a namespace-scope lambda's default argument used in f
#include "srcloc.h"
consteval unsigned len(const char *s) { unsigned n = 0; while (s[n]) ++n; return n; }
void take(unsigned);
inline auto lg = [](unsigned n = len(__builtin_FUNCTION())) { take(n); };
void start() { lg(); }
