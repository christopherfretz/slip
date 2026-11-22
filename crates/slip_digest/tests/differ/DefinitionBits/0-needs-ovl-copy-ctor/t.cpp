#include "side.h"
// §5.5 DefinitionBits `0-needs-ovl-copy-ctor`.
#ifdef B
struct M {}; struct C { mutable M m; };
#else
struct M {}; struct C { M m; };
#endif
