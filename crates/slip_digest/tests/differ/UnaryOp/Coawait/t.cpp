#include "side.h"
// §7.2 UnaryOp `Coawait`.
#ifdef B
int v = 1;
#else
// `co_await` is reached only in a coroutine's body.
int v = 0;
#endif
