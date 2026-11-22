#include "side.h"
// §5.5 DefinitionBits `0-needs-ovl-move-ctor`.
#ifdef B
struct M {}; struct C { const M m; };
#else
struct M {}; struct C { M m; };
#endif
