#include "side.h"
// §5.5 DefinitionBits `0-has-in-class-init`.
#ifdef B
struct C { int a = 1; };
#else
struct C { int a; };
#endif
