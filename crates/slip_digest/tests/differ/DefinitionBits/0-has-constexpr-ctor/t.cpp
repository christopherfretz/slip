#include "side.h"
// §5.5 DefinitionBits `0-has-constexpr-ctor`.
#ifdef B
struct C { constexpr C(int) {} };
#else
struct C { C(int) {} };
#endif
