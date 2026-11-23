#include "side.h"
// digest §4.3 "Names a program reads": `__func__` in a local class's
// constructor is the class's name.
#ifdef B
const char *f() { struct Y { const char *n; Y() : n(__func__) {} }; return Y().n; }
#else
const char *f() { struct X { const char *n; X() : n(__func__) {} }; return X().n; }
#endif
