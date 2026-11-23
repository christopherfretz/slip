#include "side.h"
// WP-107 (spec/landed/digest/WP-107.md): a field of type `decltype(*p)`: the pointer `p` is not walked, the field's type is.
typedef int I __attribute__((aligned(8)));
#ifdef B
I *p;
#else
int *p;
#endif
struct S { decltype(*p) r; };
