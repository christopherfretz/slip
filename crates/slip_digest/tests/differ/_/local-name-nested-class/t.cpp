#include "side.h"
// digest §4.3 "Names a program reads": `__PRETTY_FUNCTION__` in a member
// of a class nested in a local class prints both names; renaming the
// outer one changes f.
#ifdef B
const char *f() { struct Y { struct I { const char *g() { return __PRETTY_FUNCTION__; } }; }; return Y::I().g(); }
#else
const char *f() { struct X { struct I { const char *g() { return __PRETTY_FUNCTION__; } }; }; return X::I().g(); }
#endif
