#include "side.h"
// §5.3 DeclKind `CXXDestructor`: added.
#ifdef B
struct S { ~S(); };
#else
struct S { };
#endif
