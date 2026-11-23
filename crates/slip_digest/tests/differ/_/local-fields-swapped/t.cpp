#include "side.h"
// §4.3, §5.5: a local class's fields stay in their order.
#ifdef B
void f() { struct L { long b; int a; }; }
#else
void f() { struct L { int a; long b; }; }
#endif
