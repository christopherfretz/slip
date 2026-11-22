#include "side.h"
// §10.1 AttrNode: `syntax`: `[[gnu::cold]]` against `__attribute__((cold))`.
#ifdef B
[[gnu::cold]] void f();
#else
__attribute__((cold)) void f();
#endif
