#include "side.h"
// §5.7 ColdDeclData `explicit_expr`, read from the declaration that has it: the constructor defined outside its class, where `explicit` cannot be written (WP-033 review, item 12).
#ifdef B
struct S { explicit(true) S(int); };
#else
struct S { explicit S(int); };
#endif
S::S(int) {}
