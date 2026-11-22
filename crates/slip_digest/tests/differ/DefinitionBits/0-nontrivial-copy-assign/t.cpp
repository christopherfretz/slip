#include "side.h"
// §5.5 DefinitionBits `0-nontrivial-copy-assign`.
#ifdef B
struct M { M& operator=(const M&); }; struct C { M m; };
#else
struct M { }; struct C { M m; };
#endif
