#include "side.h"
// §5.5 CtorInit `written_index`: the order written is not the order of initialisation.
#ifdef B
struct C { int a, b; C() : b(2), a(1) {} };
#else
struct C { int a, b; C() : a(1), b(2) {} };
#endif
