#include "side.h"
// §7.2 CastKind `FunctionToPointerDecay`.
void fn();
#ifdef B
void (*v)() = &fn;
#else
void (*v)() = fn;
#endif
