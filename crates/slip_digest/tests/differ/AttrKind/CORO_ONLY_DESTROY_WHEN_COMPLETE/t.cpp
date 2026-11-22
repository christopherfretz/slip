#include "side.h"
// §10.1 AttrKind: `CORO_ONLY_DESTROY_WHEN_COMPLETE`: the attribute on one side only.
#ifdef B
struct [[clang::coro_only_destroy_when_complete]] S {};
#else
struct S {};
#endif
