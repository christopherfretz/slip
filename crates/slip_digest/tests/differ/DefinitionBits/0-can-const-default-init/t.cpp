#include "side.h"
// §5.5 DefinitionBits `0-can-const-default-init`.
#ifdef B
struct C { int a = 0; };
#else
struct C { int a; };
#endif
