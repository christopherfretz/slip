#include "side.h"
// §5.7 ColdDeclData `explicit_expr`, read from the declaration that has it: two expressions of the same value, the constructor defined outside its class, where `explicit` cannot be written (WP-033 review, item 12).
#ifdef B
struct S { explicit(1 == 1) S(int); };
#else
struct S { explicit(true) S(int); };
#endif
S::S(int) {}
