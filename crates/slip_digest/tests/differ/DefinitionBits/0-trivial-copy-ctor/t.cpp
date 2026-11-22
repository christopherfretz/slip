#include "side.h"
// §5.5 DefinitionBits `0-trivial-copy-ctor`.
#ifdef B
struct C { C(const C&) {} };
#else
struct C { C(const C&) = default; };
#endif
