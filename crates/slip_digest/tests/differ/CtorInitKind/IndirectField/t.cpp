#include "side.h"
// §5.5 CtorInitKind `IndirectField`.
#ifdef B
struct C { union { int a; }; C() : a(1) {} };
#else
struct C { union { int a; }; C() {} };
#endif
