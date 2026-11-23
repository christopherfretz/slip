#include "side.h"
// §10.2 DeclNameKind `Conversion`: a conversion function against an identifier.
#ifdef B
struct S { operator int(); };
#else
struct S { int f(); };
#endif
