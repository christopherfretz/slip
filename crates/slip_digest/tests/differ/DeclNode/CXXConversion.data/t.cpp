#include "side.h"
// §5.3 DeclNode `CXXConversion.data`: `explicit`.
#ifdef B
struct S { explicit operator int(); };
#else
struct S { operator int(); };
#endif
