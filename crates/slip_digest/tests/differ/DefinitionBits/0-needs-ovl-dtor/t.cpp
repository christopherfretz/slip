#include "side.h"
// §5.5 DefinitionBits `0-needs-ovl-dtor`.
#ifdef B
struct M { ~M(); }; struct C { M m; };
#else
struct M { }; struct C { M m; };
#endif
