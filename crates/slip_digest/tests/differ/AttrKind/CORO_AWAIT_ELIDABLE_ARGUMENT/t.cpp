#include "side.h"
// §10.1 AttrKind: `CORO_AWAIT_ELIDABLE_ARGUMENT`: the attribute on one side only.
#ifdef B
void f([[clang::coro_await_elidable_argument]] int x);
#else
void f(int x);
#endif
