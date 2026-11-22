#include "side.h"
// §10.1 AttrNode: `FALLTHROUGH.flags-scoped`: only the spelling differs.
#ifdef B
void f(int x) { switch (x) { case 0: [[clang::fallthrough]]; case 1: break; } }
#else
void f(int x) { switch (x) { case 0: [[fallthrough]]; case 1: break; } }
#endif
