#include "side.h"
// §10.1 AttrKind: `FALLTHROUGH`: the attribute on one side only.
#ifdef B
void f(int x) { switch (x) { case 0: [[fallthrough]]; case 1: break; } }
#else
void f(int x) { switch (x) { case 0: case 1: break; } }
#endif
