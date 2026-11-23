#include "side.h"
// §5.3 DeclNode `Friend.data`: the entity befriended.
#ifdef B
struct S { friend void g(); };
#else
struct S { friend void f(); };
#endif
