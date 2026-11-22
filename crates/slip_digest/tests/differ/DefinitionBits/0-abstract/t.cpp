#include "side.h"
// §5.5 DefinitionBits `0-abstract`.
#ifdef B
struct C { virtual void f() = 0; };
#else
struct C { virtual void f(); };
#endif
