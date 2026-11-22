#include "side.h"
// §5.5 DefinitionBits `0-lambda-captures`: the closure type is a local type of `l`'s unit.
#ifdef B
int x; auto l = [x] { return x; };
#else
int x; auto l = [] { return 0; };
#endif
