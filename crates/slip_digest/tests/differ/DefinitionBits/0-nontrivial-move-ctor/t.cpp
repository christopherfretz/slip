#include "side.h"
// §5.5 DefinitionBits `0-nontrivial-move-ctor`.
#ifdef B
struct M { M(M&&); }; struct C { M m; };
#else
struct M { }; struct C { M m; };
#endif
