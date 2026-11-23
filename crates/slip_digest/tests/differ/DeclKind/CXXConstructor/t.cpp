#include "side.h"
// §5.3 DeclKind `CXXConstructor`: added.
#ifdef B
struct S { S(int); };
#else
struct S { };
#endif
