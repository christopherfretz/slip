#include "side.h"
// digest §4.3 "Names a program reads": `__builtin_FUNCTION()` in a local
// class's constructor is the class's name.
#ifdef B
const char *f() { struct Y { const char *n; Y() : n(__builtin_FUNCTION()) {} }; return Y().n; }
#else
const char *f() { struct X { const char *n; X() : n(__builtin_FUNCTION()) {} }; return X().n; }
#endif
