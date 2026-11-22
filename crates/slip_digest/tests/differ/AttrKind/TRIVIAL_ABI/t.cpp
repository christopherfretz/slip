#include "side.h"
// §10.1 AttrKind: `TRIVIAL_ABI`: the attribute on one side only.
#ifdef B
struct [[clang::trivial_abi]] S { S(S &&); ~S(); };
#else
struct S { S(S &&); ~S(); };
#endif
