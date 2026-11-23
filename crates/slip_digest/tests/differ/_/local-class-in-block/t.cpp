#include "side.h"
// §4.3: a block added around a local class: f's stream changes, its key stays local(0) under f, so g, which names it, does not.
#ifdef B
auto f() { { struct L { int m; }; return L{1}; } }
#else
auto f() { struct L { int m; }; return L{1}; }
#endif
int g() { return f().m; }
