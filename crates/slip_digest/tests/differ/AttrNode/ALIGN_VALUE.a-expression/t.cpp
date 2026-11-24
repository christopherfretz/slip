#include "side.h"
// §10.1 AttrNode: `ALIGN_VALUE.a-expression`: the fact differs (WP-066 review F1).
#ifdef B
template <int N> void f(int *__attribute__((align_value(N * 2))) p) {}
#else
template <int N> void f(int *__attribute__((align_value(N))) p) {}
#endif
