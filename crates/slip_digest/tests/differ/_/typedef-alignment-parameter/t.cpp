#include "side.h"
// WP-107 (spec/landed/digest/WP-107.md): a parameter of the typedef (its `__alignof__` reads it).
typedef int I __attribute__((aligned(8)));
#ifdef B
void f(I p) { }
#else
void f(int p) { }
#endif
