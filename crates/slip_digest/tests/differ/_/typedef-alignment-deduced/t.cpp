#include "side.h"
// WP-107 (spec/landed/digest/WP-107.md): a deduced type keeps the typedef in Kiln and clang 19 (`alignof(decltype(av))` is 8).
typedef int I __attribute__((aligned(8)));
#ifdef B
I v;
#else
int v;
#endif
auto av = v;
