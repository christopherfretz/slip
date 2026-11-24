// rev-112b probe: the consumer is in a nested default: log's default calls g, whose default consumes; g also used alone first by an unrenamed function
#include "srcloc.h"
consteval unsigned len(const char *s) { unsigned n = 0; while (s[n]) ++n; return n; }
void take(unsigned);
constexpr unsigned g(unsigned n = len(__builtin_FUNCTION())) { return n; }
void log(unsigned = g());
void other() { take(g()); log(); }
void f() { log(); take(g()); }
