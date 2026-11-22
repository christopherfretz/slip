#include "side.h"
// §5.5 DefinitionBits `0-defaulted-dtor-deleted`.
#ifdef B
struct M { ~M(); }; union C { int i; M m; };
#else
struct M { }; union C { int i; M m; };
#endif
