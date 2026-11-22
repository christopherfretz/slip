#include "side.h"
// §5.5 DefinitionBits `0-structural`.
#ifdef B
struct C { mutable int a; };
#else
struct C { int a; };
#endif
