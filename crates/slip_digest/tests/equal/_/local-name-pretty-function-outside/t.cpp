#include "side.h"
// digest §4.3 "Names a program reads": `__PRETTY_FUNCTION__` in the owner
// itself does not print its local classes: renaming one is equal.
#ifdef B
const char *f() { struct Y { int m; }; Y y{1}; return y.m ? __PRETTY_FUNCTION__ : ""; }
#else
const char *f() { struct X { int m; }; X x{1}; return x.m ? __PRETTY_FUNCTION__ : ""; }
#endif
