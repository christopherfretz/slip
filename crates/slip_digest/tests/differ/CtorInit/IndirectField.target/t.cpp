#include "side.h"
// §5.5 CtorInit `IndirectField.target`.
#ifdef B
struct C { union { int a; int b; }; C() : b(1) {} };
#else
struct C { union { int a; int b; }; C() : a(1) {} };
#endif
