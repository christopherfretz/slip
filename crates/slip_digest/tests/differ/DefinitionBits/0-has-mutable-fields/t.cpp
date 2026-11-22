#include "side.h"
// §5.5 DefinitionBits `0-has-mutable-fields`.
#ifdef B
struct M { mutable int x; }; struct C { M m; };
#else
struct M { int x; }; struct C { M m; };
#endif
