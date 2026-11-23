#include "side.h"
// digest §4.3: the first conversion of a closure to a function pointer
// defines its conversion function; `mk` does not change.
auto mk() { return [] { return 1; }; }
int a() { return mk()(); }
#ifdef B
int c() { int (*p)() = mk(); return p(); }
#endif
