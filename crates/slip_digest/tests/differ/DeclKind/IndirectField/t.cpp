#include "side.h"
// §5.3 DeclKind `IndirectField`: kind changed: a field against an indirect field of one key.
#ifdef B
struct S { union { int a; }; };
#else
struct S { int a; };
#endif
