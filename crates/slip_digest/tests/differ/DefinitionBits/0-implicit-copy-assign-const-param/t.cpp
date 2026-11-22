#include "side.h"
// §5.5 DefinitionBits `0-implicit-copy-assign-const-param`.
#ifdef B
struct M { M& operator=(M&); }; struct C { M m; };
#else
struct M { M& operator=(const M&); }; struct C { M m; };
#endif
