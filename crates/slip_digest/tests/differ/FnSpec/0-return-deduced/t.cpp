#include "side.h"
// §5.4 FnSpec `0-return-deduced`: the type is not in the key; the bit is.
#ifdef B
auto f() { return 1; }
#else
int f() { return 1; }
#endif
