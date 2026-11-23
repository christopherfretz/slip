#include "side.h"
// §5.3 DeclKind `Friend`: added.
#ifdef B
struct S { friend void f(); };
#else
struct S { };
#endif
