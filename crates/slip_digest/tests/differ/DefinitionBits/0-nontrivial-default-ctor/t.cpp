#include "side.h"
// §5.5 DefinitionBits `0-nontrivial-default-ctor`.
#ifdef B
struct C { int a = 1; };
#else
struct C { int a; };
#endif
