#include "side.h"
// §10.1 AttrKind: `NO_STACK_PROTECTOR`: the attribute on one side only.
#ifdef B
__attribute__((no_stack_protector)) void f();
#else
void f();
#endif
