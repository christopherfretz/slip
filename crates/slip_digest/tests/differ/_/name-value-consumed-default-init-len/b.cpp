// WP-112 R2 (c) (digest §9): __builtin_FUNCTION() in a default member initializer is the operand of the immediate invocation len(...), which consumes it at the use (an aggregate initialization): the user's plain name is written by value in the user, and its length differs under the rename
#include "srcloc.h"
consteval unsigned len(const char *s) { unsigned n = 0; while (s[n]) ++n; return n; }
void take(unsigned);
struct Agg { int x; unsigned n = len(__builtin_FUNCTION()); };
void start() { Agg a{1}; take(a.n); }
