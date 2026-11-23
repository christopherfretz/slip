#include "side.h"
// WP-107 (spec/landed/digest/WP-107.md): every declaration carries the alignment: determinate.
typedef int R __attribute__((aligned(8)));
typedef int R __attribute__((aligned(8)));
#ifdef B
R v;
#else
int v;
#endif
