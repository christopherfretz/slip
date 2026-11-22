#include "side.h"
// §10.1 AttrKind: `FORCE_ALIGN_ARG_POINTER`: the attribute on one side only.
#ifdef B
__attribute__((force_align_arg_pointer)) void f();
#else
void f();
#endif
