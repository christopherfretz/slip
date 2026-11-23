#include "side.h"
// WP-107 (spec/landed/digest/WP-107.md): a load through a pointer to an under-aligned typedef (clang 19 loads with align 1): the pointer is not walked, the parameter's type and `*q`'s type are written with sugar.
typedef int UI __attribute__((aligned(1)));
#ifdef B
int g(UI *q) { return *q; }
#else
int g(int *q) { return *q; }
#endif
