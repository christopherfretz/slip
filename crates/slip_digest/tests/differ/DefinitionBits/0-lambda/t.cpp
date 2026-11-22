#include "side.h"
// §5.5 DefinitionBits `0-lambda`: the closure type is a local type of `l`'s unit.
#ifdef B
int x; auto l = [] { return 0; };
#else
int x; auto l = 0;
#endif
