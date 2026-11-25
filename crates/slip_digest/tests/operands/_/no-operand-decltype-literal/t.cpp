#include "side.h"
// WP-156 item 3 (S5): `decltype(0)` refers to nothing: no operand unit;
// against `int`, equal.
#ifdef B
decltype(0) v;
#else
int v;
#endif
