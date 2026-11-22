#include "side.h"
// §5.4 FnSpec `0-constexpr-kind`.
#ifdef B
consteval int f() { return 1; }
#else
constexpr int f() { return 1; }
#endif
