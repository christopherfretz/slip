#include "side.h"
// §5.5 DefinitionBits `0-literal`.
#ifdef B
struct C { constexpr ~C() {} };
#else
struct C { ~C() {} };
#endif
