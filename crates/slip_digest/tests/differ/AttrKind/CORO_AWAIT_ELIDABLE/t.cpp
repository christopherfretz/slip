#include "side.h"
// §10.1 AttrKind: `CORO_AWAIT_ELIDABLE`: the attribute on one side only.
#ifdef B
struct [[clang::coro_await_elidable]] S {};
#else
struct S {};
#endif
