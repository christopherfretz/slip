#include "side.h"
// §5.5 DefinitionBits `0-implicit-copy-ctor-const-param`.
#ifdef B
struct M { M(M&); }; struct C { M m; };
#else
struct M { M(const M&); }; struct C { M m; };
#endif
