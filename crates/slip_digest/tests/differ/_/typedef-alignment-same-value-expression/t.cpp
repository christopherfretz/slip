#include "side.h"
// WP-107 (spec/landed/digest/WP-107.md): two declarations whose argument is the same constant expression: Sema keeps its value, so they agree and stay determinate.
typedef int R __attribute__((aligned(alignof(long) * 2)));
typedef int R __attribute__((aligned(alignof(long) * 2)));
#ifdef B
R v;
#else
int v;
#endif
