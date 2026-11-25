#include "side.h"
// WP-156 item 3 (S5): `array<char, 4>` holds a literal: no reference, no
// operand unit. Side B writes `2 + 2`: still none.
template <class T, unsigned long N> struct array {};
#ifdef B
array<char, 2 + 2> a;
#else
array<char, 4> a;
#endif
