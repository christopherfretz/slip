#include "side.h"
// §5.3 DeclKind `AccessSpec`: access is written by each member.
#ifdef B
struct S { public: int a; };
#else
struct S { int a; };
#endif
