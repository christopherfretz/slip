#include "side.h"
// §5.3 DeclNode `CXXMethod.data`: `virtual`.
#ifdef B
struct S { virtual void f(); };
#else
struct S { void f(); };
#endif
