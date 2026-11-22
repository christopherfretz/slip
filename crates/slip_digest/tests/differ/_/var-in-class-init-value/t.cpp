#include "side.h"
// §2.3, WP-026: the in-class initialiser's value is in the unit of a
//  member defined out of line.
#ifdef B
struct S { static const int x = 2; };
const int S::x;
#else
struct S { static const int x = 1; };
const int S::x;
#endif
