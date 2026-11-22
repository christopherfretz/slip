#include "side.h"
// §5.5 DefinitionBits `0-has-virtual-bases`.
#ifdef B
struct P {}; struct C : virtual P {};
#else
struct P {}; struct C : P {};
#endif
