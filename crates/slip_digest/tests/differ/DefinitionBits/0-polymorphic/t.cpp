#include "side.h"
// §5.5 DefinitionBits `0-polymorphic`.
#ifdef B
struct C { virtual void f(); };
#else
struct C { void f(); };
#endif
