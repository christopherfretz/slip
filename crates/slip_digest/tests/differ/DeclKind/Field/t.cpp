#include "side.h"
// §5.3 DeclKind `Field`: added.
#ifdef B
struct S { int a; int b; };
#else
struct S { int a; };
#endif
