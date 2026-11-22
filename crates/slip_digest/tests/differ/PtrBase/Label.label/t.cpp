#include "side.h"
// §8 PtrBase `Label.label`: the address of the first label against the second.
#ifdef B
void f() { l: m: constexpr void* p = &&m; }
#else
void f() { l: m: constexpr void* p = &&l; }
#endif
