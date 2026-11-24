// WP-112 R2 (c) (digest §9): __builtin_FUNCTION() in a default argument is the operand of the immediate invocation len(...), which consumes it at the use: the caller's plain name is written by value in the caller, and its length differs under the rename
#include "srcloc.h"
consteval unsigned len(const char *s) { unsigned n = 0; while (s[n]) ++n; return n; }
void log(unsigned = len(__builtin_FUNCTION()));
void start() { log(); }
