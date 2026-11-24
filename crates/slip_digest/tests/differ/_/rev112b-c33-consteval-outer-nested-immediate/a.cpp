// rev-112b probe: an immediate invocation in f's body whose default holds another immediate invocation of the builtin (clang: 1 against 5)
#include "srcloc.h"
consteval unsigned len(const char *s) { unsigned n = 0; while (s[n]) ++n; return n; }
void take(unsigned);
consteval unsigned outer(unsigned n = len(__builtin_FUNCTION())) { return n; }
void f() { take(outer()); }
