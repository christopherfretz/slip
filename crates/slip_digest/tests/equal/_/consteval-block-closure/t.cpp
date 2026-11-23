// std: c++26
#include "side.h"
// digest §4.3: a `consteval` block's closure is owned by the block; a
// local of its body renamed leaves the block's content, its key, equal.
#ifdef B
consteval { int b = 1; (void)b; }
#else
consteval { int a = 1; (void)a; }
#endif
