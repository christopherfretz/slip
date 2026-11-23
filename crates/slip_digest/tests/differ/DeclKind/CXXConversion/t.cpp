#include "side.h"
// §5.3 DeclKind `CXXConversion`: added.
#ifdef B
struct S { operator int(); };
#else
struct S { };
#endif
