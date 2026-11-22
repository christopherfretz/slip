#include "side.h"
// §10.1 AttrSyntax: `Cxx11`: the fact differs.
#ifdef B
[[noreturn]] void f();
#else
__attribute__((noreturn)) void f();
#endif
