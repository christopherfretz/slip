#include "side.h"
// §5.5 DefinitionBits `0-nontrivial-copy-ctor`.
#ifdef B
struct M { M(const M&); }; struct C { M m; };
#else
struct M { }; struct C { M m; };
#endif
