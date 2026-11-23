#include "side.h"
// §5.3 DeclKind `CXXMethod`: added (a member function against a function cannot have one key).
#ifdef B
struct S { void f(); };
#else
struct S { };
#endif
