// std: c++23
#include "side.h"
// §6 IfKind `Consteval`.
#ifdef B
void f() { if consteval {} }
#else
void f() { if !consteval {} }
#endif
