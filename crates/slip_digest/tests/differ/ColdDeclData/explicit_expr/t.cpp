#include "side.h"
// §5.7 ColdDeclData `explicit_expr`: `explicit(true)` against `explicit`.
#ifdef B
struct S { explicit(true) S(int); };
#else
struct S { explicit S(int); };
#endif
