#include "side.h"
// §2.3, WP-026: `init`, `0-init-style` and `eval` come from the declaration
//  of the chain that holds the initialiser, here the one in the class,
//  while the out-of-line definition is the declaration read.
#ifdef B
struct S { static const int x{1}; };
const int S::x;
#else
struct S { static const int x = 1; };
const int S::x;
#endif
