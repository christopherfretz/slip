#include "side.h"
// §5.5 DefinitionBits `0-final`.
#ifdef B
struct C final {};
#else
struct C {};
#endif
