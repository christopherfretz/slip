#include "side.h"
// §5.5 DefinitionBits `0-has-variant-members`.
#ifdef B
struct C { union { int a; }; };
#else
struct C { struct { int a; }; };
#endif
