#include "side.h"
// WP-107 (spec/landed/digest/WP-107.md): two declarations with the same two `aligned` requests in another order: the same set, so they agree and stay determinate.
typedef int R __attribute__((aligned(8), aligned(16)));
typedef int R __attribute__((aligned(16), aligned(8)));
#ifdef B
R v;
#else
int v;
#endif
