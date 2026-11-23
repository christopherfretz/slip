#include "side.h"
// §4.3: two local classes of one name in two blocks: numbered by declaration.
#ifdef B
void f() { { struct L { long b; }; } { struct L { int a; }; } }
#else
void f() { { struct L { int a; }; } { struct L { long b; }; } }
#endif
