#include "side.h"
// §10.2 DeclName `Conversion.0-payload`: the type.
#ifdef B
struct S { operator long(); };
#else
struct S { operator int(); };
#endif
