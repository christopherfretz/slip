#include "side.h"
// §8 PtrBase `Label`: a label's address against a null pointer.
#ifdef B
void f() { l: constexpr void* p = nullptr; }
#else
void f() { l: constexpr void* p = &&l; }
#endif
