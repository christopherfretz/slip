// WP-112 R2 (c): consumed by len(...) (digest §9), in a default argument and a default member initializer, used by a function the rule does not move: equal, no note
#include "srcloc.h"
void take(unsigned);
consteval unsigned len(const char *s) { unsigned n = 0; while (s[n]) ++n; return n; }
void log(unsigned = len(__builtin_FUNCTION()));
struct Agg { int x; unsigned n = len(std::source_location::current().function_name()); };
void g() { log(); Agg a{1}; take(a.n); }
namespace legacy { void unrelated(); }
