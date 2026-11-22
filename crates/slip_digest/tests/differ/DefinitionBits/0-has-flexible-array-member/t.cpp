#include "side.h"
// §5.5 DefinitionBits `0-has-flexible-array-member`.
#ifdef B
struct C { int n; int a[]; };
#else
struct C { int n; int a[1]; };
#endif
