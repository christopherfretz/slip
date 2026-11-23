#include "side.h"
// WP-107 (spec/landed/digest/WP-107.md): every declaration carries aligned(16): the declarations agree, determinate.
typedef int R __attribute__((aligned(16)));
typedef int R __attribute__((aligned(16)));
#ifdef B
R v;
#else
int v;
#endif
