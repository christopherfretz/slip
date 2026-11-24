#include "side.h"
// §10.1 AttrNode: `ALIGN_VALUE.flags-value-dependent`: the fact differs (WP-066 review F1).
#ifdef B
template <int N> void f(int *__attribute__((align_value(N))) p) {}
#else
template <int N> void f(int *__attribute__((align_value(16))) p) {}
#endif
