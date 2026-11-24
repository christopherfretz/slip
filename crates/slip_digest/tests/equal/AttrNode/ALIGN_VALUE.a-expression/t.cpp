#include "side.h"
// §10.1 AttrNode: `ALIGN_VALUE.a-expression`: the same expression, written
// apart by a parameter's name and spacing, is equal (WP-066 review F1).
#ifdef B
template <int M> void f(int *__attribute__((align_value( M ))) p) {}
#else
template <int N> void f(int *__attribute__((align_value(N))) p) {}
#endif
