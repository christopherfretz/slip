// WP-112 R2 (a): __builtin_FUNCTION() and __builtin_source_location() passed on (digest §9), in a default argument and a default member initializer, used by a function the rule does not move: equal, no note
#include "srcloc.h"
void take(const char *);
void log(const char * = __builtin_FUNCTION());
struct Agg { int x; const void *p = __builtin_source_location(); };
void g() { log(); Agg a{1}; take(static_cast<const char *>(a.p)); }
namespace acme { void unrelated(); }
