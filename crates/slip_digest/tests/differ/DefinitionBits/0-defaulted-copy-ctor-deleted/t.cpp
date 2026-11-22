#include "side.h"
// §5.5 DefinitionBits `0-defaulted-copy-ctor-deleted`.
#ifdef B
struct C { int&& r; };
#else
struct C { int& r; };
#endif
