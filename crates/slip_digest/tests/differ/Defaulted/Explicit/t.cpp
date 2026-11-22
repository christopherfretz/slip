#include "side.h"
// §5.4 Defaulted `Explicit`: defaulted on a later declaration.
#ifdef B
struct C { C(); }; C::C() = default;
#else
struct C { C(); }; C::C() {}
#endif
