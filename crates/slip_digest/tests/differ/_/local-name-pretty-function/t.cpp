#include "side.h"
// digest §4.3 "Names a program reads": `__PRETTY_FUNCTION__` in a member
// of a local class prints the class's name; renaming it changes f.
#ifdef B
const char *f() { struct Y { const char *g() { return __PRETTY_FUNCTION__; } }; return Y().g(); }
#else
const char *f() { struct X { const char *g() { return __PRETTY_FUNCTION__; } }; return X().g(); }
#endif
