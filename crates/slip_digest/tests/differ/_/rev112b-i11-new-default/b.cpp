// rev-112b probe: operator new with a defaulted extra parameter used by a plain new-expression
#include "srcloc.h"
consteval unsigned len(const char *s) { unsigned n = 0; while (s[n]) ++n; return n; }
void take(unsigned);
struct S { static void *operator new(decltype(sizeof 0), unsigned = len(__builtin_FUNCTION())); };
void start() { S *p = new S; (void)p; }
