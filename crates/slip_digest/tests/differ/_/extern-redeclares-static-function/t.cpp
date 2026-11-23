#include "side.h"
// `extern` on a redeclaration of a function first declared `static`
// keeps its internal linkage ([dcl.stc]/7): the linkage is equal, but
// the written storage class differs (§5.4 FnStorage, kept as written).
static void f();
#ifdef B
extern void f() {}
#else
void f() {}
#endif
