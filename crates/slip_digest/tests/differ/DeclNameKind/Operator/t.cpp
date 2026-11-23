#include "side.h"
// §10.2 DeclNameKind `Operator`: an operator against an identifier.
#ifdef B
struct S { void operator()(); };
#else
struct S { void f(); };
#endif
