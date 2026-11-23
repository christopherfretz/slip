#include "side.h"
// §5.3 DeclNode `CXXDestructor.data`: `virtual`.
#ifdef B
struct S { virtual ~S(); };
#else
struct S { ~S(); };
#endif
