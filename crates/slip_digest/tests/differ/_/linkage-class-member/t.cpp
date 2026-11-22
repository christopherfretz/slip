#include "side.h"
// §5.4 ScopeData rule: through an enclosing class.
#ifdef B
extern "C" { struct C { void f(); }; }
#else
struct C { void f(); };
#endif
