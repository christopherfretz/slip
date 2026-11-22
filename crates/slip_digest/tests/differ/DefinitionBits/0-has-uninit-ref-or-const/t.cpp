#include "side.h"
// §5.5 DefinitionBits `0-has-uninit-ref-or-const`.
#ifdef B
struct C { const int a; };
#else
struct C { const int a = 1; };
#endif
