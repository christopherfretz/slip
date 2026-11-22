#include "side.h"
// §5.5 DefinitionBits `0-trivial-abi`.
#ifdef B
struct [[clang::trivial_abi]] C { C(C&&); ~C(); };
#else
struct C { C(C&&); ~C(); };
#endif
