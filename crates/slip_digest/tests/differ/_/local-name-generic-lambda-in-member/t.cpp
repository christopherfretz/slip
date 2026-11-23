#include "side.h"
// digest §4.3 "Names a program reads": `__PRETTY_FUNCTION__` in a generic
// lambda inside a member of a local class: the specialisation, a unit
// under the local class, prints the class's name.
#ifdef B
const char *f() { struct Y { const char *g() { return [](auto) { return __PRETTY_FUNCTION__; }(0); } }; return Y().g(); }
#else
const char *f() { struct X { const char *g() { return [](auto) { return __PRETTY_FUNCTION__; }(0); } }; return X().g(); }
#endif
