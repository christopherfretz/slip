#include "side.h"
// §10.1 AttrKind: `NO_SPLIT_STACK`: the attribute on one side only.
#ifdef B
__attribute__((no_split_stack)) void f();
#else
void f();
#endif
