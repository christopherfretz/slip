#include "side.h"
// §10.1 AttrKind: `NO_BUILTIN`: the attribute on one side only.
#ifdef B
__attribute__((no_builtin("memcpy"))) void f() {};
#else
void f() {};
#endif
