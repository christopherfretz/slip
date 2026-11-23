#include "side.h"
// §5.3 DeclNode `CXXConstructor.data`: `explicit`.
#ifdef B
struct S { explicit S(int); };
#else
struct S { S(int); };
#endif
