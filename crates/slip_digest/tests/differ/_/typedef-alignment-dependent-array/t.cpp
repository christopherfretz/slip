#include "side.h"
// WP-107 (spec/landed/digest/WP-107.md): a dependent-sized array of the typedef in a class template.
typedef long L __attribute__((aligned(4)));
#ifdef B
template <int N> struct T { char c; L a[N]; };
#else
template <int N> struct T { char c; long a[N]; };
#endif
static_assert(sizeof(T<2>) == (alignof(T<2>) == 4 ? 20 : 24));
