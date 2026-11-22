#include "side.h"
// args: -fcf-protection=branch
// §10.1 AttrKind `NOCF_CHECK`: against `cold`.
#ifdef B
__attribute__((cold)) void f();
#else
__attribute__((nocf_check)) void f();
#endif
