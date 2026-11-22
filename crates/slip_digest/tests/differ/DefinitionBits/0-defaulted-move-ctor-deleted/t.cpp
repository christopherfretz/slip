#include "side.h"
// §5.5 DefinitionBits `0-defaulted-move-ctor-deleted`.
#ifdef B
struct M { M(M&&); }; union C { int i; M m; };
#else
struct M { M(M&&); }; union C { int i; };
#endif
