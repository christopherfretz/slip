#include "side.h"
// §5.5 DefinitionBits `0-aggregate`.
#ifdef B
struct C { int a; C(int); };
#else
struct C { int a; };
#endif
