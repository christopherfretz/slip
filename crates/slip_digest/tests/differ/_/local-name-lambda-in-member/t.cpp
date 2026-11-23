#include "side.h"
// digest §4.3 "Names a program reads": `__PRETTY_FUNCTION__` in a lambda
// inside a member of a local class prints the class's name.
#ifdef B
const char *f() { struct Y { const char *g() { return [] { return __PRETTY_FUNCTION__; }(); } }; return Y().g(); }
#else
const char *f() { struct X { const char *g() { return [] { return __PRETTY_FUNCTION__; }(); } }; return X().g(); }
#endif
