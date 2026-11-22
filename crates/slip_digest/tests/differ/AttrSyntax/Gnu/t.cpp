#include "side.h"
// §10.1 AttrSyntax: `Gnu`: the fact differs.
#ifdef B
__attribute__((cold)) void f();
#else
[[gnu::cold]] void f();
#endif
