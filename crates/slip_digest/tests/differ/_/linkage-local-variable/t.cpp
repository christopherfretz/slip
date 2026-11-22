#include "side.h"
// §5.4 ScopeData rule: through an enclosing function, for a local variable.
#ifdef B
extern "C" { static void g() { int x; } }
#else
extern "C++" { static void g() { int x; } }
#endif
