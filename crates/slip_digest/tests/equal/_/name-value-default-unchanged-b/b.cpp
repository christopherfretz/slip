// WP-112 R2 (b): std::source_location::current() passed on (digest §9), in a default argument and a default member initializer, used by a function the rule does not move: equal, no note
#include "srcloc.h"
void take(const char *);
void log(std::source_location = std::source_location::current());
struct Agg { int x; std::source_location l = std::source_location::current(); };
void g() { log(); Agg a{1}; take(a.l.function_name()); }
namespace acme { void unrelated(); }
