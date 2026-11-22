#include "side.h"
// §5.5 DefinitionBits `0-needs-ovl-move-assign`.
#ifdef B
struct M {}; struct C { volatile M m; };
#else
struct M {}; struct C { M m; };
#endif
