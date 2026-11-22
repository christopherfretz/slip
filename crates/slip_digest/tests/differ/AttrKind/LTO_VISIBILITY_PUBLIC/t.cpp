#include "side.h"
// §10.1 AttrKind: `LTO_VISIBILITY_PUBLIC`: the attribute on one side only.
#ifdef B
struct [[clang::lto_visibility_public]] S {};
#else
struct S {};
#endif
